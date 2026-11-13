//! Sample content: a welcome guide for a first start, and a fuller demo set for screenshots.

use crate::extract::make_doc;
use crate::model::*;

pub const GUIDE: &str = "# MyDocDesk 시작 가이드

MyDocDesk는 내 문서를 모아 두고, 그 안에서 답을 찾아 주는 개인용 문서 AI 책상입니다.

## 1. 문서 넣기
'지식 보관함'에서 파일 추가를 누르거나 창 위로 파일을 끌어다 놓으세요. Word, PowerPoint, Excel, 한글(HWPX), PDF, 텍스트, 웹 문서를 읽습니다. 문서는 자동으로 작은 조각으로 나뉘어 검색할 수 있게 됩니다.

## 2. 질문하기
'질문하기'에서 궁금한 것을 물어보세요. AI를 연결하지 않아도 내 PC 안에서 가장 관련 있는 구절을 찾아 보여 줍니다. 설정에서 Ollama(내 PC에서 도는 AI)나 클라우드 AI를 연결하면 문서를 근거로 한 답변을 써 줍니다.

## 3. 에이전트
자주 하는 일을 에이전트로 만들어 두세요. 예를 들어 '계약서 검토 도우미'는 계약 태그가 붙은 문서만 보고 위험 조항을 짚어 줍니다.

## 4. 내 PC 전용 모드
처음에는 모든 데이터가 이 컴퓨터 안에만 있습니다. 다른 컴퓨터와 같이 쓰려면 '계정·동기화'에서 계정 이름과 암호를 정하고 클라우드 폴더를 고르세요. 데이터는 암호화되어 저장됩니다.

## 5. 파일 공유
'파일 공유'에서 문서를 골라 공유 페이지를 만들 수 있습니다. 같은 와이파이의 다른 사람에게 링크를 보내거나, 공유 폴더로 저장해 클라우드에 올리세요.";

fn doc(title: &str, file: &str, ext: &str, text: &str, tags: &[&str], days_ago: i64) -> Document {
    let mut d = make_doc(title.into(), file.into(), ext, text.into(), text.len() as u64 * 2, false);
    d.tags = tags.iter().map(|s| s.to_string()).collect();
    d.added_ms = now_ms() - days_ago * 86_400_000;
    d.updated_ms = d.added_ms;
    d
}

pub fn default_agent() -> Agent {
    Agent {
        id: new_id(),
        name: "기본 도우미".into(),
        description: "모든 문서를 보고 질문에 답합니다".into(),
        instruction: "당신은 사용자의 개인 문서를 잘 아는 비서입니다. 간결하고 정확하게 답하세요.".into(),
        tag_filter: String::new(),
        top_k: 5,
        model: String::new(),
        format: "free".into(),
        guardrail: false,
        cite_check: true,
        updated_ms: now_ms(),
        deleted: false,
    }
}

pub fn welcome() -> Library {
    let mut lib = Library::default();
    let mut g = make_doc("MyDocDesk 시작 가이드".into(), "시작 가이드.md".into(), "md", GUIDE.into(), GUIDE.len() as u64, false);
    g.tags = vec!["가이드".into()];
    lib.docs.push(g);
    lib.agents.push(default_agent());
    lib
}

pub fn demo() -> Library {
    let mut lib = welcome();
    lib.docs.push(doc(
        "2026 가계 예산 계획",
        "2026 가계 예산 계획.xlsx",
        "xlsx",
        "# 2026 가계 예산 계획\n\n항목 | 월 예산 | 비고\n주거비 | 1,200,000원 | 관리비 포함\n식비 | 650,000원 | 외식 월 4회 이하\n교통비 | 180,000원 | 대중교통 위주\n저축 | 1,000,000원 | 비상금 6개월치 목표\n\n## 목표\n연말까지 비상금 600만 원을 모으고, 보험료는 3월에 다시 비교합니다.",
        &["가계", "계획"],
        12,
    ));
    lib.docs.push(doc(
        "전세 계약 체크리스트",
        "전세 계약 체크리스트.hwpx",
        "hwpx",
        "제1조 (보증금) 보증금은 2억 3천만 원으로 하며 계약금은 10%로 한다.\n\n제2조 (계약 기간) 계약 기간은 2026년 3월 1일부터 2년으로 한다.\n\n제3조 (수선 의무) 주요 설비의 수선은 임대인이 부담한다.\n\n확인 사항 | 상태\n등기부등본 근저당 | 확인 완료\n전입신고와 확정일자 | 입주 당일\n보증보험 가입 | 진행 중\n\n특약: 계약 만료 2개월 전까지 갱신 여부를 서로 알린다.",
        &["계약", "집"],
        30,
    ));
    lib.docs.push(doc(
        "11월 프로젝트 회의록",
        "11월 프로젝트 회의록.docx",
        "docx",
        "# 11월 프로젝트 회의록\n\n일시: 11월 5일 오후 2시\n참석: 기획 2명, 개발 3명\n\n## 결정 사항\n1. 출시일은 12월 15일로 확정한다.\n2. 베타 테스트는 11월 25일부터 1주일 동안 진행한다.\n3. 다음 회의에서 가격 정책을 정한다.\n\n## 할 일\n- 테스트 계정 20개 준비 (개발)\n- 안내 메일 초안 작성 (기획)",
        &["업무"],
        8,
    ));
    lib.docs.push(doc(
        "건강검진 결과 요약",
        "건강검진 결과 요약.pdf",
        "pdf",
        "건강검진 결과 요약\n\n혈압 118/76 정상. 공복 혈당 98 정상 범위 상한에 가까움.\n콜레스테롤 LDL 132로 경계. 6개월 뒤 재검 권장.\n\n생활 습관 권고: 주 3회 이상 30분 걷기, 저녁 늦은 간식 줄이기.",
        &["건강"],
        3,
    ));
    lib.docs.push(doc(
        "여행 준비 메모",
        "여행 준비 메모.md",
        "md",
        "# 제주 여행 준비\n\n- 항공권: 12월 20일 출발, 23일 귀국\n- 숙소: 애월 쪽 2박, 서귀포 1박\n- 렌터카 예약 확인 번호는 메일함에 있음\n- 챙길 것: 충전기, 우산, 상비약",
        &["여행"],
        1,
    ));
    let mut contract = default_agent();
    contract.name = "계약서 검토 도우미".into();
    contract.description = "'계약' 태그 문서만 보고 놓치기 쉬운 조항을 짚어 줍니다".into();
    contract.instruction = "당신은 꼼꼼한 계약서 검토자입니다. 날짜, 금액, 의무 조항을 정리하고 위험해 보이는 부분을 먼저 알려 주세요.".into();
    contract.tag_filter = "계약".into();
    contract.top_k = 4;
    contract.guardrail = true;
    contract.format = "report".into();
    lib.agents.push(contract);
    let mut summary = default_agent();
    summary.name = "회의록 요약가".into();
    summary.description = "업무 문서에서 결정 사항과 할 일만 뽑아 줍니다".into();
    summary.instruction = "결정 사항, 할 일, 마감일을 표로 정리하세요.".into();
    summary.tag_filter = "업무".into();
    summary.format = "table".into();
    lib.agents.push(summary);

    for (q, e) in [
        ("전세 보증금은 얼마야?", "보증금, 2억"),
        ("출시일이 언제로 정해졌어?", "출시일, 12월 15일"),
        ("LDL 콜레스테롤 수치는?", "LDL, 132"),
        ("렌터카 예약 번호 어디 있어?", "렌터카, 메일함"),
    ] {
        lib.evals.push(EvalCase {
            id: new_id(),
            question: q.into(),
            expected: e.into(),
            last: None,
            updated_ms: now_ms(),
            deleted: false,
        });
    }
    let day = 86_400_000;
    for (i, (p, c)) in [(1800u64, 420u64), (3200, 650), (900, 240), (4100, 980), (2500, 510), (5200, 1100), (3000, 700)].iter().enumerate() {
        lib.usage.push(UsageEntry {
            id: new_id(),
            ms: now_ms() - (6 - i as i64) * day,
            model: "llama3.1".into(),
            prompt_tokens: *p,
            completion_tokens: *c,
            cost_usd: 0.0,
            agent: if i % 2 == 0 { "기본 도우미".into() } else { "계약서 검토 도우미".into() },
        });
    }
    let t = now_ms() - 600_000;
    lib.chats.push(ChatMessage { id: new_id(), role: "user".into(), text: "전세 계약에서 꼭 챙길 것만 알려줘".into(), agent: "계약서 검토 도우미".into(), ms: t, ..Default::default() });
    lib.chats.push(ChatMessage {
        id: new_id(),
        role: "assistant".into(),
        text: "보증금은 2억 3천만 원이고 계약 기간은 2026년 3월 1일부터 2년입니다 [1]. 보증보험 가입이 아직 '진행 중'이니 입주 전에 마무리하세요 [1]. 갱신 여부는 만료 2개월 전까지 알려야 합니다 [1].".into(),
        sources: vec![SourceRef { doc_id: String::new(), title: "전세 계약 체크리스트".into(), excerpt: "제1조 (보증금) 보증금은 2억 3천만 원으로 하며…".into(), score: 7.2, chunk: 1 }],
        agent: "계약서 검토 도우미".into(),
        model: "llama3.1".into(),
        ms: t + 4000,
        flags: vec!["🔒 개인정보 1건을 가리고 AI에 보냈습니다".into()],
    });
    lib.devices.push(DeviceInfo { name: "내 PC".into(), os: "Windows".into(), last_sync_ms: now_ms() - 120_000 });
    lib.devices.push(DeviceInfo { name: "내 MacBook".into(), os: "macOS".into(), last_sync_ms: now_ms() - 3_600_000 * 5 });
    lib
}
