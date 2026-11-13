# MyDocDesk

**혼자 쓰는 문서 AI 데스크톱 앱** — 흩어진 문서를 모아 내 PC 안에서 검색하고, AI에게 내 문서를 근거로 질문합니다.
Windows·macOS 설치형이며, 기본은 **내 PC에서만** 동작합니다.

![홈 화면](app/shots/home.png)

## 무엇을 하나

| 화면 | 하는 일 |
|---|---|
| 지식 보관함 | 파일을 끌어다 놓거나 폴더째 가져와 AI가 찾을 수 있는 텍스트로 바꿔 둡니다. 태그·메모 지원 |
| 자동 수집 | 연결한 폴더에서 새로 생기거나 바뀐 문서만 골라 반영합니다 |
| 문서 분석 | 문서 한 건을 깊이 읽어 요약·구조·신뢰도를 보여 주고 Markdown·JSON으로 내보냅니다 |
| 질문하기 | 내 문서에서 근거를 찾아 답합니다. 답에 근거 번호가 붙고, 특정 문서만 골라 물을 수도 있습니다 |
| 에이전트 | 자주 하는 일을 도우미로 저장합니다 (답변 형식, 태그 범위, 개인정보 가림, 근거 확인) |
| 품질 평가 | 질문 세트로 검색·답변 품질을 점검합니다 |
| 파일 공유 | 고른 파일을 공유 페이지로 보냅니다 (폴더로 내보내기 / 같은 네트워크에 PIN·만료시간이 있는 링크) |
| 계정·동기화 | 같은 계정의 PC·Mac끼리 데이터를 맞춥니다 |

### 4자리 잠금 (로그인)

본인 PC에서 쓰는 앱이라 아이디·비밀번호 로그인 대신 **숫자 4자리**로 엽니다.

- 처음 실행할 때 4자리를 정하고 한 번 더 입력해 확인하면 바로 사용할 수 있습니다.
- 자리를 비울 땐 오른쪽 위 `🔒 잠금` 또는 `Ctrl+L`. 돌아와서 4자리만 넣으면 풀립니다.
- 5번 틀리면 30초 동안 입력을 막습니다. 번호는 **설정 > 잠금 번호 바꾸기**에서 바꿉니다.
- 번호는 이 기기에만 해시로 저장되며 다른 기기와 동기화되지 않습니다.
- 화면을 가리는 잠금이며 문서 파일 자체를 암호화하지는 않습니다.
- **번호를 잊었다면:** 앱을 끄고 데이터 폴더의 `settings.json` 에서 `"lock_pin"` 값을 `""` 로 지운 뒤 다시 실행하면 새 번호를 정합니다 (문서는 그대로).

화면 언어는 **한국어 / 베트남어**, 테마는 라이트 / 다크를 지원합니다.

### 읽을 수 있는 파일

- 문서: PDF, Word(`docx`), Excel(`xlsx`), PowerPoint(`pptx`), 한글(`hwp`, `hwpx`), `odt`, `rtf`, HTML
- 텍스트: `txt`, `md`, `csv`, `tsv`, `json`, `xml`, `yaml`, 소스 코드 등
- 그림: `png`, `jpg`, `webp` — 그림 해석을 켜면 AI가 그림·차트·스캔 내용을 글로 바꿔 함께 저장합니다

표 안의 표, 차트 숫자도 보존합니다. 구형 바이너리 `hwp` 도 별도 프로그램 없이 읽습니다.

## AI 연결

AI 없이도 문서 보관·검색은 됩니다. 답변을 만들 때만 아래 중 하나를 연결합니다.

| 방식 | 예 | 문서가 PC 밖으로 나가나 |
|---|---|---|
| 내 PC의 AI | Ollama, LM Studio, 기타 OpenAI 호환 로컬 서버 | 아니요 |
| 구독 계정 | Claude Code(`claude`), Codex(`codex`), Gemini CLI(`gemini`) — 각 사 공식 프로그램에 로그인해 두면 그 구독으로 답합니다 | 예 |
| API 키 | OpenAI, Anthropic, Google Gemini | 예 |

- **'내 PC 전용 모드'가 기본으로 켜져 있습니다.** 켜져 있는 동안은 구독·API로 아무것도 보내지 않고 동기화도 하지 않습니다.
- 구독 방식은 공식 프로그램을 그대로 실행할 뿐, 로그인 정보를 읽거나 복사하지 않습니다.
- Gemini CLI는 구글이 개인 계정 로그인을 막는 경우가 있습니다. 그때는 Google Gemini API 키(AI Studio에서 무료 한도 발급)로 연결하세요.
- API 키와 '암호 기억'으로 저장한 동기화 암호는 설정 파일이 아니라 **Windows 자격 증명 관리자 / macOS 키체인**에 저장됩니다
  (예전 버전에서 파일에 저장된 키는 다음 실행 때 자동으로 옮겨집니다).
- 구독 프로그램은 파일 읽기·명령 실행·웹 검색 같은 도구를 모두 끈 상태로 실행하고, 문서 속 지시는 따르지 않도록 지시합니다.

## 동기화와 공유

- **동기화**: 별도 서버가 없습니다. Google Drive·OneDrive·Dropbox·iCloud 같은 클라우드 폴더를 하나 고르면,
  그 안에 계정 이름+암호로 **암호화한 파일**(XChaCha20-Poly1305)을 두고 기기끼리 맞춥니다. 클라우드 업체는 내용을 볼 수 없습니다.
  기기마다 자기 파일에만 쓰기 때문에 두 기기가 동시에 동기화해도 서로 덮어쓰지 않습니다. **모든 기기를 같은 버전으로 업데이트해 주세요**
  (예전 버전은 새 방식으로 쓴 파일을 읽지 못합니다).
- **공유**: 동기화와는 별개입니다. 고른 파일만 공유 페이지로 만들어 폴더로 내보내거나(USB·클라우드에 올리기),
  같은 네트워크의 다른 기기에 비밀 링크로 바로 보여 줍니다. PIN 을 정하면 받는 사람은 화면에서 PIN 을 입력하고(주소에 PIN 이 남지 않음),
  5번 틀리면 5분 동안 막힙니다.

## 빌드와 실행

[Rust](https://rustup.rs) (stable) 가 필요합니다. 소스는 `app/` 폴더에 있습니다.

```bash
cd app
cargo run --release          # 실행
cargo test                   # 테스트
cargo bundle --release       # 설치 파일 만들기 (cargo-bundle 필요: macOS .app / Windows .msi)
```

한글 글꼴은 시스템에 설치된 것(Noto Sans KR → 맑은 고딕 → Apple SD Gothic Neo 순)을 씁니다.

### 화면 캡처 모드

실제 데이터를 건드리지 않고 데모 데이터로 화면을 찍은 뒤 스스로 종료합니다.

```bash
./target/release/mydocdesk --screenshot shots/home.png --page home
MDD_THEME=dark MDD_LANG=vi ./target/release/mydocdesk --screenshot shots/vi-home.png --page home
```

`--page`: `home` `library` `pipeline` `analyze` `ask` `agents` `eval` `share` `ai` `sync` `settings`

## 데이터 위치

문서 목록·설정·원본 파일 사본은 이 기기에만 저장됩니다.
저장할 때마다 마지막 저장본을 `*.json.bak` 으로 함께 남기고, 파일이 손상되면 손상본을 `*.json.corrupt-<시각>` 으로 옮겨 둔 뒤 백업에서 되살립니다.

- Windows: `%APPDATA%\MyDocDesk\`
- macOS: `~/Library/Application Support/MyDocDesk/`

## 화면

| 질문하기 | 문서 분석 |
|---|---|
| ![질문하기](app/shots/ask.png) | ![문서 분석](app/shots/analyze.png) |
| **AI 연결** | **파일 공유** |
| ![AI 연결](app/shots/ai.png) | ![파일 공유](app/shots/share.png) |

## 라이선스

[Apache License 2.0](LICENSE)
