# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 프로젝트 개요

MyDocDesk — crowdworks.ai 의 Alpy 같은 **개인용 문서 AI 데스크톱 앱** (Windows/macOS 설치형).
Rust + eframe/egui 0.31 단일 바이너리. 기본은 **로컬 전용**으로 동작하고, 같은 계정의 PC·Mac 끼리만 데이터를 동기화하며,
파일 공유는 별도 공유 페이지로 한다. 브랜드·로고·문구는 복제하지 않는다 (기능만 재구현).

- 소스는 전부 `app/` 안에 있다. 저장소 루트의 `agent-session.jsonl`, `craft.json`, `feed.json`, `build-activity.log`,
  `checks/`, `investigation.cany/`, `inbox/`, `attachments/` 등은 이 앱을 처음 만든 빌드 에이전트의 기록물이다 — 소스가 아니므로 고치지 않는다.
- 빌드 에이전트 기록물과 `app/target/` 은 `.gitignore` 로 git 에서 제외돼 있다.

## 명령어 (모두 `app/` 에서 실행)

```bash
cargo build                         # 디버그 빌드 → target/debug/mydocdesk(.exe)
cargo build --release
cargo test                          # 단위 테스트 (현재 12개 통과, 3개 ignored)
cargo test sync::tests              # 모듈 하나만
cargo test two_devices_share_one_account   # 테스트 하나만 (이름 일부로 필터)
cargo bundle --release              # cargo-bundle: macOS .app / Windows .msi

# 실제 AI(구독 CLI·로컬 AI)를 부르는 라이브 테스트 — 구독 사용량을 조금 쓴다
MDD_LIVE=1 cargo test live_ -- --ignored --nocapture
```

### 스크린샷 모드 (화면 확인용, 필수 기능)

```bash
./target/debug/mydocdesk.exe --screenshot shots/home.png --page home
MDD_THEME=dark MDD_LANG=vi ./target/debug/mydocdesk.exe --screenshot shots/vi-home.png --page home
```

- `--screenshot` 이면 `Store::demo()`(읽기 전용, 아무것도 저장 안 함) + `demo::demo()` 가짜 데이터로 뜨고,
  120프레임 뒤 PNG 를 저장하고 스스로 닫힌다. 실제 사용자 데이터는 건드리지 않는다.
- `--page` 이름은 `app.rs` 의 `Page::from_name` 참고 (`home library pipeline analyze analyze-setup analyze-running ask ask-selected agents eval share ai sync settings`).
- 화면을 바꾸면 이 모드로 PNG 를 찍어 직접 보고 확인한다. 기존 결과물은 `app/shots/`.

## 아키텍처

### 한 개의 거대한 상태 객체 + 페이지 메서드

- `main.rs` → `app::DeskApp` 하나가 모든 UI 상태를 들고 있다 (`app.rs`, 약 2,700줄).
- 페이지마다 `page_xxx(&mut self, ui)` 메서드. 일부 페이지는 별도 파일이지만 **`app` 의 자식 모듈**이다:
  `app.rs` 안에서 `#[path = "analyze_page.rs"] mod analyze_page;` 식으로 붙인다
  (`analyze_page.rs`, `pipeline_page.rs`, `eval_ai.rs`, `ai_page.rs`). 그래서 이 파일들은 `main.rs` 의 `mod` 목록에 없고,
  `impl DeskApp { ... }` 블록으로 `DeskApp` 의 private 필드를 직접 쓴다.
- 새 페이지 추가 시: `Page` enum + `Page::ALL` + `label/icon/short/subtitle/from_name` + `update()` 의 `match self.page` 를 모두 고친다.

### 백그라운드 작업 = 스레드 + mpsc 채널 + 매 프레임 poll

- AI 호출·가져오기·분석·폴더 스캔은 `std::thread::spawn` 으로 돌리고, 결과는 `Receiver` 필드(`ask_rx`, `import_rx`, `an.rx`, `pipe.rx` …)로 받는다.
- `update()` 맨 앞의 `poll_*()` 들이 `try_recv` 로 결과를 꺼낸다. **수신기가 하나라도 `Some` 이면 200ms 마다 다시 그리도록**
  `update()` 의 긴 `if self.kw_rx.is_some() || ...` 조건에 새 채널을 추가해야 한다 (빠뜨리면 결과가 와도 화면이 안 바뀜).
- 스레드에는 `Settings` 를 `clone()` 해서 넘긴다.

### 저장 (`model.rs`)

- `Store` 가 OS 데이터 폴더(`dirs::data_dir()/MyDocDesk/`)에 `library.json`, `settings.json`, 원본 파일 복사본 `files/<doc id>.<ext>` 를 둔다.
  쓰기는 `.tmp` 에 쓴 뒤 rename.
- 변경 시 `lib_dirty` / `settings_dirty` 만 켜면 `update()` 가 1초 간격으로 `save_now()` 한다.
- **삭제는 지우지 않고 `deleted: true` 톰스톤**으로 표시하고 `updated_ms` 도 갱신한다 (동기화 병합이 `updated_ms` 가 더 큰 쪽을 이기게 하므로, 안 올리면 다른 기기에서 되살아난다). 목록은 항상 `live_docs()` / `live_agents()` / `live_evals()` 로 읽는다.
- 모든 직렬화 struct 는 `#[serde(default)]` — 예전 JSON 도 읽히도록 새 필드는 기본값이 있어야 한다.

### AI 연결 (`ai.rs`, `cli.rs`, `local.rs`)

- `Provider` 는 경로(`Route`)가 셋: **Local**(Ollama·LM Studio 등, OpenAI 호환 `/v1`), **Api**(OpenAI·Anthropic·Gemini 키),
  **Subscription**(Claude Code `claude` / Codex `codex` / Gemini CLI `gemini` 공식 CLI 를 "답하고 종료" 모드로 실행).
- 모든 질문은 `ai::ask(settings, system, user)` 한 곳을 거친다. `settings.local_only`(내 PC 전용 모드, 기본 켜짐)이면
  클라우드 경로를 막는다 — 이 검사(`blocked`)를 우회하는 새 호출 경로를 만들지 않는다.
- 구독 CLI 는 로그인 토큰을 읽거나 복사하지 않는다 (로그인 파일의 **존재 여부만** 확인).
- 비용 계산은 Api 경로만 (구독·로컬은 0).

### 문서 처리 파이프라인

- `extract.rs` → 확장자별 텍스트 추출 진입점 (`document_from_file`). PDF 는 `pdf-extract`, Office(docx/xlsx/pptx)·HWPX 는 zip+XML,
  구형 바이너리 **HWP** 는 `cfb` + 레코드 파싱 (`docparse.rs`). 표 안의 표(nested table)·차트 숫자·그림 개수를 보존·집계한다.
- `search.rs` — 자체 키워드 인덱스(한글 어미 처리 포함). 임베딩/벡터DB 없음. `ensure_index()` 로 lazily 재구축(`index_dirty`).
- `rag.rs` — 에이전트 설정(답변 형식·태그 필터·개인정보 가림 `guardrail`·근거번호 검사 `cite_check`)으로 프롬프트를 만들고 답을 검사. 질문하기와 품질 평가가 공유.
- `analyze.rs` — 문서 한 건 심층 분석(블록 구조, Markdown/JSON 출력, PII 마스킹, 신뢰도).
- `pipeline.rs` — 감시 폴더에서 새/변경/삭제 파일만 골라 반영 (`source_path` + `source_mtime` 비교).

### 동기화와 공유

- `sync.rs` — 서버 없음. 사용자가 고른 **클라우드 동기화 폴더**(구글 드라이브 등) 안에 계정 폴더를 만들고,
  계정 이름+암호에서 PBKDF2 로 키를 만들어 `library.enc`·`files/*.enc` 를 XChaCha20-Poly1305 로 암호화해 둔다.
  병합은 id 기준 `updated_ms` 최신 우선(`merge_by_id`).
- `share.rs` — 두 방식: ① 정적 HTML 공유 폴더로 내보내기, ② `tiny_http` 로 같은 네트워크에 라이브 서빙
  (`/s/<무작위 토큰>/`, 선택적 PIN, 만료 시간).

## 화면 문구 (한국어 원문 + 베트남어 번역)

- 한국어가 원문이다. 고정 문구는 `crate::i18n::tr("한국어")`, 값이 들어가는 문장은
  `crate::ft!("한국어 {}", "Tiếng Việt {}", x)` 로 쓴다 (`ft!` 는 두 언어 모두 컴파일 시 형식 검사됨).
- `tr()` 용 새 문구는 **`src/i18n.rs` 의 `tr_in` match 표**에 `"한국어" => "Tiếng Việt",` 를 추가하고, `app/i18n_tools/vi.txt` 에도 같은 줄(`한국어 ||| Tiếng Việt`)을 넣는다.
  표에 없으면 베트남어 화면에서도 한국어로 나온다 (에러 없음 — 놓치기 쉽다).
- **`i18n_tools/apply.py` 는 다시 실행하지 않는다.** 이미 적용됐고, 다시 돌리면 번역문을 이중으로 감싼다. (`scan.py`·`list.py` 는 읽기 전용 점검용)
- AI 답변 언어는 화면 언어가 아니라 **질문 언어**를 따른다 (`i18n::answer_in_vi`).

## 함정

- egui 기본 이모지 폰트에 없는 이모지는 네모로 나온다. 확인된 미지원: 🤖 🧩 🪙 🧠 🗂. 아이콘은 이미 쓰고 있는 것(🏠 📚 💬 ✅ 🔗 🔄 ⚙ 📄 ✨ 📊 등) 위주로 쓴다.
- 한글 폰트는 시스템 폰트를 런타임에 읽는다 (`theme::install_fonts`: Noto Sans KR → 맑은 고딕 → Apple SD Gothic Neo). 번들된 폰트 없음.
- 색은 `theme.rs` 의 함수(`BG()`, `TEXT()`, `ACCENT()` …)로만 쓴다 — 라이트/다크 팔레트가 런타임에 바뀌므로 `Color32` 상수를 직접 박지 않는다.
- `eframe`/`egui` 버전은 `=0.31.1` 로 고정돼 있다. API 가 버전마다 크게 바뀌므로 올리지 않는다.
