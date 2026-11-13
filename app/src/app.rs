//! The window: sidebar navigation and every page.

use crate::ai;
use crate::demo;
use crate::extract;
use crate::model::*;
use crate::search::{excerpt, Index};
use crate::share::{self, LiveShare, ShareItem};
use crate::sync::{self, Account};
use crate::theme::{self, *};
use crate::Args;
use egui::{vec2, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, RichText, Sense, Stroke};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "analyze_page.rs"]
mod analyze_page;
#[path = "pipeline_page.rs"]
mod pipeline_page;
#[path = "eval_ai.rs"]
mod eval_ai;
#[path = "ai_page.rs"]
mod ai_page;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Library,
    Pipeline,
    Analyze,
    Ask,
    Agents,
    Eval,
    Share,
    AiConnect,
    Sync,
    Settings,
}

impl Page {
    const ALL: [Page; 11] = [Page::Home, Page::Library, Page::Pipeline, Page::Analyze, Page::Ask, Page::Agents, Page::Eval, Page::Share, Page::AiConnect, Page::Sync, Page::Settings];
    fn label(self) -> &'static str {
        match self {
            Page::Home => crate::i18n::tr("홈"),
            Page::Library => crate::i18n::tr("지식 보관함"),
            Page::Pipeline => crate::i18n::tr("자동 수집"),
            Page::Analyze => crate::i18n::tr("문서 분석"),
            Page::Ask => crate::i18n::tr("질문하기"),
            Page::Agents => crate::i18n::tr("에이전트"),
            Page::Eval => crate::i18n::tr("품질 평가"),
            Page::Share => crate::i18n::tr("파일 공유"),
            Page::AiConnect => crate::i18n::tr("AI 연결"),
            Page::Sync => crate::i18n::tr("계정·동기화"),
            Page::Settings => crate::i18n::tr("설정"),
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Page::Home => "🏠",
            Page::Library => "📚",
            Page::Pipeline => "📈",
            Page::Analyze => "🔍",
            Page::Ask => "💬",
            Page::Agents => "✨",
            Page::Eval => "✅",
            Page::Share => "🔗",
            Page::AiConnect => "✨",
            Page::Sync => "🔄",
            Page::Settings => "⚙",
        }
    }
    /// The label under the icon in the narrow rail.
    fn short(self) -> &'static str {
        if crate::i18n::vi() {
            match self {
                Page::Pipeline => return "Thu thập",
                Page::Analyze => return "Phân tích",
                _ => {}
            }
        }
        match self {
            Page::Home => crate::i18n::tr("홈"),
            Page::Library => crate::i18n::tr("보관함"),
            Page::Pipeline => crate::i18n::tr("자동 수집"),
            Page::Analyze => crate::i18n::tr("문서 분석"),
            Page::Ask => crate::i18n::tr("질문"),
            Page::Agents => crate::i18n::tr("에이전트"),
            Page::Eval => crate::i18n::tr("평가"),
            Page::Share => crate::i18n::tr("공유"),
            Page::AiConnect => crate::i18n::tr("AI 연결"),
            Page::Sync => crate::i18n::tr("동기화"),
            Page::Settings => crate::i18n::tr("설정"),
        }
    }
    fn subtitle(self) -> &'static str {
        match self {
            Page::Home => crate::i18n::tr("내 문서와 AI 사용 현황을 한눈에 봅니다"),
            Page::Library => crate::i18n::tr("흩어진 문서를 모아 AI가 찾을 수 있는 지식으로 바꿉니다"),
            Page::Pipeline => crate::i18n::tr("연결한 폴더의 새 문서·바뀐 문서만 골라 지식으로 바꿉니다"),
            Page::Analyze => crate::i18n::tr("문서 한 건을 깊이 읽어 요약·지표·Markdown·JSON으로 바꿉니다"),
            Page::Ask => crate::i18n::tr("내 문서를 근거로 답을 찾습니다"),
            Page::Agents => crate::i18n::tr("자주 하는 일을 도우미로 만들어 둡니다"),
            Page::Eval => crate::i18n::tr("질문 세트로 검색 품질을 점검합니다"),
            Page::Share => crate::i18n::tr("고른 파일을 공유 페이지로 보냅니다"),
            Page::AiConnect => crate::i18n::tr("구독 계정 · 내 PC의 AI · API 키 중에서 고르고 모델을 정합니다"),
            Page::Sync => crate::i18n::tr("같은 계정의 PC·Mac과 데이터를 맞춥니다"),
            Page::Settings => crate::i18n::tr("AI 연결과 문서 처리 방식을 정합니다"),
        }
    }
    fn from_name(s: &str) -> Option<Page> {
        Some(match s {
            "home" => Page::Home,
            "library" => Page::Library,
            "analyze" | "analyze-setup" | "analyze-running" => Page::Analyze,
            "pipeline" => Page::Pipeline,
            "ai" => Page::AiConnect,
            "ask-selected" => Page::Ask,
            "ask" => Page::Ask,
            "agents" => Page::Agents,
            "eval" => Page::Eval,
            "share" => Page::Share,
            "sync" => Page::Sync,
            "settings" => Page::Settings,
            _ => return None,
        })
    }
}

enum ImportMsg {
    Progress(String),
    Done(Vec<Result<Document, String>>, Vec<UsageEntry>),
}

type AskResult = Result<ai::AiReply, String>;

pub struct DeskApp {
    store: Store,
    lib: Library,
    settings: Settings,
    index: Index,
    chunk_counts: Vec<usize>,
    index_dirty: bool,
    lib_dirty: bool,
    settings_dirty: bool,
    last_save: Instant,
    page: Page,
    toast: Option<(String, Color32, Instant)>,

    shot: Option<PathBuf>,
    frame: u32,
    shot_requested: bool,

    lib_filter: String,
    lib_tag: String,
    selected_doc: Option<String>,
    memo_open: bool,
    memo_title: String,
    memo_text: String,
    tag_input: String,
    import_rx: Option<Receiver<ImportMsg>>,
    importing: usize,
    import_status: String,

    question: String,
    agent_sel: String,
    ask_rx: Option<Receiver<AskResult>>,
    pending_sources: Vec<SourceRef>,
    pending_agent: Option<Agent>,
    pending_flags: Vec<String>,
    highlight: Option<(String, usize)>,
    eval_rx: Option<Receiver<(usize, EvalResult, Option<UsageEntry>)>>,
    eval_running: usize,

    agent_edit: Option<Agent>,

    eval_q: String,
    eval_e: String,

    share_sel: HashSet<String>,
    share_filter: String,
    share_title: String,
    share_msg: String,
    share_pin: String,
    share_hours: u32,
    live: Option<LiveShare>,
    last_export: Option<PathBuf>,

    pass_input: String,
    account: Option<Account>,
    sync_msg: String,
    last_auto_sync: Instant,

    test_rx: Option<Receiver<AskResult>>,
    test_msg: String,

    an: analyze_page::AnalyzeState,
    pipe: pipeline_page::PipeState,
    theme_ready: bool,
    nav_expanded: bool,
    search_q: String,
    aic: ai_page::AiConn,
    /// 질문하기: answer only from the documents picked here.
    ask_selected_only: bool,
    kw_rx: Option<Receiver<(String, AskResult)>>,
    ask_docs: Vec<String>,
    ask_doc_filter: String,
}

impl DeskApp {
    pub fn new(args: Args) -> Self {
        let demo_mode = args.screenshot.is_some();
        let store = if demo_mode { Store::demo() } else { Store::open() };
        let (lib, settings) = if demo_mode {
            let s = Settings { provider: Provider::Ollama, device_name: "내 PC".into(), theme: if std::env::var("MDD_THEME").as_deref() == Ok("dark") { "dark".into() } else { "light".into() }, language: if std::env::var("MDD_LANG").as_deref() == Ok("vi") { "vi".into() } else { "ko".into() }, ..Default::default() };
            (demo::demo(), s)
        } else {
            let first = store.load_settings().is_none();
            let mut lib = store.load_library();
            if first && lib.docs.is_empty() {
                lib = demo::welcome();
            }
            (lib, store.load_settings().unwrap_or_default())
        };
        let page = args.page.as_deref().and_then(Page::from_name).unwrap_or(Page::Home);
        let args_page = args.page.clone();
        let mut app = DeskApp {
            store,
            lib,
            settings,
            index: Index::default(),
            chunk_counts: Vec::new(),
            index_dirty: true,
            lib_dirty: !demo_mode,
            settings_dirty: !demo_mode,
            last_save: Instant::now(),
            page,
            toast: None,
            shot: args.screenshot,
            frame: 0,
            shot_requested: false,
            lib_filter: String::new(),
            lib_tag: String::new(),
            selected_doc: None,
            memo_open: false,
            memo_title: String::new(),
            memo_text: String::new(),
            tag_input: String::new(),
            import_rx: None,
            importing: 0,
            import_status: String::new(),
            question: String::new(),
            agent_sel: String::new(),
            ask_rx: None,
            pending_sources: Vec::new(),
            pending_agent: None,
            pending_flags: Vec::new(),
            highlight: None,
            eval_rx: None,
            eval_running: 0,
            agent_edit: None,
            eval_q: String::new(),
            eval_e: String::new(),
            share_sel: HashSet::new(),
            share_filter: String::new(),
            share_title: "공유 문서".into(),
            share_msg: String::new(),
            share_pin: String::new(),
            share_hours: 24,
            live: None,
            last_export: None,
            pass_input: String::new(),
            account: None,
            sync_msg: String::new(),
            last_auto_sync: Instant::now(),
            test_rx: None,
            test_msg: String::new(),
            an: Default::default(),
            pipe: Default::default(),
            theme_ready: false,
            nav_expanded: false,
            search_q: String::new(),
            aic: Default::default(),
            ask_selected_only: false,
            kw_rx: None,
            ask_docs: Vec::new(),
            ask_doc_filter: String::new(),
        };
        app.agent_sel = app.lib.live_agents().next().map(|(_, a)| a.id.clone()).unwrap_or_default();
        app.selected_doc = app.lib.live_docs().nth(if demo_mode { 2 } else { 0 }).map(|(_, d)| d.id.clone());
        if demo_mode {
            app.share_sel = app.lib.live_docs().skip(2).take(2).map(|(_, d)| d.id.clone()).collect();
            app.share_title = "전세 계약 서류".into();
            app.share_msg = "계약 전에 같이 봐 주세요. 궁금한 점은 메시지 주세요.".into();
            app.settings.account_id = "my-desk".into();
            app.ensure_index();
            app.run_evals();
            app.settings.watch_folders = vec!["D:\\내 문서\\계약".into(), "D:\\내 문서\\업무".into()];
            for d in app.lib.docs.iter_mut() {
                let f = if d.tags.iter().any(|t| t == "계약" || t == "집") { "계약" } else { "업무" };
                d.source_path = format!("D:\\내 문서\\{}\\{}", f, d.file_name);
            }
            for (k, (a, u, r, s)) in [(2usize, 1usize, 0usize, 1.4f32), (0, 0, 0, 0.3), (5, 0, 1, 3.2)].iter().enumerate() {
                app.settings.runs.push(PipelineRun { ms: now_ms() - (k as i64 + 1) * 600_000, folders: 2, scanned: 6, added: *a, updated: *u, removed: *r, failed: 0, seconds: *s, errors: vec![] });
            }
            let judged: [(f32, bool, bool, u8); 4] = [(1.0, true, true, 5), (1.0, true, true, 4), (0.5, true, true, 3), (1.0, false, true, 4)];
            let mut k = 0;
            for e in app.lib.evals.iter_mut() {
                if let (Some(l), Some(j)) = (e.last.as_mut(), judged.get(k)) {
                    l.answer_score = Some(j.0);
                    l.cites = Some(j.1);
                    l.pii_safe = Some(j.2);
                    l.judge = Some(j.3);
                }
                k += 1;
            }
            match args_page.as_deref() {
                Some("analyze-setup") => {}
                Some("analyze-running") => {
                    let (tx, rx) = channel();
                    std::mem::forget(tx);
                    app.an.rx = Some(rx);
                    app.an.file = Some(PathBuf::from("2026 사업계획서.hwp"));
                    app.an.step = 2;
                    app.an.pct = 0.62;
                    app.an.msg = "그림 2/3 해석 중".into();
                }
                _ => app.demo_analysis(),
            }
            app.demo_ai();
            if args_page.as_deref() == Some("ask-selected") {
                app.ask_selected_only = true;
                app.ask_docs = app.lib.live_docs().filter(|(_, d)| d.tags.iter().any(|t| t == "계약" || t == "업무")).map(|(_, d)| d.id.clone()).collect();
                app.lib.chats_cleared_ms = now_ms();
                app.question = "두 문서에서 날짜가 정해진 일만 정리해 줘".into();
            }
        }
        if !demo_mode {
            app.start_ai_detect();
        }
        if !demo_mode && app.settings.remember_pass && !app.settings.saved_pass.is_empty() && !app.settings.local_only {
            app.pass_input = app.settings.saved_pass.clone();
            app.connect_account();
        }
        app
    }

    fn toast(&mut self, msg: impl Into<String>, color: Color32) {
        self.toast = Some((msg.into(), color, Instant::now()));
    }

    fn touch_lib(&mut self) {
        self.lib_dirty = true;
        self.index_dirty = true;
    }

    fn ensure_index(&mut self) {
        if self.index_dirty {
            self.index = Index::build(&self.lib, self.settings.chunk_size);
            self.chunk_counts = vec![0; self.lib.docs.len()];
            for c in &self.index.chunks {
                if let Some(n) = self.chunk_counts.get_mut(c.doc) {
                    *n += 1;
                }
            }
            self.index_dirty = false;
        }
    }

    fn save_now(&mut self) {
        if self.lib_dirty {
            if let Err(e) = self.store.save_library(&self.lib) {
                self.toast(crate::ft!("저장하지 못했습니다: {e}", "Không lưu được: {e}", e = e), BAD());
            }
            self.lib_dirty = false;
        }
        if self.settings_dirty {
            let mut s = self.settings.clone();
            if !s.remember_pass {
                s.saved_pass.clear();
            }
            let _ = self.store.save_settings(&s);
            self.settings_dirty = false;
        }
        self.last_save = Instant::now();
    }

    // ---------- import ----------

    fn import_paths(&mut self, paths: Vec<PathBuf>) {
        let mut files = Vec::new();
        for p in paths {
            if p.is_dir() {
                extract::collect_folder(&p, &mut files, 0);
            } else if extract::is_supported(&p) {
                files.push(p);
            }
        }
        if files.is_empty() {
            self.toast(crate::i18n::tr("읽을 수 있는 문서가 없습니다"), WARN());
            return;
        }
        let (tx, rx) = channel();
        let store = self.store.clone();
        let settings = self.settings.clone();
        let vision = settings.vision && settings.provider != Provider::None && !(settings.local_only && settings.provider.is_cloud());
        self.importing = files.len();
        self.import_status.clear();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let mut usage = Vec::new();
            let total = files.len();
            for (fi, p) in files.into_iter().enumerate() {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let _ = tx.send(ImportMsg::Progress(crate::ft!("{}/{} '{}' 읽는 중", "{}/{} đang đọc '{}'", fi + 1, total, name)));
                let r = extract::document_from_file(&p).and_then(|(mut d, bytes, media)| {
                    if vision && !media.is_empty() {
                        let mut added = String::new();
                        let n = media.len().min(8);
                        for (mi, m) in media.iter().take(n).enumerate() {
                            let _ = tx.send(ImportMsg::Progress(crate::ft!("'{}' 그림 {}/{} AI 해석 중", "'{}' đang dùng AI đọc hình {}/{}", name, mi + 1, n)));
                            match ai::describe_image(&settings, m.mime, &m.bytes) {
                                Ok(rep) => {
                                    usage.push(UsageEntry {
                                        id: new_id(),
                                        ms: now_ms(),
                                        model: rep.model.clone(),
                                        prompt_tokens: rep.prompt_tokens,
                                        completion_tokens: rep.completion_tokens,
                                        cost_usd: ai::cost(&settings, rep.prompt_tokens, rep.completion_tokens),
                                        agent: crate::i18n::tr("그림 해석").into(),
                                    });
                                    added.push_str(&format!("\n\n## 그림 {} (AI 해석)\n{}", mi + 1, rep.text.trim()));
                                    d.ai_pictures += 1;
                                }
                                Err(e) => {
                                    d.parse_note = crate::ft!("그림 해석 일부 실패: {e}", "Một phần đọc hình bị lỗi: {e}", e = e);
                                    break;
                                }
                            }
                        }
                        d.text.push_str(&added);
                        d.text = d.text.trim().to_string();
                    }
                    if d.text.trim().is_empty() {
                        return Err(if vision {
                            crate::i18n::tr("그림에서 글자나 내용을 얻지 못했습니다").to_string()
                        } else {
                            crate::i18n::tr("그림만 있는 파일입니다. 설정에서 'AI 그림 해석'을 켜면 읽을 수 있습니다").to_string()
                        });
                    }
                    let (_, tables, headings) = extract::complexity(&d.text);
                    d.tables = d.tables.max(tables);
                    d.headings = headings;
                    extract::rescore(&mut d);
                    if !store.read_only {
                        std::fs::write(store.file_path(&d), &bytes).map_err(|e| crate::ft!("원본 복사 실패: {e}", "Sao chép bản gốc thất bại: {e}", e = e))?;
                    }
                    Ok(d)
                });
                out.push(r.map_err(|e| format!("{name}: {e}")));
            }
            let _ = tx.send(ImportMsg::Done(out, usage));
        });
        self.import_rx = Some(rx);
    }

    fn poll_import(&mut self) {
        let Some(rx) = &self.import_rx else { return };
        let mut done = None;
        while let Ok(m) = rx.try_recv() {
            match m {
                ImportMsg::Progress(s) => self.import_status = s,
                ImportMsg::Done(r, u) => done = Some((r, u)),
            }
        }
        if let Some((results, usage)) = done {
            self.lib.usage.extend(usage);
            self.import_rx = None;
            self.importing = 0;
            let (mut ok, mut skipped, mut errs) = (0, 0, Vec::new());
            let mut last = None;
            for r in results {
                match r {
                    Ok(d) => {
                        let dup = self.lib.live_docs().any(|(_, x)| x.file_name == d.file_name && x.size_bytes == d.size_bytes);
                        if dup {
                            let _ = std::fs::remove_file(self.store.file_path(&d));
                            skipped += 1;
                        } else {
                            last = Some(d.id.clone());
                            self.lib.docs.push(d);
                            ok += 1;
                        }
                    }
                    Err(e) => errs.push(e),
                }
            }
            if ok > 0 {
                self.touch_lib();
                self.selected_doc = last;
            }
            let mut msg = crate::ft!("문서 {ok}개를 지식으로 변환했습니다", "Đã chuyển {ok} tài liệu thành tri thức", ok = ok);
            if skipped > 0 {
                msg.push_str(&crate::ft!(" · 이미 있는 {skipped}개는 건너뜀", " · bỏ qua {skipped} tài liệu đã có", skipped = skipped));
            }
            if !errs.is_empty() {
                msg.push_str(&crate::ft!(" · 실패 {}개: {}", " · lỗi {}: {}", errs.len(), errs[0]));
            }
            self.toast(msg, if errs.is_empty() { GOOD() } else { WARN() });
        }
    }

    // ---------- ask ----------

    fn current_agent(&self) -> Agent {
        self.lib
            .live_agents()
            .find(|(_, a)| a.id == self.agent_sel)
            .map(|(_, a)| a.clone())
            .unwrap_or_else(demo::default_agent)
    }

    /// The picked documents that still exist, in the order they were picked.
    fn picked_docs(&self) -> Vec<(usize, String)> {
        self.ask_docs
            .iter()
            .filter_map(|id| self.lib.docs.iter().position(|d| &d.id == id && !d.deleted).map(|i| (i, self.lib.docs[i].title.clone())))
            .collect()
    }

    fn retrieve(&mut self, q: &str, agent: &Agent) -> Vec<(usize, f32)> {
        self.ensure_index();
        let docs = &self.lib.docs;
        if !self.ask_selected_only {
            let tag = agent.tag_filter.trim().to_string();
            return self.index.search(q, agent.top_k.max(1), |di| tag.is_empty() || docs[di].tags.iter().any(|t| t == &tag));
        }
        // only the picked documents (the agent's tag limit doesn't apply: the person chose them)
        let picked: Vec<usize> = self.ask_docs.iter().filter_map(|id| docs.iter().position(|d| &d.id == id && !d.deleted)).collect();
        pick_passages(&self.index, q, &picked, agent.top_k.max(1))
    }

    fn send_question(&mut self) {
        let q = self.question.trim().to_string();
        if q.is_empty() || self.ask_rx.is_some() || self.kw_rx.is_some() {
            return;
        }
        if self.ask_selected_only && self.picked_docs().is_empty() {
            self.toast(crate::i18n::tr("'선택 문서'에서 답할 문서를 하나 이상 골라 주세요"), WARN());
            return;
        }
        self.question.clear();
        let agent = self.current_agent();
        self.lib.chats.push(ChatMessage { id: new_id(), role: "user".into(), text: q.clone(), agent: agent.name.clone(), ms: now_ms(), ..Default::default() });
        self.lib_dirty = true;
        // A Vietnamese question shares no words with Korean documents: with an AI connected,
        // turn it into Korean search words first, then search with both.
        if crate::i18n::detect(&q) == crate::i18n::Lang::Vi && crate::analyze::ai_ready(&self.settings) {
            let s = crate::rag::settings_for(&self.settings, &agent);
            let q2 = q.clone();
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let r = ai::ask(
                    &s,
                    "You convert a Vietnamese question into search keywords for a document search over Korean and Vietnamese documents. Output only 3 to 8 keywords on one line: the Korean words first, then the key Vietnamese terms. No explanation.",
                    &q2,
                );
                let _ = tx.send((q2, r));
            });
            self.kw_rx = Some(rx);
            return;
        }
        self.answer_question(q, String::new());
    }

    fn poll_keywords(&mut self) {
        let Some(rx) = &self.kw_rx else { return };
        let Ok((q, r)) = rx.try_recv() else { return };
        self.kw_rx = None;
        let kw = match r {
            Ok(rep) => {
                self.lib.usage.push(UsageEntry {
                    id: new_id(),
                    ms: now_ms(),
                    model: rep.model.clone(),
                    prompt_tokens: rep.prompt_tokens,
                    completion_tokens: rep.completion_tokens,
                    cost_usd: ai::cost(&self.settings, rep.prompt_tokens, rep.completion_tokens),
                    agent: "검색어 번역".into(),
                });
                rep.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().chars().take(200).collect()
            }
            Err(_) => String::new(),
        };
        self.answer_question(q, kw);
    }

    /// Finds the passages and answers (in the language of the question).
    fn answer_question(&mut self, q: String, keywords: String) {
        let picked = self.picked_docs();
        let agent = self.current_agent();
        let to_vi = crate::i18n::answer_in_vi(&q);
        let t = |s: &'static str| crate::i18n::tr_in(to_vi, s);
        let search_q = if keywords.is_empty() { q.clone() } else { format!("{q} {keywords}") };
        let hits = self.retrieve(&search_q, &agent);
        let mut flags: Vec<String> = Vec::new();
        if self.ask_selected_only {
            let names: Vec<String> = picked.iter().take(3).map(|(_, t)| t.clone()).collect();
            flags.push(crate::ft!("📄 선택 문서 {}개에서만 찾았습니다: {}{}", "📄 Chỉ tìm trong {} tài liệu đã chọn: {}{}", picked.len(), names.join(", "), if picked.len() > 3 { crate::i18n::tr(" 외") } else { "" }));
        }
        if !keywords.is_empty() {
            flags.push(if crate::i18n::vi() { format!("🔎 Từ khóa tìm kiếm: {keywords}") } else { format!("🔎 검색어로 바꿔 찾음: {keywords}") });
        }
        let sources: Vec<SourceRef> = hits
            .iter()
            .map(|(ci, s)| {
                let c = &self.index.chunks[*ci];
                let d = &self.lib.docs[c.doc];
                SourceRef { doc_id: d.id.clone(), title: d.title.clone(), excerpt: excerpt(&c.text, 900), score: *s, chunk: c.ord + 1 }
            })
            .collect();

        if self.settings.provider == Provider::None || sources.is_empty() {
            let text = if sources.is_empty() {
                let mut s = t("관련된 내용을 문서에서 찾지 못했습니다. 다른 낱말로 물어보거나, 지식 보관함에 문서를 더 넣어 주세요.").to_string();
                if to_vi && keywords.is_empty() && !crate::analyze::ai_ready(&self.settings) {
                    s.push_str("\n\nNếu tài liệu là tiếng Hàn, câu hỏi tiếng Việt cần kết nối AI để dịch từ khóa tìm kiếm (Kết nối AI).");
                }
                s
            } else {
                let mut s = String::from(t("내 PC 안에서 가장 관련 있는 구절을 찾았습니다 (AI 연결 없이 검색만 사용):\n"));
                for (i, x) in sources.iter().take(3).enumerate() {
                    s.push_str(&format!("\n[{}] {} — {}", i + 1, x.title, excerpt(&x.excerpt, 220)));
                }
                s.push_str(t("\n\n설정에서 AI를 연결하면 이 구절들을 바탕으로 정리된 답변을 받을 수 있습니다."));
                s
            };
            self.lib.chats.push(ChatMessage { id: new_id(), role: "assistant".into(), text, sources, agent: agent.name, model: t("내 PC 검색").into(), ms: now_ms(), flags });
            return;
        }

        let mut p = crate::rag::prepare(&self.settings, &agent, &sources, &q);
        for f in flags.into_iter().rev() {
            p.flags.insert(0, f);
        }
        let (tx, rx) = channel();
        let (settings, system, user) = (p.settings, p.system, p.user);
        std::thread::spawn(move || {
            let _ = tx.send(ai::ask(&settings, &system, &user));
        });
        self.ask_rx = Some(rx);
        self.pending_sources = sources;
        self.pending_flags = p.flags;
        self.pending_agent = Some(agent);
    }

    fn poll_ask(&mut self) {
        let Some(rx) = &self.ask_rx else { return };
        let Ok(r) = rx.try_recv() else { return };
        self.ask_rx = None;
        let sources = std::mem::take(&mut self.pending_sources);
        let mut flags = std::mem::take(&mut self.pending_flags);
        let agent_obj = self.pending_agent.take().unwrap_or_else(demo::default_agent);
        let agent = agent_obj.name.clone();
        match r {
            Ok(rep) => {
                let cost = ai::cost(&self.settings, rep.prompt_tokens, rep.completion_tokens);
                self.lib.usage.push(UsageEntry {
                    id: new_id(),
                    ms: now_ms(),
                    model: rep.model.clone(),
                    prompt_tokens: rep.prompt_tokens,
                    completion_tokens: rep.completion_tokens,
                    cost_usd: cost,
                    agent: agent.clone(),
                });
                let text = crate::rag::check(&agent_obj, rep.text, &mut flags);
                self.lib.chats.push(ChatMessage { id: new_id(), role: "assistant".into(), text, sources, agent, model: rep.model, ms: now_ms(), flags });
            }
            Err(e) => {
                let mut text = crate::ft!("AI 답변을 받지 못했습니다: {e}", "Không nhận được câu trả lời từ AI: {e}", e = e);
                if !sources.is_empty() {
                    text.push_str(crate::i18n::tr("\n\n대신 찾은 구절:"));
                    for (i, s) in sources.iter().take(3).enumerate() {
                        text.push_str(&format!("\n[{}] {} — {}", i + 1, s.title, excerpt(&s.excerpt, 200)));
                    }
                }
                self.lib.chats.push(ChatMessage { id: new_id(), role: "assistant".into(), text, sources, agent, model: crate::i18n::tr("오류").into(), ms: now_ms(), flags });
            }
        }
        self.lib_dirty = true;
    }

    // ---------- evaluation ----------

    fn run_evals(&mut self) {
        self.ensure_index();
        let mut results = Vec::new();
        for (i, e) in self.lib.live_evals() {
            let hits = self.index.search(&e.question, 3, |_| true);
            let combined: String = hits.iter().map(|(c, _)| self.index.chunks[*c].text.to_lowercase()).collect::<Vec<_>>().join("\n");
            let kws: Vec<String> = e.expected.split(',').map(|k| k.trim().to_lowercase()).filter(|k| !k.is_empty()).collect();
            let found = kws.iter().filter(|k| combined.contains(k.as_str())).count();
            let score = if kws.is_empty() { if hits.is_empty() { 0.0 } else { 1.0 } } else { found as f32 / kws.len() as f32 };
            let found_in = hits.first().map(|(c, _)| self.lib.docs[self.index.chunks[*c].doc].title.clone()).unwrap_or_default();
            let prev = self.lib.evals[i].last.clone().unwrap_or_default();
            results.push((i, EvalResult { passed: score >= 0.999, score, found_in, ms: now_ms(), ..prev }));
        }
        for (i, r) in results {
            self.lib.evals[i].last = Some(r);
            self.lib.evals[i].updated_ms = now_ms();
        }
        self.lib_dirty = !self.store.read_only;
    }

    fn eval_rate(&self) -> Option<f32> {
        let done: Vec<&EvalResult> = self.lib.live_evals().filter_map(|(_, e)| e.last.as_ref()).collect();
        if done.is_empty() {
            None
        } else {
            Some(done.iter().filter(|r| r.passed).count() as f32 / done.len() as f32)
        }
    }

    // ---------- sync ----------

    fn connect_account(&mut self) {
        if self.settings.local_only {
            self.sync_msg = crate::i18n::tr("'내 PC 전용 모드'를 먼저 꺼 주세요").into();
            return;
        }
        match sync::unlock(&self.settings.sync_folder, &self.settings.account_id, &self.pass_input) {
            Ok((acc, new)) => {
                self.account = Some(acc);
                if self.settings.remember_pass {
                    self.settings.saved_pass = self.pass_input.clone();
                }
                self.settings_dirty = true;
                self.sync_msg = if new { crate::i18n::tr("새 계정을 만들었습니다. 다른 기기에서도 같은 이름·암호로 연결하세요.").into() } else { crate::i18n::tr("계정에 연결했습니다").into() };
                self.do_sync();
            }
            Err(e) => {
                self.sync_msg = e.clone();
                self.toast(e, BAD());
            }
        }
    }

    fn do_sync(&mut self) {
        let Some(acc) = self.account.clone() else { return };
        if self.settings.local_only {
            return;
        }
        let device = self.settings.device_name.clone();
        match sync::sync(&acc, &mut self.lib, &self.store, &device) {
            Ok(r) => {
                self.settings.last_sync_ms = now_ms();
                self.settings_dirty = true;
                self.touch_lib();
                self.sync_msg = crate::ft!("동기화 완료 · 문서 {}개 · 받은 변경 {}건 · 올린 파일 {}개 · 받은 파일 {}개{}", "Đồng bộ xong · {} tài liệu · nhận {} thay đổi · tải lên {} tệp · tải về {} tệp{}", r.total_docs,
                    r.pulled_docs,
                    r.pushed_files,
                    r.pulled_files,
                    if r.new_account { crate::i18n::tr(" (처음 올림)") } else { "" });
                self.toast(crate::i18n::tr("동기화했습니다"), GOOD());
            }
            Err(e) => {
                self.sync_msg = crate::ft!("동기화 실패: {e}", "Đồng bộ thất bại: {e}", e = e);
                self.toast(self.sync_msg.clone(), BAD());
            }
        }
        self.last_auto_sync = Instant::now();
    }

    // ---------- share ----------

    fn share_items(&self) -> Vec<ShareItem> {
        let mut out = Vec::new();
        for (_, d) in self.lib.live_docs().filter(|(_, d)| self.share_sel.contains(&d.id)) {
            let p = self.store.file_path(d);
            if d.has_file && p.exists() {
                if let Ok(b) = std::fs::read(&p) {
                    out.push(ShareItem { name: d.file_name.clone(), kind: d.kind.clone(), bytes: Arc::new(b) });
                    continue;
                }
            }
            out.push(ShareItem { name: format!("{}.txt", d.title), kind: crate::i18n::tr("텍스트").into(), bytes: Arc::new(d.text.clone().into_bytes()) });
        }
        out
    }

    // ---------- frame ----------

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else { return };
        self.frame += 1;
        ctx.request_repaint();
        if self.frame >= 120 && !self.shot_requested {
            self.shot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let mut done = false;
        ctx.input(|i| {
            for e in &i.raw.events {
                if let egui::Event::Screenshot { image, .. } = e {
                    let [w, h] = image.size;
                    let mut buf = Vec::with_capacity(w * h * 4);
                    for p in &image.pixels {
                        buf.extend_from_slice(&p.to_srgba_unmultiplied());
                    }
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = image::save_buffer(&path, &buf, w as u32, h as u32, image::ExtendedColorType::Rgba8) {
                        eprintln!("screenshot failed: {e}");
                    }
                    done = true;
                }
            }
        });
        if done || self.frame > 900 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for DeskApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.screenshot_step(ctx);
        self.poll_import();
        self.poll_ask();
        self.poll_keywords();
        self.poll_analyze();
        self.poll_scan();
        self.poll_ai_conn();
        self.poll_eval_ai();
        self.auto_scan_tick();
        if let Some(rx) = &self.test_rx {
            if let Ok(r) = rx.try_recv() {
                self.test_msg = match r {
                    Ok(rep) => crate::ft!("연결 성공 ✔ ({}): {}", "Kết nối thành công ✔ ({}): {}", rep.model, excerpt(&rep.text, 80)),
                    Err(e) => crate::ft!("연결 실패: {e}", "Kết nối thất bại: {e}", e = e),
                };
                self.test_rx = None;
            }
        }
        if self.kw_rx.is_some() || self.aic.rx.is_some() || self.aic.models_rx.is_some() || self.aic.start_rx.is_some() || self.pipe.rx.is_some() || self.eval_rx.is_some() || self.an.rx.is_some() || self.import_rx.is_some() || self.ask_rx.is_some() || self.test_rx.is_some() || self.live.is_some() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        // files dropped onto the window
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            if self.page == Page::Analyze && self.an.rx.is_none() {
                self.an.result = None;
                self.an.file = dropped.into_iter().find(|p| extract::is_supported(p));
            } else {
                self.page = Page::Library;
                self.import_paths(dropped);
            }
        }

        // auto sync every 5 minutes
        if self.account.is_some() && self.settings.auto_sync && !self.settings.local_only && self.last_auto_sync.elapsed() > Duration::from_secs(300) {
            self.do_sync();
        }

        crate::i18n::set_vi(self.settings.language == "vi");
        self.sync_theme(ctx);
        self.app_bar(ctx);
        self.nav(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(BG()).inner_margin(Margin { left: 10, right: 10, top: 10, bottom: 10 }))
            .show(ctx, |ui| {
                // the white sheet the page sits on
                egui::Frame::default()
                    .fill(CARD())
                    .stroke(Stroke::new(1.0, LINE()))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin { left: 28, right: 28, top: 24, bottom: 14 })
                    .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                self.top_bar(ui);
                ui.add_space(16.0);
                match self.page {
                    Page::Home => self.page_home(ui),
                    Page::Library => self.page_library(ui),
                    Page::Analyze => self.page_analyze(ui),
                    Page::Pipeline => self.page_pipeline(ui),
                    Page::AiConnect => self.page_ai(ui),
                    Page::Ask => self.page_ask(ui),
                    Page::Agents => self.page_agents(ui),
                    Page::Eval => self.page_eval(ui),
                    Page::Share => self.page_share(ui),
                    Page::Sync => self.page_sync(ui),
                    Page::Settings => self.page_settings(ui),
                }
                    });
            });
        self.agent_editor(ctx);
        self.memo_window(ctx);
        self.drop_overlay(ctx);
        self.toast_area(ctx);

        if self.last_save.elapsed() > Duration::from_secs(1) && (self.lib_dirty || self.settings_dirty) {
            self.save_now();
        }
    }

    fn persist_egui_memory(&self) -> bool {
        self.shot.is_none()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(l) = &self.live {
            l.stop();
        }
        self.save_now();
    }
}

// ================= drawing =================

fn kind_color(kind: &str) -> Color32 {
    match kind {
        "Word" => Color32::from_rgb(0x2e, 0x7d, 0xff),
        "PowerPoint" => Color32::from_rgb(0xf0, 0x6a, 0x3c),
        "Excel" | "표(CSV)" => Color32::from_rgb(0x27, 0xa8, 0x6b),
        "한글" => Color32::from_rgb(0x2b, 0xb6, 0xd8),
        "PDF" => Color32::from_rgb(0xe5, 0x48, 0x4d),
        "Markdown" | "메모" => PURPLE(),
        _ => Color32::from_rgb(0x6b, 0x78, 0x88),
    }
}

fn kind_badge(ui: &mut egui::Ui, kind: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let c = kind_color(kind);
    ui.painter().rect_filled(rect, CornerRadius::same(8), c.gamma_multiply(0.25));
    let short: String = match kind {
        "PowerPoint" => "PPT".into(),
        "Markdown" => "MD".into(),
        "텍스트" => "TXT".into(),
        "표(CSV)" => "CSV".into(),
        "웹 문서" => "WEB".into(),
        k => k.chars().take(4).collect(),
    };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, short, FontId::proportional(size * 0.3), c);
}

fn complexity_color(c: u8) -> Color32 {
    match c {
        0..=29 => GOOD(),
        30..=59 => WARN(),
        _ => BAD(),
    }
}

fn stat_card(ui: &mut egui::Ui, icon: &str, title: &str, value: String, sub: String, color: Color32) {
    // WorkBoard ".stat": bordered surface, small grey label, big number, faint note
    card().inner_margin(Margin::symmetric(16, 14)).show(ui, |ui| {
        ui.set_min_height(84.0);
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon).color(TEXT2()).size(13.0));
            ui.label(RichText::new(title).color(TEXT2()).size(12.5));
        });
        ui.label(bold(value, 26.0).color(if color == BAD() { BAD() } else { TEXT() }));
        ui.label(RichText::new(sub).color(MUTED()).size(12.0));
    });
}

impl DeskApp {
    /// The 47 px bar across the top: menu toggle, logo, search, status and theme.
    fn app_bar(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::K)) {
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("global_search")));
        }
        egui::TopBottomPanel::top("appbar")
            .exact_height(47.0)
            .frame(egui::Frame::default().fill(CARD()).inner_margin(Margin { left: 12, right: 14, top: 0, bottom: 0 }))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // menu toggle (rail <-> full sidebar)
                    let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(4), theme::HOVER());
                    }
                    let ib = egui::Rect::from_center_size(r.center(), vec2(16.0, 14.0));
                    ui.painter().rect_stroke(ib, CornerRadius::same(2), Stroke::new(1.4, TEXT2()), egui::StrokeKind::Inside);
                    ui.painter().line_segment([egui::pos2(ib.left() + 5.0, ib.top()), egui::pos2(ib.left() + 5.0, ib.bottom())], Stroke::new(1.4, TEXT2()));
                    if resp.on_hover_text(if self.nav_expanded { crate::i18n::tr("메뉴 접기") } else { crate::i18n::tr("메뉴 펼치기") }).clicked() {
                        self.nav_expanded = !self.nav_expanded;
                    }
                    ui.add_space(8.0);
                    // logo
                    let (rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                    let p = ui.painter();
                    p.circle_filled(rect.center(), 13.0, ACCENT());
                    p.rect_filled(egui::Rect::from_center_size(rect.center(), vec2(11.0, 13.0)), CornerRadius::same(2), ACCENT_INK());
                    for k in 0..3 {
                        let y = rect.center().y - 3.5 + k as f32 * 3.5;
                        p.line_segment([egui::pos2(rect.center().x - 3.5, y), egui::pos2(rect.center().x + 3.5, y)], Stroke::new(1.2, ACCENT()));
                    }
                    ui.label(bold("MyDocDesk", 16.0).color(PURPLE()));
                    ui.add_space(22.0);
                    // search box
                    let sw = 360.0f32.min(ui.available_width() - 420.0).max(160.0);
                    egui::Frame::default()
                        .fill(FIELD())
                        .stroke(Stroke::new(1.0, LINE()))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(10, 0))
                        .show(ui, |ui| {
                            ui.set_width(sw);
                            // a slimmer box: 28 px tall instead of 40
                            ui.spacing_mut().interact_size.y = 26.0;
                            ui.horizontal(|ui| {
                                ui.set_height(26.0);
                                ui.label(RichText::new("🔍").size(12.5).color(MUTED()));
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut self.search_q)
                                        .id(egui::Id::new("global_search"))
                                        .hint_text(crate::i18n::tr("문서, 대화, 에이전트 검색"))
                                        .frame(false)
                                        .margin(Margin::symmetric(2, 4))
                                        .desired_width(sw - 92.0),
                                );
                                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.search_q.trim().is_empty() {
                                    self.lib_filter = self.search_q.trim().to_string();
                                    self.page = Page::Library;
                                }
                                let g = ui.painter().layout_no_wrap("Ctrl K".into(), FontId::monospace(11.0), MUTED());
                                let (kr, _) = ui.allocate_exact_size(g.size() + vec2(10.0, 2.0), Sense::hover());
                                ui.painter().rect_stroke(kr, CornerRadius::same(3), Stroke::new(1.0, LINE2()), egui::StrokeKind::Inside);
                                ui.painter().galley(kr.center() - g.size() / 2.0, g, MUTED());
                            });
                        });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // light / dark segment (like the KO | VI switch)
                        let dark = theme::is_dark();
                        let mut pick: Option<&str> = None;
                        let mut rects = Vec::new();
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (k, l, on) in [("dark", crate::i18n::tr("다크"), dark), ("light", crate::i18n::tr("라이트"), !dark)] {
                            let g = ui.painter().layout_no_wrap(l.into(), FontId::proportional(12.5), if on { ACCENT() } else { TEXT2() });
                            let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 20.0, 28.0), Sense::click());
                            if on {
                                ui.painter().rect_filled(r, CornerRadius::ZERO, ACCENT_SOFT());
                            } else if resp.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::ZERO, theme::HOVER());
                            }
                            ui.painter().galley(r.center() - g.size() / 2.0, g, TEXT());
                            if resp.clicked() {
                                pick = Some(k);
                            }
                            rects.push(r);
                        }
                        if rects.len() == 2 {
                            let all = rects[0].union(rects[1]);
                            ui.painter().rect_stroke(all, CornerRadius::same(4), Stroke::new(1.0, LINE2()), egui::StrokeKind::Inside);
                            let x = rects[0].left().max(rects[1].left());
                            ui.painter().line_segment([egui::pos2(x, all.top()), egui::pos2(x, all.bottom())], Stroke::new(1.0, LINE2()));
                        }
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if let Some(k) = pick {
                            self.settings.theme = k.into();
                            self.settings_dirty = true;
                        }
                        ui.add_space(6.0);
                        // KO | VI segment, drawn the same way
                        let is_vi = self.settings.language == "vi";
                        let mut lang_pick: Option<&str> = None;
                        let mut rects = Vec::new();
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (k, l, on) in [("vi", "VI", is_vi), ("ko", "KO", !is_vi)] {
                            let g = ui.painter().layout_no_wrap(l.into(), FontId::proportional(12.5), if on { ACCENT() } else { TEXT2() });
                            let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 20.0, 28.0), Sense::click());
                            if on {
                                ui.painter().rect_filled(r, CornerRadius::ZERO, ACCENT_SOFT());
                            } else if resp.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::ZERO, theme::HOVER());
                            }
                            ui.painter().galley(r.center() - g.size() / 2.0, g, TEXT());
                            if resp.on_hover_text(if k == "vi" { "Tiếng Việt" } else { "한국어" }).clicked() {
                                lang_pick = Some(k);
                            }
                            rects.push(r);
                        }
                        if rects.len() == 2 {
                            let all = rects[0].union(rects[1]);
                            ui.painter().rect_stroke(all, CornerRadius::same(4), Stroke::new(1.0, LINE2()), egui::StrokeKind::Inside);
                            let x = rects[0].left().max(rects[1].left());
                            ui.painter().line_segment([egui::pos2(x, all.top()), egui::pos2(x, all.bottom())], Stroke::new(1.0, LINE2()));
                        }
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if let Some(k) = lang_pick {
                            self.settings.language = k.into();
                            crate::i18n::set_vi(k == "vi");
                            self.settings_dirty = true;
                        }
                        ui.add_space(4.0);
                        let ai_text = match self.settings.provider {
                            Provider::None => crate::i18n::tr("AI 검색만").to_string(),
                            p => {
                                let m = ai::model_name(&self.settings);
                                if m.is_empty() { p.short().to_string() } else { format!("{} · {}", p.short(), m) }
                            }
                        };
                        let blocked = self.settings.local_only && self.settings.provider.is_cloud();
                        chip(ui, &ai_text, if blocked { WARN() } else { PURPLE() });
                        if self.settings.local_only {
                            chip(ui, crate::i18n::tr("🔒 내 PC 전용"), GOOD());
                        } else if self.account.is_some() {
                            chip(ui, crate::i18n::tr("☁ 동기화 켜짐"), ACCENT());
                        } else {
                            chip(ui, crate::i18n::tr("☁ 동기화 대기"), WARN());
                        }
                    });
                });
            });
    }

    fn nav(&mut self, ctx: &egui::Context) {
        if self.nav_expanded {
            self.sidebar(ctx);
        } else {
            self.rail(ctx);
        }
    }

    /// WorkBoard's 64 px icon rail: icon above a small label; agents as coloured squares.
    fn rail(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("rail")
            .exact_width(64.0)
            .resizable(false)
            .frame(egui::Frame::default().fill(SIDEBAR()).inner_margin(Margin { left: 4, right: 4, top: 8, bottom: 8 }))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                let item = |ui: &mut egui::Ui, icon: &str, label: &str, selected: bool, badge: bool| -> egui::Response {
                    let (rect, resp) = ui.allocate_exact_size(vec2(56.0, 48.0), Sense::click());
                    let p = ui.painter();
                    if selected {
                        p.rect_filled(rect, CornerRadius::same(6), ACCENT_SOFT());
                    } else if resp.hovered() {
                        p.rect_filled(rect, CornerRadius::same(6), theme::HOVER());
                    }
                    let col = if selected { ACCENT() } else { TEXT2() };
                    p.text(rect.center() - vec2(0.0, 8.0), Align2::CENTER_CENTER, icon, FontId::proportional(16.0), col);
                    p.text(rect.center() + vec2(0.0, 12.0), Align2::CENTER_CENTER, label, FontId::proportional(10.5), col);
                    if badge {
                        p.circle_filled(rect.center() + vec2(11.0, -15.0), 4.0, BAD());
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    resp
                };
                let (top, bottom): (Vec<Page>, Vec<Page>) = Page::ALL.iter().partition(|p| !matches!(p, Page::AiConnect | Page::Sync | Page::Settings));
                for p in top {
                    let busy = (p == Page::Share && self.live.is_some()) || (p == Page::Pipeline && self.pipe.rx.is_some());
                    if item(ui, p.icon(), p.short(), self.page == p, busy).on_hover_text(p.label()).clicked() {
                        self.page = p;
                    }
                }
                ui.add_space((ui.available_height() - 3.0 * 52.0).max(4.0));
                for p in bottom {
                    if item(ui, p.icon(), p.short(), self.page == p, false).on_hover_text(p.label()).clicked() {
                        self.page = p;
                    }
                }
            });
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav")
            .exact_width(256.0)
            .resizable(false)
            .frame(egui::Frame::default().fill(SIDEBAR()).inner_margin(Margin { left: 8, right: 8, top: 10, bottom: 12 }))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (i, p) in Page::ALL.iter().enumerate() {
                    if i == 0 || i == 8 {
                        if i == 8 {
                            ui.add_space(10.0);
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(10.0);
                            ui.label(RichText::new(if i == 0 { crate::i18n::tr("작업") } else { crate::i18n::tr("계정 · 설정") }).color(MUTED()).size(12.0));
                        });
                        ui.add_space(2.0);
                    }
                    let selected = self.page == *p;
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                    let painter = ui.painter();
                    if selected {
                        painter.rect_filled(rect, CornerRadius::same(4), ACCENT_SOFT());
                    } else if resp.hovered() {
                        painter.rect_filled(rect, CornerRadius::same(4), theme::HOVER());
                    }
                    let (ic, tx) = if selected { (ACCENT(), ACCENT()) } else { (TEXT2(), TEXT()) };
                    painter.text(rect.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, p.icon(), FontId::proportional(14.0), ic);
                    let font = if selected { FontId::new(14.0, egui::FontFamily::Name("bold".into())) } else { FontId::proportional(14.0) };
                    painter.text(rect.left_center() + vec2(38.0, 0.0), Align2::LEFT_CENTER, p.label(), font, tx);
                    if *p == Page::Share && self.live.is_some() {
                        painter.circle_filled(rect.right_center() - vec2(14.0, 0.0), 4.0, GOOD());
                    }
                    if *p == Page::Pipeline && self.pipe.rx.is_some() {
                        painter.text(rect.right_center() - vec2(12.0, 0.0), Align2::RIGHT_CENTER, crate::i18n::tr("수집 중"), FontId::proportional(12.0), MUTED());
                    }
                    if resp.clicked() {
                        self.page = *p;
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
                ui.add_space((ui.available_height() - 76.0).max(8.0));
                egui::Frame::default()
                    .fill(FIELD())
                    .stroke(Stroke::new(1.0, LINE()))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 2.0;
                        if self.settings.local_only {
                            ui.label(RichText::new(crate::i18n::tr("🔒 내 PC 전용 모드")).color(GOOD()).size(13.0));
                            ui.label(RichText::new(crate::i18n::tr("모든 데이터가 이 컴퓨터 안에만 있습니다")).color(TEXT2()).size(12.0));
                        } else if let Some(acc) = &self.account {
                            ui.label(RichText::new(crate::ft!("☁ {} 계정 연결됨", "☁ Đã kết nối tài khoản {}", acc.id)).color(ACCENT()).size(13.0));
                            let t = if self.settings.last_sync_ms > 0 { fmt_time(self.settings.last_sync_ms) } else { "-".into() };
                            ui.label(RichText::new(crate::ft!("마지막 동기화 {t}", "Đồng bộ gần nhất {t}", t = t)).color(TEXT2()).size(12.0));
                        } else {
                            ui.label(RichText::new(crate::i18n::tr("☁ 동기화 꺼짐")).color(WARN()).size(13.0));
                            ui.label(RichText::new(crate::i18n::tr("계정·동기화에서 연결하세요")).color(TEXT2()).size(12.0));
                        }
                    });
            });
    }

    /// WorkBoard ".page-h": a 22 px title with its note underneath.
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.label(bold(self.page.label(), 22.0));
        ui.add_space(-4.0);
        ui.label(RichText::new(self.page.subtitle()).color(TEXT2()).size(13.0));
    }

    /// Follows the system theme unless the person picked light or dark.
    fn sync_theme(&mut self, ctx: &egui::Context) {
        let want_dark = match self.settings.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => ctx.system_theme() == Some(egui::Theme::Dark),
        };
        if want_dark != theme::is_dark() || !self.theme_ready {
            ctx.set_theme(if want_dark { egui::Theme::Dark } else { egui::Theme::Light });
            theme::set_dark(ctx, want_dark);
            self.theme_ready = true;
        }
    }

    // ---------------- home ----------------

    fn page_home(&mut self, ui: &mut egui::Ui) {
        self.ensure_index();
        let docs = self.lib.live_docs().count();
        let chars: usize = self.lib.live_docs().map(|(_, d)| d.text.chars().count()).sum();
        let questions = self.lib.chats.iter().filter(|c| c.role == "user").count();
        let month_start = {
            use chrono::Datelike;
            let n = chrono::Local::now();
            chrono::NaiveDate::from_ymd_opt(n.year(), n.month(), 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|d| d.and_utc().timestamp_millis())
                .unwrap_or(0)
        };
        let month: Vec<&UsageEntry> = self.lib.usage.iter().filter(|u| u.ms >= month_start).collect();
        let month_tokens: u64 = month.iter().map(|u| u.prompt_tokens + u.completion_tokens).sum();
        let month_cost: f64 = month.iter().map(|u| u.cost_usd).sum();
        let chunks = self.index.chunks.len();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(4, |c| {
                stat_card(&mut c[0], "📄", crate::i18n::tr("보관 중인 문서"), crate::ft!("{docs}개", "{docs}", docs = docs), crate::ft!("총 {}자", "Tổng {} ký tự", fmt_num(chars as u64)), ACCENT());
                stat_card(&mut c[1], "💡", crate::i18n::tr("지식 조각"), crate::ft!("{chunks}개", "{chunks}", chunks = chunks), crate::ft!("조각 크기 약 {}자", "Mỗi đoạn khoảng {} ký tự", self.settings.chunk_size), PURPLE());
                stat_card(&mut c[2], "💬", crate::i18n::tr("질문한 횟수"), crate::ft!("{questions}회", "{questions} lần", questions = questions), crate::i18n::tr("대화 기록은 계정에 함께 저장").into(), GOOD());
                stat_card(&mut c[3], "💰", crate::i18n::tr("이번 달 AI 사용"), crate::ft!("{} 토큰", "{} token", fmt_num(month_tokens)), crate::ft!("예상 비용 ${:.2}", "Chi phí ước tính ${:.2}", month_cost), WARN());
            });
            ui.add_space(6.0);
            ui.columns(2, |c| {
                card().show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(230.0);
                    ui.horizontal(|ui| {
                        ui.label(bold(crate::i18n::tr("최근 7일 AI 사용량"), 16.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(muted(crate::i18n::tr("■ 질문  ■ 답변")));
                        });
                    });
                    ui.add_space(8.0);
                    self.usage_chart(ui);
                });
                card().show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(230.0);
                    ui.label(bold(crate::i18n::tr("지식 품질"), 16.0));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let rate = self.eval_rate();
                        ui.vertical(|ui| {
                            ui.label(muted(crate::i18n::tr("검색 정답률")));
                            ui.label(bold(rate.map(|r| format!("{:.0}%", r * 100.0)).unwrap_or_else(|| "-".into()), 34.0).color(if rate.unwrap_or(0.0) >= 0.8 { GOOD() } else { WARN() }));
                            ui.label(muted(crate::ft!("평가 질문 {}개", "{} câu hỏi đánh giá", self.lib.live_evals().count())));
                        });
                        ui.add_space(24.0);
                        ui.vertical(|ui| {
                            ui.label(muted(crate::i18n::tr("문서 복잡도 분포")));
                            let mut b = [0usize; 3];
                            for (_, d) in self.lib.live_docs() {
                                b[match d.complexity { 0..=29 => 0, 30..=59 => 1, _ => 2 }] += 1;
                            }
                            let total = docs.max(1) as f32;
                            for (i, (name, col)) in [(crate::i18n::tr("쉬움"), GOOD()), (crate::i18n::tr("보통"), WARN()), (crate::i18n::tr("복잡"), BAD())].iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(*name).size(13.0));
                                    theme::bar(ui, b[i] as f32 / total, 170.0, *col, &crate::ft!("{}개", "{}", b[i]));
                                });
                            }
                        });
                    });
                    ui.add_space(6.0);
                    if ui.button(crate::i18n::tr("품질 평가 열기 →")).clicked() {
                        self.page = Page::Eval;
                    }
                });
            });
            ui.add_space(6.0);
            ui.columns(2, |c| {
                card().show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(210.0);
                    ui.horizontal(|ui| {
                        ui.label(bold(crate::i18n::tr("최근 추가한 문서"), 16.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if primary_button(ui, crate::i18n::tr("＋ 파일 추가")).clicked() {
                                if let Some(files) = rfd::FileDialog::new().pick_files() {
                                    self.page = Page::Library;
                                    self.import_paths(files);
                                }
                            }
                        });
                    });
                    ui.add_space(6.0);
                    let mut recent: Vec<Document> = self.lib.live_docs().map(|(_, d)| d.clone()).collect();
                    recent.sort_by_key(|d| -d.added_ms);
                    for d in recent.iter().take(4) {
                        let r = ui.horizontal(|ui| {
                            kind_badge(ui, &d.kind, 32.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&d.title).color(TEXT()));
                                ui.label(muted(format!("{} · {}", d.kind, fmt_time(d.added_ms))));
                            });
                        });
                        if ui.interact(r.response.rect, egui::Id::new(("recent", &d.id)), Sense::click()).clicked() {
                            self.selected_doc = Some(d.id.clone());
                            self.page = Page::Library;
                        }
                    }
                    if recent.is_empty() {
                        ui.label(muted(crate::i18n::tr("아직 문서가 없습니다. 파일을 창에 끌어다 놓아도 됩니다.")));
                    }
                });
                card().show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(210.0);
                    ui.label(bold(crate::i18n::tr("데이터 위치와 기기"), 16.0));
                    ui.add_space(6.0);
                    if self.settings.local_only {
                        ui.label(RichText::new(crate::i18n::tr("🔒 내 PC 전용 모드 — 문서, 대화, 설정이 이 컴퓨터 밖으로 나가지 않습니다.")).color(TEXT2()));
                    } else {
                        ui.label(RichText::new(crate::ft!("☁ 동기화 계정: {}", "☁ Tài khoản đồng bộ: {}", if self.settings.account_id.is_empty() { "-" } else { &self.settings.account_id })).color(TEXT2()));
                    }
                    ui.add_space(4.0);
                    let mut devs = self.lib.devices.clone();
                    if devs.is_empty() {
                        devs.push(DeviceInfo { name: self.settings.device_name.clone(), os: os_name().into(), last_sync_ms: 0 });
                    }
                    for d in devs.iter().take(4) {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(if d.os == "macOS" { "💻" } else { "🖥" }).size(16.0));
                            ui.label(RichText::new(&d.name).color(TEXT()));
                            ui.label(muted(d.os.clone()));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(muted(if d.last_sync_ms > 0 { crate::ft!("동기화 {}", "Đồng bộ {}", fmt_time(d.last_sync_ms)) } else { crate::i18n::tr("이 기기").into() }));
                            });
                        });
                    }
                    ui.add_space(4.0);
                    if ui.button(crate::i18n::tr("계정·동기화 열기 →")).clicked() {
                        self.page = Page::Sync;
                    }
                });
            });
        });
    }

    fn usage_chart(&self, ui: &mut egui::Ui) {
        let day = 86_400_000i64;
        let today0 = {
            let n = chrono::Local::now();
            n.date_naive().and_hms_opt(0, 0, 0).map(|d| d.and_local_timezone(chrono::Local).single().map(|t| t.timestamp_millis()).unwrap_or(0)).unwrap_or(0)
        };
        let mut bars = [(0u64, 0u64); 7];
        for u in &self.lib.usage {
            let k = (today0 + day - 1 - u.ms).div_euclid(day);
            let idx = 6 - k;
            if (0..7).contains(&idx) {
                bars[idx as usize].0 += u.prompt_tokens;
                bars[idx as usize].1 += u.completion_tokens;
            }
        }
        let max = bars.iter().map(|b| b.0 + b.1).max().unwrap_or(0).max(1) as f32;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 160.0), Sense::hover());
        let p = ui.painter();
        for g in 0..4 {
            let y = rect.bottom() - 22.0 - g as f32 * (rect.height() - 30.0) / 3.0;
            p.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], Stroke::new(1.0, LINE()));
        }
        let slot = rect.width() / 7.0;
        for (i, (a, b)) in bars.iter().enumerate() {
            let x = rect.left() + slot * i as f32 + slot * 0.28;
            let w = slot * 0.44;
            let h_total = rect.height() - 30.0;
            let ha = *a as f32 / max * h_total;
            let hb = *b as f32 / max * h_total;
            let base = rect.bottom() - 22.0;
            p.rect_filled(egui::Rect::from_min_max(egui::pos2(x, base - ha), egui::pos2(x + w, base)), CornerRadius::same(3), ACCENT());
            p.rect_filled(egui::Rect::from_min_max(egui::pos2(x, base - ha - hb), egui::pos2(x + w, base - ha)), CornerRadius::same(3), PURPLE());
            let label = fmt_day(today0 - (6 - i as i64) * day + day / 2);
            p.text(egui::pos2(x + w / 2.0, rect.bottom() - 8.0), Align2::CENTER_CENTER, label, FontId::proportional(11.5), MUTED());
        }
        if self.lib.usage.is_empty() {
            p.text(rect.center(), Align2::CENTER_CENTER, crate::i18n::tr("아직 AI 사용 기록이 없습니다"), FontId::proportional(13.0), MUTED());
        }
    }

    // ---------------- library ----------------

    fn page_library(&mut self, ui: &mut egui::Ui) {
        self.ensure_index();
        ui.horizontal(|ui| {
            if primary_button(ui, crate::i18n::tr("＋ 파일 추가")).clicked() {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter(crate::i18n::tr("문서"), extract::SUPPORTED)
                    .add_filter(crate::i18n::tr("모든 파일"), &["*"])
                    .pick_files()
                {
                    self.import_paths(files);
                }
            }
            if ui.button(crate::i18n::tr("📁 폴더 통째로 추가")).clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.import_paths(vec![dir]);
                }
            }
            if ui.button(crate::i18n::tr("📝 메모로 추가")).clicked() {
                self.memo_open = true;
            }
            if self.importing > 0 {
                ui.spinner();
                ui.label(RichText::new(if self.import_status.is_empty() { crate::ft!("{}개 문서를 지식으로 바꾸는 중…", "Đang chuyển {} tài liệu thành tri thức…", self.importing) } else { self.import_status.clone() }).color(ACCENT()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let tags = self.lib.all_tags();
                egui::ComboBox::from_id_salt("tagf")
                    .selected_text(if self.lib_tag.is_empty() { crate::i18n::tr("모든 태그").to_string() } else { format!("# {}", self.lib_tag) })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.lib_tag, String::new(), crate::i18n::tr("모든 태그"));
                        for t in tags {
                            let l = format!("# {t}");
                            ui.selectable_value(&mut self.lib_tag, t, l);
                        }
                    });
                ui.add(egui::TextEdit::singleline(&mut self.lib_filter).hint_text(crate::i18n::tr("🔍 제목·내용 검색")).desired_width(220.0));
            });
        });
        ui.add_space(8.0);

        let avail = ui.available_size();
        let left_w = (avail.x * 0.55).floor();
        let filter = self.lib_filter.to_lowercase();
        let rows: Vec<(usize, Document)> = self
            .lib
            .live_docs()
            .filter(|(_, d)| self.lib_tag.is_empty() || d.tags.contains(&self.lib_tag))
            .filter(|(_, d)| filter.is_empty() || d.title.to_lowercase().contains(&filter) || d.text.to_lowercase().contains(&filter))
            .map(|(i, d)| (i, d.clone()))
            .collect();

        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(left_w, avail.y), Layout::top_down(Align::Min), |ui| {
                card().inner_margin(Margin::same(12)).show(ui, |ui| {
                    ui.set_min_size(vec2(left_w - 24.0, avail.y - 40.0));
                    ui.horizontal(|ui| {
                        ui.label(bold(crate::ft!("문서 {}개", "{} tài liệu", rows.len()), 15.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(muted(crate::i18n::tr("파일을 이 창에 끌어다 놓아도 됩니다")));
                        });
                    });
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().id_salt("doclist").auto_shrink([false, false]).show(ui, |ui| {
                        for (i, d) in &rows {
                            let selected = self.selected_doc.as_deref() == Some(d.id.as_str());
                            let fill = if selected { ACCENT_SOFT() } else { Color32::TRANSPARENT };
                            let resp = egui::Frame::default()
                                .fill(fill)
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(10, 8))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        kind_badge(ui, &d.kind, 38.0);
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(&d.title).color(TEXT()).size(14.5));
                                            let chunks = self.chunk_counts.get(*i).copied().unwrap_or(0);
                                            ui.label(muted(crate::ft!("{} · {} · 조각 {} · {}", "{} · {} · {} đoạn · {}", d.kind, fmt_size(d.size_bytes), chunks, fmt_time(d.added_ms))));
                                        });
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            chip(ui, &format!("{} {}", extract::complexity_label(d.complexity), d.complexity), complexity_color(d.complexity));
                                            for t in d.tags.iter().take(2) {
                                                chip(ui, &format!("#{t}"), MUTED());
                                            }
                                        });
                                    });
                                })
                                .response;
                            let click = ui.interact(resp.rect, egui::Id::new(("doc", &d.id)), Sense::click());
                            if click.hovered() && !selected {
                                ui.painter().rect_stroke(resp.rect, CornerRadius::same(8), Stroke::new(1.0, LINE()), egui::StrokeKind::Inside);
                            }
                            if click.clicked() {
                                self.selected_doc = Some(d.id.clone());
                            }
                        }
                        if rows.is_empty() {
                            ui.add_space(40.0);
                            ui.vertical_centered(|ui| {
                                ui.label(bold(crate::i18n::tr("문서가 없습니다"), 16.0));
                                ui.label(muted(crate::i18n::tr("Word, PowerPoint, Excel, 한글(HWPX), PDF, 텍스트 파일을 넣어 보세요")));
                            });
                        }
                    });
                });
            });
            ui.add_space(4.0);
            let right_w = avail.x - left_w - 12.0;
            ui.allocate_ui_with_layout(vec2(right_w, avail.y), Layout::top_down(Align::Min), |ui| {
                card().show(ui, |ui| {
                    ui.set_min_size(vec2(right_w - 36.0, avail.y - 52.0));
                    ui.set_max_width(right_w - 36.0);
                    self.doc_detail(ui);
                });
            });
        });
    }

    fn doc_detail(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected_doc.clone() else {
            ui.label(muted(crate::i18n::tr("왼쪽에서 문서를 고르세요")));
            return;
        };
        let Some(di) = self.lib.docs.iter().position(|d| d.id == id && !d.deleted) else {
            ui.label(muted(crate::i18n::tr("왼쪽에서 문서를 고르세요")));
            return;
        };
        egui::ScrollArea::vertical().id_salt("detail").auto_shrink([false, false]).show(ui, |ui| {
            let d = self.lib.docs[di].clone();
            ui.horizontal(|ui| {
                kind_badge(ui, &d.kind, 44.0);
                let mut title = d.title.clone();
                let r = ui.add(egui::TextEdit::singleline(&mut title).font(FontId::proportional(18.0)).desired_width(ui.available_width()).frame(false));
                if r.changed() {
                    self.lib.docs[di].title = title;
                    self.lib.docs[di].updated_ms = now_ms();
                    self.touch_lib();
                }
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                let mut remove = None;
                for (ti, t) in d.tags.iter().enumerate() {
                    if ui.add(egui::Button::new(RichText::new(format!("#{t}  ×")).size(12.5).color(TEXT2())).corner_radius(CornerRadius::same(100))).on_hover_text(crate::i18n::tr("태그 빼기")).clicked() {
                        remove = Some(ti);
                    }
                }
                if let Some(ti) = remove {
                    self.lib.docs[di].tags.remove(ti);
                    self.lib.docs[di].updated_ms = now_ms();
                    self.touch_lib();
                }
                let r = ui.add(egui::TextEdit::singleline(&mut self.tag_input).hint_text(crate::i18n::tr("+ 태그")).desired_width(90.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.tag_input.trim().is_empty() {
                    let t = self.tag_input.trim().trim_start_matches('#').to_string();
                    if !self.lib.docs[di].tags.contains(&t) {
                        self.lib.docs[di].tags.push(t);
                        self.lib.docs[di].updated_ms = now_ms();
                        self.touch_lib();
                    }
                    self.tag_input.clear();
                }
            });
            ui.add_space(8.0);
            let chunks = self.chunk_counts.get(di).copied().unwrap_or(0);
            egui::Grid::new("docinfo").num_columns(4).spacing(vec2(18.0, 6.0)).show(ui, |ui| {
                ui.label(muted(crate::i18n::tr("형식")));
                ui.label(RichText::new(&d.kind).color(TEXT()));
                ui.label(muted(crate::i18n::tr("크기")));
                ui.label(RichText::new(fmt_size(d.size_bytes)).color(TEXT()));
                ui.end_row();
                ui.label(muted(crate::i18n::tr("추가한 날")));
                ui.label(RichText::new(fmt_time(d.added_ms)).color(TEXT()));
                ui.label(muted(crate::i18n::tr("지식 조각")));
                ui.label(RichText::new(crate::ft!("{chunks}개", "{chunks}", chunks = chunks)).color(TEXT()));
                ui.end_row();
                ui.label(muted(crate::i18n::tr("표")));
                ui.label(RichText::new(crate::ft!("{}개", "{}", d.tables)).color(TEXT()));
                ui.label(muted(crate::i18n::tr("표 안의 표")));
                ui.label(RichText::new(crate::ft!("{}개", "{}", d.nested_tables)).color(if d.nested_tables > 0 { WARN() } else { TEXT() }));
                ui.end_row();
                ui.label(muted(crate::i18n::tr("그림")));
                ui.label(RichText::new(if d.ai_pictures > 0 { crate::ft!("{}개 (AI 해석 {}개)", "{} (AI đã đọc {})", d.images, d.ai_pictures) } else { crate::ft!("{}개", "{}", d.images) }).color(TEXT()));
                ui.label(muted(crate::i18n::tr("차트")));
                ui.label(RichText::new(crate::ft!("{}개 (수치 추출)", "{} (đã lấy số liệu)", d.charts)).color(TEXT()));
                ui.end_row();
                ui.label(muted(crate::i18n::tr("제목·조항")));
                ui.label(RichText::new(crate::ft!("{}개", "{}", d.headings)).color(TEXT()));
                ui.end_row();
            });
            if !d.parse_note.is_empty() {
                ui.label(RichText::new(format!("⚠ {}", d.parse_note)).color(WARN()).size(13.0));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(muted(crate::i18n::tr("문서 복잡도")));
                theme::bar(ui, d.complexity as f32 / 100.0, 200.0, complexity_color(d.complexity), &crate::ft!("{} · {}점", "{} · {} điểm", extract::complexity_label(d.complexity), d.complexity));
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let path = self.store.file_path(&d);
                if ui.add_enabled(d.has_file && path.exists(), egui::Button::new(crate::i18n::tr("📂 원본 열기"))).clicked() {
                    let _ = open::that(&path);
                }
                if ui.button(crate::i18n::tr("💬 이 문서로 질문")).clicked() {
                    self.ask_selected_only = true;
                    self.ask_docs = vec![d.id.clone()];
                    self.question = crate::i18n::tr("이 문서를 요약해 줘").into();
                    self.page = Page::Ask;
                }
                if ui.button(crate::i18n::tr("🔗 공유에 담기")).clicked() {
                    self.share_sel.insert(d.id.clone());
                    self.toast(crate::i18n::tr("공유 목록에 담았습니다"), GOOD());
                }
                if ui.add(egui::Button::new(RichText::new(crate::i18n::tr("🗑 삭제")).color(BAD()))).clicked() {
                    self.lib.docs[di].deleted = true;
                    self.lib.docs[di].text.clear();
                    self.lib.docs[di].updated_ms = now_ms();
                    let _ = std::fs::remove_file(self.store.file_path(&d));
                    self.selected_doc = None;
                    self.touch_lib();
                    self.toast(crate::i18n::tr("문서를 지웠습니다"), MUTED());
                }
            });
            ui.add_space(10.0);
            egui::CollapsingHeader::new(bold(crate::i18n::tr("지식 조각 미리보기"), 14.5)).default_open(true).show(ui, |ui| {
                let hl = self.highlight.as_ref().filter(|h| h.0 == d.id).map(|h| h.1);
                let mut shown: Vec<&crate::search::Chunk> = self.index.chunks_of(di).collect();
                if let Some(h) = hl {
                    shown.sort_by_key(|c| if c.ord + 1 == h { 0 } else { 1 });
                }
                for c in shown.into_iter().take(6) {
                    let is_hl = hl == Some(c.ord + 1);
                    egui::Frame::default().fill(FIELD()).stroke(if is_hl { Stroke::new(1.5, PURPLE()) } else { Stroke::NONE }).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(10)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(if is_hl { crate::ft!("조각 {} — 답변의 근거", "Đoạn {} — căn cứ của câu trả lời", c.ord + 1) } else { crate::ft!("조각 {}", "Đoạn {}", c.ord + 1) }).color(if is_hl { PURPLE() } else { ACCENT() }).size(12.0));
                        ui.add(egui::Label::new(RichText::new(excerpt(&c.text, 260)).size(13.0)).wrap());
                    });
                }
                if chunks > 6 {
                    ui.label(muted(crate::ft!("… 외 {}개", "… và {} đoạn khác", chunks - 6)));
                }
            });
            egui::CollapsingHeader::new(bold(crate::i18n::tr("본문 전체"), 14.5)).default_open(false).show(ui, |ui| {
                let t: String = d.text.chars().take(8000).collect();
                ui.add(egui::Label::new(RichText::new(t).size(13.0)).wrap());
            });
        });
    }

    fn memo_window(&mut self, ctx: &egui::Context) {
        if !self.memo_open {
            return;
        }
        let mut open = true;
        let mut save = false;
        egui::Window::new(crate::i18n::tr("메모로 추가")).open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).default_width(520.0).show(ctx, |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.memo_title).hint_text(crate::i18n::tr("제목")).desired_width(f32::INFINITY));
            ui.add(egui::TextEdit::multiline(&mut self.memo_text).hint_text(crate::i18n::tr("내용을 붙여 넣거나 적으세요")).desired_rows(12).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if primary_button(ui, crate::i18n::tr("보관함에 넣기")).clicked() {
                    save = true;
                }
            });
        });
        if save && !self.memo_text.trim().is_empty() {
            let title = if self.memo_title.trim().is_empty() { crate::i18n::tr("메모").to_string() } else { self.memo_title.trim().to_string() };
            let mut d = extract::make_doc(title.clone(), format!("{title}.txt"), "memo", self.memo_text.clone(), self.memo_text.len() as u64, false);
            d.tags.push(crate::i18n::tr("메모").into());
            self.selected_doc = Some(d.id.clone());
            self.lib.docs.push(d);
            self.touch_lib();
            self.memo_text.clear();
            self.memo_title.clear();
            self.memo_open = false;
            self.toast(crate::i18n::tr("메모를 보관함에 넣었습니다"), GOOD());
        } else if !open {
            self.memo_open = false;
        }
    }

    fn drop_overlay(&self, ctx: &egui::Context) {
        if !ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            return;
        }
        let rect = ctx.screen_rect();
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
        p.rect_filled(rect, CornerRadius::ZERO, Color32::from_black_alpha(170));
        p.rect_stroke(rect.shrink(24.0), CornerRadius::same(16), Stroke::new(2.0, ACCENT()), egui::StrokeKind::Inside);
        p.text(rect.center(), Align2::CENTER_CENTER, crate::i18n::tr("여기에 놓으면 지식 보관함에 추가합니다"), FontId::proportional(22.0), TEXT());
    }

    // ---------------- ask ----------------

    fn page_ask(&mut self, ui: &mut egui::Ui) {
        egui::TopBottomPanel::top("ask_top").frame(egui::Frame::default()).show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(muted(crate::i18n::tr("에이전트")));
                let agents: Vec<(String, String)> = self.lib.live_agents().map(|(_, a)| (a.id.clone(), a.name.clone())).collect();
                let cur = agents.iter().find(|a| a.0 == self.agent_sel).map(|a| a.1.clone()).unwrap_or_else(|| crate::i18n::tr("기본 도우미").into());
                egui::ComboBox::from_id_salt("agentsel").selected_text(cur).width(200.0).show_ui(ui, |ui| {
                    for (id, name) in agents {
                        ui.selectable_value(&mut self.agent_sel, id, name);
                    }
                });
                let a = self.current_agent();
                if !a.tag_filter.is_empty() && !self.ask_selected_only {
                    chip(ui, &crate::ft!("#{} 문서만", "Chỉ tài liệu #{}", a.tag_filter), ACCENT());
                }
                ui.label(muted(a.description.clone()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(crate::i18n::tr("대화 지우기")).clicked() {
                        self.lib.chats_cleared_ms = now_ms();
                        self.lib_dirty = true;
                    }
                });
            });
            ui.add_space(4.0);
            self.ask_scope_row(ui);
            ui.add_space(8.0);
        });
        egui::TopBottomPanel::bottom("ask_input").frame(egui::Frame::default().inner_margin(Margin { left: 0, right: 0, top: 10, bottom: 0 })).show_inside(ui, |ui| {
            egui::Frame::default().fill(CARD()).corner_radius(CornerRadius::same(12)).stroke(Stroke::new(1.0, LINE())).inner_margin(Margin::same(10)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let w = ui.available_width() - 90.0;
                    let r = ui.add(
                        egui::TextEdit::multiline(&mut self.question)
                            .hint_text(crate::i18n::tr("내 문서에 대해 물어보세요 (Enter: 보내기, Shift+Enter: 줄바꿈)"))
                            .desired_rows(2)
                            .desired_width(w)
                            .frame(false),
                    );
                    let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                    let busy = self.ask_rx.is_some() || self.kw_rx.is_some();
                    let clicked = ui.add_enabled(!busy, egui::Button::new(RichText::new(crate::i18n::tr("보내기")).color(ACCENT_INK())).fill(ACCENT()).min_size(vec2(80.0, 40.0))).clicked();
                    if (enter || clicked) && !busy {
                        while self.question.ends_with('\n') {
                            self.question.pop();
                        }
                        self.send_question();
                    }
                });
            });
            ui.label(muted(if self.settings.provider == Provider::None {
                crate::i18n::tr("지금은 AI 없이 내 PC 안의 검색만 사용합니다 · 설정에서 Ollama나 클라우드 AI를 연결할 수 있습니다").to_string()
            } else if self.settings.local_only && self.settings.provider.is_cloud() {
                crate::i18n::tr("⚠ 내 PC 전용 모드라 클라우드 AI로 보내지 않습니다").to_string()
            } else {
                crate::ft!("답변: {} · 질문과 관련 구절만 AI에 보냅니다", "Trả lời: {} · chỉ gửi câu hỏi và các đoạn liên quan tới AI", ai::model_name(&self.settings))
            }));
        });
        egui::CentralPanel::default().frame(egui::Frame::default()).show_inside(ui, |ui| {
            let msgs: Vec<ChatMessage> = self.lib.visible_chats().cloned().collect();
            egui::ScrollArea::vertical().id_salt("chat").auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                if msgs.is_empty() {
                    ui.add_space(50.0);
                    ui.vertical_centered(|ui| {
                        ui.label(bold(crate::i18n::tr("무엇이든 물어보세요"), 22.0));
                        ui.label(muted(crate::i18n::tr("보관함의 문서에서 근거를 찾아 답합니다")));
                        ui.add_space(14.0);
                        for s in [crate::i18n::tr("이번 달 할 일만 정리해 줘"), crate::i18n::tr("계약서에서 날짜와 금액을 알려 줘"), crate::i18n::tr("건강검진에서 주의할 점은?")] {
                            if ui.button(s).clicked() {
                                self.question = s.into();
                                self.send_question();
                            }
                        }
                    });
                }
                for m in &msgs {
                    self.chat_bubble(ui, m);
                }
                if self.ask_rx.is_some() || self.kw_rx.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(crate::i18n::tr("문서를 읽고 답을 쓰는 중…")).color(MUTED()));
                    });
                }
            });
        });
    }

    /// 문서 범위: 모든 문서 | 선택 문서 (one or several documents).
    fn ask_scope_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(muted(crate::i18n::tr("문서 범위")));
            if let Some(i) = theme::seg(ui, &[crate::i18n::tr("모든 문서"), crate::i18n::tr("선택 문서")], if self.ask_selected_only { 1 } else { 0 }) {
                self.ask_selected_only = i == 1;
            }
            ui.add_space(6.0);
            if !self.ask_selected_only {
                ui.label(RichText::new(crate::ft!("보관함의 문서 {}개 전체에서 찾습니다", "Tìm trong toàn bộ {} tài liệu của kho", self.lib.live_docs().count())).color(TEXT2()).size(13.0));
                return;
            }
            let picked = self.picked_docs();
            let mut remove = None;
            for (di, title) in &picked {
                let kind = self.lib.docs[*di].kind.clone();
                egui::Frame::default()
                    .fill(ACCENT_SOFT())
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin { left: 6, right: 2, top: 2, bottom: 2 })
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(RichText::new(kind).color(ACCENT()).size(11.5));
                        ui.label(RichText::new(excerpt(title, 24)).color(TEXT()).size(13.0));
                        if ui.add(egui::Button::new(RichText::new("×").color(TEXT2()).size(13.0)).frame(false).min_size(vec2(18.0, 18.0))).on_hover_text(crate::i18n::tr("빼기")).clicked() {
                            remove = Some(self.lib.docs[*di].id.clone());
                        }
                    });
            }
            if let Some(id) = remove {
                self.ask_docs.retain(|x| x != &id);
            }
            let label = if picked.is_empty() { crate::i18n::tr("＋ 문서 고르기") } else { crate::i18n::tr("＋ 문서 추가") };
            ui.menu_button(RichText::new(label).color(ACCENT()), |ui| {
                ui.set_min_width(360.0);
                ui.add(egui::TextEdit::singleline(&mut self.ask_doc_filter).hint_text(crate::i18n::tr("🔍 문서 찾기")).desired_width(340.0));
                let f = self.ask_doc_filter.to_lowercase();
                let docs: Vec<(String, String, String)> = self
                    .lib
                    .live_docs()
                    .filter(|(_, d)| f.is_empty() || d.title.to_lowercase().contains(&f) || d.tags.iter().any(|t| t.to_lowercase().contains(&f)))
                    .map(|(_, d)| (d.id.clone(), d.title.clone(), d.kind.clone()))
                    .collect();
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (id, title, kind) in docs {
                        let mut on = self.ask_docs.contains(&id);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut on, "").changed() {
                                if on {
                                    self.ask_docs.push(id.clone());
                                } else {
                                    self.ask_docs.retain(|x| x != &id);
                                }
                            }
                            kind_badge(ui, &kind, 24.0);
                            ui.label(RichText::new(title).color(TEXT()));
                        });
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(muted(crate::ft!("{}개 선택", "Đã chọn {}", self.ask_docs.len())));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(crate::i18n::tr("닫기")).clicked() {
                            ui.close_menu();
                        }
                        if ui.button(crate::i18n::tr("모두 해제")).clicked() {
                            self.ask_docs.clear();
                        }
                    });
                });
            });
            if picked.is_empty() {
                ui.label(RichText::new(crate::i18n::tr("답할 문서를 골라 주세요")).color(WARN()).size(13.0));
            } else {
                ui.label(RichText::new(if picked.len() == 1 { crate::i18n::tr("이 문서 하나에서만 답합니다").to_string() } else { crate::ft!("고른 {}개 문서에서만 답합니다", "Chỉ trả lời từ {} tài liệu đã chọn", picked.len()) }).color(TEXT2()).size(13.0));
            }
        });
    }

    fn chat_bubble(&mut self, ui: &mut egui::Ui, m: &ChatMessage) {
        let max_w = (ui.available_width() * 0.78).min(820.0);
        if m.role == "user" {
            ui.with_layout(Layout::top_down(Align::Max), |ui| {
                egui::Frame::default().fill(ACCENT()).corner_radius(CornerRadius { nw: 14, ne: 4, sw: 14, se: 14 }).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_max_width(max_w);
                    ui.add(egui::Label::new(RichText::new(&m.text).color(ACCENT_INK())).wrap());
                });
                ui.label(RichText::new(fmt_time(m.ms)).color(MUTED()).size(11.0));
            });
        } else {
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 15.0, PURPLE().gamma_multiply(0.3));
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "✨", FontId::proportional(15.0), PURPLE());
                ui.vertical(|ui| {
                    egui::Frame::default().fill(CARD()).corner_radius(CornerRadius { nw: 4, ne: 14, sw: 14, se: 14 }).inner_margin(Margin::symmetric(14, 12)).show(ui, |ui| {
                        ui.set_max_width(max_w);
                        ui.add(egui::Label::new(RichText::new(&m.text).color(TEXT())).wrap());
                        if !m.sources.is_empty() {
                            ui.add_space(6.0);
                            ui.separator();
                            ui.label(RichText::new(crate::i18n::tr("근거 문서")).color(MUTED()).size(12.0));
                            for (i, s) in m.sources.iter().take(5).enumerate() {
                                let r = ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(format!("[{}]", i + 1)).color(ACCENT()).size(12.5));
                                    ui.label(RichText::new(&s.title).color(TEXT2()).size(12.5));
                                    if s.chunk > 0 {
                                        ui.label(RichText::new(crate::ft!("조각 {}", "Đoạn {}", s.chunk)).color(PURPLE()).size(11.5));
                                    }
                                    ui.label(RichText::new(excerpt(&s.excerpt, 90)).color(MUTED()).size(12.0));
                                });
                                let click = ui.interact(r.response.rect, egui::Id::new(("src", &m.id, i)), Sense::click());
                                if click.on_hover_text(crate::i18n::tr("이 근거가 나온 조각을 보관함에서 보기")).clicked() && !s.doc_id.is_empty() {
                                    self.selected_doc = Some(s.doc_id.clone());
                                    self.highlight = Some((s.doc_id.clone(), s.chunk));
                                    self.page = Page::Library;
                                }
                            }
                        }
                    });
                    for f in &m.flags {
                        ui.label(RichText::new(f).color(if f.starts_with('⚠') { WARN() } else { GOOD() }).size(12.0));
                    }
                    ui.label(RichText::new(format!("{} · {} · {}", m.agent, m.model, fmt_time(m.ms))).color(MUTED()).size(11.0));
                });
            });
        }
        ui.add_space(10.0);
    }

    // ---------------- agents ----------------

    fn page_agents(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if primary_button(ui, crate::i18n::tr("＋ 새 에이전트")).clicked() {
                let mut a = demo::default_agent();
                a.name = crate::i18n::tr("새 에이전트").into();
                a.description = String::new();
                self.agent_edit = Some(a);
            }
            ui.label(muted(crate::i18n::tr("에이전트 = 어떤 문서를 보고, 어떤 말투와 규칙으로 답할지 정해 둔 도우미")));
        });
        ui.add_space(10.0);
        let agents: Vec<Agent> = self.lib.live_agents().map(|(_, a)| a.clone()).collect();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for row in agents.chunks(3) {
                ui.columns(3, |cols| {
                    for (i, a) in row.iter().enumerate() {
                        card().show(&mut cols[i], |ui| {
                            let w = ui.available_width();
                            ui.set_width(w);
                            ui.set_max_width(w);
                            ui.set_min_height(190.0);
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::hover());
                                ui.painter().rect_filled(rect, CornerRadius::same(10), PURPLE().gamma_multiply(0.25));
                                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "✨", FontId::proportional(18.0), PURPLE());
                                ui.vertical(|ui| {
                                    ui.label(bold(&a.name, 16.0));
                                    ui.horizontal_wrapped(|ui| {
                                        chip(ui, &if a.tag_filter.is_empty() { crate::i18n::tr("모든 문서").to_string() } else { format!("#{}", a.tag_filter) }, ACCENT());
                                        chip(ui, &crate::ft!("근거 {}개", "{} căn cứ", a.top_k), MUTED());
                                        chip(ui, format_label(&a.format), PURPLE());
                                        if a.guardrail {
                                            chip(ui, crate::i18n::tr("🔒 개인정보 보호"), GOOD());
                                        }
                                    });
                                });
                            });
                            ui.add_space(6.0);
                            ui.add(egui::Label::new(RichText::new(if a.description.is_empty() { crate::i18n::tr("설명 없음") } else { &a.description }).color(TEXT2())).wrap());
                            ui.add(egui::Label::new(RichText::new(excerpt(&a.instruction, 90)).color(MUTED()).size(12.5)).wrap());
                            let asked = self.lib.chats.iter().filter(|m| m.role == "user" && m.agent == a.name).count();
                            let (tok, cost) = self.lib.usage.iter().filter(|u| u.agent == a.name).fold((0u64, 0f64), |acc, u| (acc.0 + u.prompt_tokens + u.completion_tokens, acc.1 + u.cost_usd));
                            let model = if a.model.trim().is_empty() { ai::model_name(&self.settings) } else { a.model.clone() };
                            egui::Frame::default().fill(FIELD()).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new(crate::ft!("질문 {asked}회 · 토큰 {} · ${cost:.2} · 모델 {}", "{asked} câu hỏi · {} token · ${cost:.2} · mô hình {}", fmt_num(tok), if model.is_empty() { crate::i18n::tr("검색만").into() } else { model }, asked = asked, cost = cost)).size(12.0).color(TEXT2()));
                            });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if primary_button(ui, crate::i18n::tr("💬 질문하기")).clicked() {
                                    self.agent_sel = a.id.clone();
                                    self.page = Page::Ask;
                                }
                                if ui.button(crate::i18n::tr("편집")).clicked() {
                                    self.agent_edit = Some(a.clone());
                                }
                                if self.lib.live_agents().count() > 1 && ui.button(RichText::new(crate::i18n::tr("삭제")).color(BAD())).clicked() {
                                    if let Some(x) = self.lib.agents.iter_mut().find(|x| x.id == a.id) {
                                        x.deleted = true;
                                        x.updated_ms = now_ms();
                                    }
                                    self.lib_dirty = true;
                                }
                            });
                        });
                    }
                });
                ui.add_space(4.0);
            }
        });
    }

    fn agent_editor(&mut self, ctx: &egui::Context) {
        let Some(mut a) = self.agent_edit.take() else { return };
        let mut open = true;
        let mut save = false;
        let tags = self.lib.all_tags();
        let model_opts = self.models_for(self.settings.provider);
        egui::Window::new(crate::i18n::tr("에이전트 설정")).open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).default_width(560.0).show(ctx, |ui| {
            egui::Grid::new("agentform").num_columns(2).spacing(vec2(12.0, 10.0)).show(ui, |ui| {
                ui.label(crate::i18n::tr("이름"));
                ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(400.0));
                ui.end_row();
                ui.label(crate::i18n::tr("설명"));
                ui.add(egui::TextEdit::singleline(&mut a.description).hint_text(crate::i18n::tr("무엇을 돕는지 한 줄로")).desired_width(400.0));
                ui.end_row();
                ui.label(crate::i18n::tr("볼 문서"));
                egui::ComboBox::from_id_salt("agtag")
                    .selected_text(if a.tag_filter.is_empty() { crate::i18n::tr("모든 문서").to_string() } else { crate::ft!("#{} 태그 문서만", "Chỉ tài liệu có thẻ #{}", a.tag_filter) })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut a.tag_filter, String::new(), crate::i18n::tr("모든 문서"));
                        for t in tags {
                            let l = crate::ft!("#{t} 태그 문서만", "Chỉ tài liệu có thẻ #{t}", t = t);
                            ui.selectable_value(&mut a.tag_filter, t, l);
                        }
                    });
                ui.end_row();
                ui.label(crate::i18n::tr("근거 개수"));
                ui.add(egui::Slider::new(&mut a.top_k, 1..=10).text(crate::i18n::tr("개 구절을 찾아 답에 사용")));
                ui.end_row();
                ui.label(crate::i18n::tr("AI 모델"));
                let hint = crate::ft!("비우면 설정의 모델 ({})", "Để trống sẽ dùng mô hình trong cài đặt ({})", ai::model_name(&self.settings));
                ai_page::model_box(ui, "agmodel", &mut a.model, &model_opts, &hint, 330.0);
                ui.end_row();
                ui.label(crate::i18n::tr("답변 형식"));
                egui::ComboBox::from_id_salt("agfmt").selected_text(format_label(&a.format)).show_ui(ui, |ui| {
                    for (k, l, _) in FORMATS {
                        ui.selectable_value(&mut a.format, k.to_string(), l);
                    }
                });
                ui.end_row();
                ui.label(crate::i18n::tr("안전장치"));
                ui.vertical(|ui| {
                    ui.checkbox(&mut a.guardrail, crate::i18n::tr("개인정보를 가리고 AI에 보내기 (답변 속 개인정보도 가림)"));
                    ui.checkbox(&mut a.cite_check, crate::i18n::tr("근거 번호가 없는 답변이면 경고하기"));
                });
                ui.end_row();
            });
            ui.label(crate::i18n::tr("답변 규칙 (AI에게 주는 지시)"));
            ui.add(egui::TextEdit::multiline(&mut a.instruction).desired_rows(6).desired_width(f32::INFINITY));
            ui.add_space(6.0);
            if primary_button(ui, crate::i18n::tr("저장")).clicked() {
                save = true;
            }
        });
        if save {
            a.updated_ms = now_ms();
            if a.name.trim().is_empty() {
                a.name = crate::i18n::tr("이름 없는 에이전트").into();
            }
            match self.lib.agents.iter_mut().find(|x| x.id == a.id) {
                Some(x) => *x = a,
                None => self.lib.agents.push(a),
            }
            self.lib_dirty = true;
            self.toast(crate::i18n::tr("에이전트를 저장했습니다"), GOOD());
        } else if open {
            self.agent_edit = Some(a);
        }
    }

    // ---------------- evaluation ----------------

    fn page_eval(&mut self, ui: &mut egui::Ui) {
        let rate = self.eval_rate();
        ui.columns(3, |c| {
            stat_card(&mut c[0], "🎯", crate::i18n::tr("검색 정답률"), rate.map(|r| format!("{:.0}%", r * 100.0)).unwrap_or_else(|| "-".into()), crate::i18n::tr("정답 구절이 상위 3개 안에 든 비율").into(), GOOD());
            stat_card(&mut c[1], "📝", crate::i18n::tr("평가 질문"), crate::ft!("{}개", "{}", self.lib.live_evals().count()), crate::i18n::tr("질문과 정답에 들어갈 낱말").into(), ACCENT());
            let failed = self.lib.live_evals().filter(|(_, e)| e.last.as_ref().map(|r| !r.passed).unwrap_or(false)).count();
            stat_card(&mut c[2], "⚠", crate::i18n::tr("고쳐야 할 질문"), crate::ft!("{failed}개", "{failed}", failed = failed), crate::i18n::tr("문서를 보강하거나 조각 크기를 바꿔 보세요").into(), if failed > 0 { BAD() } else { MUTED() });
        });
        ui.add_space(6.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(bold(crate::i18n::tr("평가 질문 추가"), 15.0));
                ui.add(egui::TextEdit::singleline(&mut self.eval_q).hint_text(crate::i18n::tr("질문 (예: 전세 보증금은 얼마야?)")).desired_width(260.0));
                ui.add(egui::TextEdit::singleline(&mut self.eval_e).hint_text(crate::i18n::tr("정답 낱말, 쉼표로 구분")).desired_width(170.0));
                if ui.button(crate::i18n::tr("＋ 추가")).clicked() && !self.eval_q.trim().is_empty() {
                    self.lib.evals.push(EvalCase { id: new_id(), question: self.eval_q.trim().into(), expected: self.eval_e.trim().into(), last: None, updated_ms: now_ms(), deleted: false });
                    self.eval_q.clear();
                    self.eval_e.clear();
                    self.lib_dirty = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ai_ok = crate::analyze::ai_ready(&self.settings);
                    let busy = self.eval_rx.is_some();
                    if ui.add_enabled(ai_ok && !busy, egui::Button::new(RichText::new(crate::i18n::tr("✨ AI 답변까지 평가")).color(ACCENT_INK())).fill(PURPLE()))
                        .on_disabled_hover_text(crate::i18n::tr("설정에서 AI를 연결하면 답변의 정확도·근거·개인정보 노출·AI 판정까지 평가합니다"))
                        .clicked()
                    {
                        self.start_eval_ai();
                    }
                    if primary_button(ui, crate::i18n::tr("▶ 검색 평가")).clicked() {
                        self.run_evals();
                        self.toast(crate::i18n::tr("검색 평가를 마쳤습니다"), GOOD());
                    }
                    if busy {
                        ui.spinner();
                        ui.label(RichText::new(crate::ft!("남은 질문 {}개", "Còn {} câu", self.eval_running)).color(PURPLE()));
                    }
                });
            });
        });
        ui.add_space(6.0);
        self.eval_report(ui);
        ui.add_space(6.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let cases: Vec<EvalCase> = self.lib.live_evals().map(|(_, e)| e.clone()).collect();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("evalgrid").num_columns(6).striped(true).spacing(vec2(20.0, 10.0)).min_col_width(60.0).show(ui, |ui| {
                    for h in [crate::i18n::tr("검색"), crate::i18n::tr("질문"), crate::i18n::tr("정답 낱말"), crate::i18n::tr("가장 관련 있던 문서"), crate::i18n::tr("AI 답변"), ""] {
                        ui.label(muted(h));
                    }
                    ui.end_row();
                    for e in &cases {
                        match &e.last {
                            Some(r) if r.passed => chip(ui, crate::i18n::tr("통과"), GOOD()),
                            Some(r) => chip(ui, &crate::ft!("실패 {:.0}%", "Trượt {:.0}%", r.score * 100.0), BAD()),
                            None => chip(ui, crate::i18n::tr("미실행"), MUTED()),
                        }
                        ui.label(RichText::new(&e.question).color(TEXT()));
                        ui.label(RichText::new(&e.expected).color(TEXT2()));
                        ui.label(RichText::new(e.last.as_ref().map(|r| r.found_in.clone()).unwrap_or_default()).color(MUTED()));
                        match e.last.as_ref().and_then(|r| r.judge.map(|j| (j, r.answer_score.unwrap_or(0.0), r.cites.unwrap_or(false), r.pii_safe.unwrap_or(true)))) {
                            Some((j, s, c, p)) => {
                                let r = ui.horizontal(|ui| {
                                    chip(ui, &crate::ft!("판정 {j}/5", "Điểm {j}/5", j = j), if j >= 4 { GOOD() } else if j >= 3 { WARN() } else { BAD() });
                                    ui.label(muted(crate::ft!("답 {:.0}%{}{}", "Đáp án {:.0}%{}{}", s * 100.0, if c { "" } else { crate::i18n::tr(" · 근거 없음") }, if p { "" } else { crate::i18n::tr(" · 개인정보 노출") })));
                                });
                                if let Some(a) = e.last.as_ref().map(|r| r.answer.clone()).filter(|a| !a.is_empty()) {
                                    r.response.on_hover_text(a);
                                }
                            }
                            None => {
                                ui.label(muted("-"));
                            }
                        }
                        if ui.small_button(crate::i18n::tr("삭제")).clicked() {
                            if let Some(x) = self.lib.evals.iter_mut().find(|x| x.id == e.id) {
                                x.deleted = true;
                                x.updated_ms = now_ms();
                            }
                            self.lib_dirty = true;
                        }
                        ui.end_row();
                    }
                });
                if cases.is_empty() {
                    ui.label(muted(crate::i18n::tr("질문을 추가하고 '모두 평가하기'를 누르면 내 문서에서 답을 잘 찾는지 점검합니다.")));
                }
            });
        });
    }

    // ---------------- share ----------------

    fn page_share(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        let left_w = (avail.x * 0.45).floor();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(left_w, avail.y), Layout::top_down(Align::Min), |ui| {
                card().show(ui, |ui| {
                    ui.set_min_size(vec2(left_w - 36.0, avail.y - 40.0));
                    ui.horizontal(|ui| {
                        ui.label(bold(crate::ft!("공유할 문서 ({}개 선택)", "Tài liệu chia sẻ (đã chọn {})", self.share_sel.len()), 15.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button(crate::i18n::tr("선택 해제")).clicked() {
                                self.share_sel.clear();
                            }
                        });
                    });
                    ui.add(egui::TextEdit::singleline(&mut self.share_filter).hint_text(crate::i18n::tr("🔍 문서 찾기")).desired_width(f32::INFINITY));
                    ui.add_space(4.0);
                    let f = self.share_filter.to_lowercase();
                    let docs: Vec<Document> = self.lib.live_docs().filter(|(_, d)| f.is_empty() || d.title.to_lowercase().contains(&f)).map(|(_, d)| d.clone()).collect();
                    egui::ScrollArea::vertical().id_salt("sharelist").auto_shrink([false, false]).show(ui, |ui| {
                        for d in docs {
                            let mut on = self.share_sel.contains(&d.id);
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut on, "").changed() {
                                    if on {
                                        self.share_sel.insert(d.id.clone());
                                    } else {
                                        self.share_sel.remove(&d.id);
                                    }
                                }
                                kind_badge(ui, &d.kind, 28.0);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&d.title).color(TEXT()));
                                    let original = d.has_file && self.store.file_path(&d).exists();
                                    ui.label(muted(if original { crate::ft!("원본 {} · {}", "Bản gốc {} · {}", d.file_name, fmt_size(d.size_bytes)) } else { crate::i18n::tr("본문을 텍스트 파일로 공유").into() }));
                                });
                            });
                            ui.add_space(2.0);
                        }
                    });
                });
            });
            ui.add_space(4.0);
            let right_w = avail.x - left_w - 12.0;
            ui.allocate_ui_with_layout(vec2(right_w, avail.y), Layout::top_down(Align::Min), |ui| {
                egui::ScrollArea::vertical().id_salt("shareright").auto_shrink([false, false]).show(ui, |ui| {
                    card().show(ui, |ui| {
                        ui.set_width(right_w - 36.0);
                        ui.label(bold(crate::i18n::tr("공유 페이지 꾸미기"), 15.0));
                        ui.add_space(4.0);
                        egui::Grid::new("shareform").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                            ui.label(crate::i18n::tr("제목"));
                            ui.add(egui::TextEdit::singleline(&mut self.share_title).desired_width(right_w - 150.0));
                            ui.end_row();
                            ui.label(crate::i18n::tr("전할 말"));
                            ui.add(egui::TextEdit::multiline(&mut self.share_msg).desired_rows(2).desired_width(right_w - 150.0));
                            ui.end_row();
                            ui.label(crate::i18n::tr("PIN (선택)"));
                            ui.add(egui::TextEdit::singleline(&mut self.share_pin).hint_text(crate::i18n::tr("비워 두면 링크만으로 열람")).desired_width(200.0));
                            ui.end_row();
                            ui.label(crate::i18n::tr("열람 기간"));
                            ui.horizontal(|ui| {
                                for (h, l) in [(1u32, crate::i18n::tr("1시간")), (24, crate::i18n::tr("하루")), (72, crate::i18n::tr("3일")), (168, crate::i18n::tr("1주일"))] {
                                    ui.selectable_value(&mut self.share_hours, h, l);
                                }
                            });
                            ui.end_row();
                        });
                        ui.add_space(8.0);
                        let can = !self.share_sel.is_empty();
                        ui.horizontal(|ui| {
                            let live_btn = ui.add_enabled(can && self.live.is_none(), egui::Button::new(RichText::new(crate::i18n::tr("📡 같은 네트워크에 공유 시작")).color(ACCENT_INK())).fill(ACCENT()));
                            if live_btn.clicked() {
                                let items = self.share_items();
                                match share::start_live(self.share_title.clone(), self.share_msg.clone(), items, self.share_pin.trim().to_string(), self.share_hours) {
                                    Ok(l) => {
                                        self.toast(crate::i18n::tr("공유를 시작했습니다. 링크를 보내 주세요"), GOOD());
                                        self.live = Some(l);
                                    }
                                    Err(e) => self.toast(e, BAD()),
                                }
                            }
                            if ui.add_enabled(can, egui::Button::new(crate::i18n::tr("💾 공유 페이지를 폴더로 저장"))).clicked() {
                                if let Some(dir) = rfd::FileDialog::new().set_title(crate::i18n::tr("공유 폴더를 만들 곳 (클라우드 폴더 추천)")).pick_folder() {
                                    match share::export_folder(&dir, &self.share_title, &self.share_msg, &self.share_items()) {
                                        Ok(p) => {
                                            self.toast(crate::i18n::tr("공유 페이지 폴더를 만들었습니다"), GOOD());
                                            self.last_export = Some(p);
                                        }
                                        Err(e) => self.toast(e, BAD()),
                                    }
                                }
                            }
                        });
                        ui.label(muted(crate::i18n::tr("· 같은 와이파이의 사람: '공유 시작' 후 링크 전달 (처음 한 번 방화벽 허용 창이 뜰 수 있음)")));
                        ui.label(muted(crate::i18n::tr("· 멀리 있는 사람: '폴더로 저장'을 구글 드라이브 등에 만든 뒤 그 폴더의 공유 링크 전달")));
                        if let Some(p) = self.last_export.clone() {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(crate::ft!("저장됨: {}", "Đã lưu: {}", p.display())).color(GOOD()).size(12.5));
                                if ui.small_button(crate::i18n::tr("열기")).clicked() {
                                    let _ = open::that(p.join("index.html"));
                                }
                            });
                        }
                    });
                    ui.add_space(6.0);
                    self.live_card(ui, right_w);
                });
            });
        });
    }

    fn live_card(&mut self, ui: &mut egui::Ui, right_w: f32) {
        let mut stop = false;
        card().show(ui, |ui| {
            ui.set_width(right_w - 36.0);
            match &self.live {
                None => {
                    ui.label(bold(crate::i18n::tr("공유 중인 페이지 없음"), 15.0));
                    ui.label(muted(crate::i18n::tr("공유를 시작하면 여기에 링크와 열람 기록이 보입니다.")));
                    ui.add_space(6.0);
                    // preview of what the visitor will see
                    egui::Frame::default().fill(SIDEBAR()).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(16)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(crate::i18n::tr("받는 사람 화면 미리보기")).color(MUTED()).size(11.5));
                        chip(ui, "공유 문서", ACCENT());
                        ui.label(bold(&self.share_title, 20.0));
                        if !self.share_msg.is_empty() {
                            ui.label(RichText::new(&self.share_msg).color(TEXT2()));
                        }
                        for (_, d) in self.lib.live_docs().filter(|(_, d)| self.share_sel.contains(&d.id)).take(4) {
                            egui::Frame::default().fill(BG()).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    kind_badge(ui, &d.kind, 28.0);
                                    ui.label(RichText::new(if d.has_file { d.file_name.clone() } else { format!("{}.txt", d.title) }).color(TEXT()));
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.add(egui::Button::new(RichText::new(crate::i18n::tr("내려받기")).color(ACCENT_INK()).size(12.5)).fill(ACCENT()).sense(Sense::hover()));
                                    });
                                });
                            });
                        }
                    });
                }
                Some(l) => {
                    ui.horizontal(|ui| {
                        ui.label(bold(crate::i18n::tr("● 공유 중"), 15.0).color(GOOD()));
                        ui.label(muted(crate::ft!("'{}' · 파일 {}개 · {}까지", "'{}' · {} tệp · đến {}", l.title, l.files, fmt_time(l.expires_ms))));
                    });
                    ui.add_space(4.0);
                    egui::Frame::default().fill(FIELD()).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(10)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(egui::Label::new(RichText::new(&l.url).monospace().color(ACCENT()).size(14.0)).selectable(true));
                    });
                    ui.horizontal(|ui| {
                        if ui.button(crate::i18n::tr("📋 링크 복사")).clicked() {
                            ui.ctx().copy_text(if l.pin.is_empty() { l.url.clone() } else { format!("{} (PIN: {})", l.url, l.pin) });
                        }
                        if ui.button(crate::i18n::tr("🌐 브라우저로 보기")).clicked() {
                            let _ = open::that(&l.url);
                        }
                        if ui.add(egui::Button::new(RichText::new(crate::i18n::tr("■ 공유 끝내기")).color(BAD()))).clicked() {
                            stop = true;
                        }
                    });
                    use std::sync::atomic::Ordering;
                    ui.label(RichText::new(crate::ft!("열람 {}회 · 내려받기 {}회{}", "{} lượt xem · {} lượt tải{}", l.visits.load(Ordering::SeqCst), l.downloads.load(Ordering::SeqCst), if l.expired() { crate::i18n::tr(" · 기간 끝남") } else { "" })).color(TEXT2()));
                    if let Ok(log) = l.log.lock() {
                        for line in log.iter().rev().take(6) {
                            ui.label(muted(line.clone()));
                        }
                    }
                }
            }
        });
        if stop {
            if let Some(l) = self.live.take() {
                l.stop();
            }
            self.toast(crate::i18n::tr("공유를 끝냈습니다"), MUTED());
        }
    }

    // ---------------- sync ----------------

    fn cloud_suggestions() -> Vec<(String, PathBuf)> {
        let mut out = Vec::new();
        let home = dirs::home_dir().unwrap_or_default();
        let mut cand: Vec<(String, PathBuf)> = vec![
            ("OneDrive".into(), home.join("OneDrive")),
            ("Dropbox".into(), home.join("Dropbox")),
            ("iCloud Drive".into(), home.join("iCloudDrive")),
            ("Google Drive".into(), home.join("Google Drive")),
            ("iCloud Drive".into(), home.join("Library/Mobile Documents/com~apple~CloudDocs")),
        ];
        for drive in ["G:\\내 드라이브", "G:\\My Drive", "H:\\내 드라이브", "H:\\My Drive"] {
            cand.push(("Google Drive".into(), PathBuf::from(drive)));
        }
        if let Ok(rd) = std::fs::read_dir(home.join("Library/CloudStorage")) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                let label = n.split('-').next().unwrap_or(&n).to_string();
                cand.push((label, e.path()));
            }
        }
        for (l, p) in cand {
            if p.is_dir() && !out.iter().any(|(_, q): &(String, PathBuf)| q == &p) {
                out.push((l, p));
            }
        }
        out
    }

    fn page_sync(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |c| {
                card().show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(bold(crate::i18n::tr("데이터를 어디까지 보낼까요?"), 16.0));
                    ui.add_space(6.0);
                    let before = self.settings.local_only;
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.settings.local_only, true, crate::i18n::tr("🔒 내 PC 전용 (기본)"));
                        ui.selectable_value(&mut self.settings.local_only, false, crate::i18n::tr("☁ 같은 계정 기기와 동기화"));
                    });
                    if before != self.settings.local_only {
                        self.settings_dirty = true;
                        if self.settings.local_only {
                            self.account = None;
                            self.sync_msg = crate::i18n::tr("내 PC 전용 모드로 돌아왔습니다. 동기화를 멈췄습니다.").into();
                        }
                    }
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(RichText::new(if self.settings.local_only {
                        crate::i18n::tr("문서·대화·설정이 이 컴퓨터에만 저장되고, 클라우드 AI 호출과 동기화도 막힙니다. 내 PC에서 도는 Ollama는 쓸 수 있습니다.")
                    } else {
                        crate::i18n::tr("같은 계정 이름과 암호를 넣은 PC·Mac끼리 데이터를 맞춥니다. 파일은 암호화되어 클라우드 폴더에 저장되므로 클라우드 회사도 내용을 볼 수 없습니다.")
                    }).color(TEXT2())).wrap());
                    ui.add_space(8.0);
                    ui.label(muted(crate::i18n::tr("이 컴퓨터의 데이터 폴더")));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(self.store.dir.display().to_string()).monospace().size(12.5));
                        if ui.small_button(crate::i18n::tr("열기")).clicked() {
                            let _ = open::that(&self.store.dir);
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(muted(crate::i18n::tr("이 기기 이름")));
                        if ui.add(egui::TextEdit::singleline(&mut self.settings.device_name).desired_width(180.0)).changed() {
                            self.settings_dirty = true;
                        }
                    });
                });
                card().show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(bold(crate::i18n::tr("연결된 기기"), 16.0));
                    ui.add_space(6.0);
                    if self.lib.devices.is_empty() {
                        ui.label(muted(crate::i18n::tr("아직 동기화한 기기가 없습니다.")));
                    }
                    for d in self.lib.devices.clone() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(if d.os == "macOS" { "💻" } else { "🖥" }).size(18.0));
                            ui.vertical(|ui| {
                                let me = d.name == self.settings.device_name;
                                ui.label(RichText::new(format!("{}{}", d.name, if me { crate::i18n::tr(" (이 기기)") } else { "" })).color(TEXT()));
                                ui.label(muted(crate::ft!("{} · 마지막 동기화 {}", "{} · đồng bộ gần nhất {}", d.os, fmt_time(d.last_sync_ms))));
                            });
                        });
                    }
                });
            });
            ui.add_space(6.0);
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(bold(crate::i18n::tr("계정 연결"), 16.0));
                ui.label(muted(crate::i18n::tr("서버 가입 없이, 이미 쓰는 클라우드 폴더(구글 드라이브·OneDrive·Dropbox·iCloud)를 통해 기기끼리 데이터를 맞춥니다.")));
                ui.add_space(8.0);
                let disabled = self.settings.local_only;
                ui.add_enabled_ui(!disabled, |ui| {
                    egui::Grid::new("acct").num_columns(2).spacing(vec2(14.0, 10.0)).show(ui, |ui| {
                        ui.label(crate::i18n::tr("계정 이름"));
                        if ui.add(egui::TextEdit::singleline(&mut self.settings.account_id).hint_text(crate::i18n::tr("예: my-desk")).desired_width(260.0)).changed() {
                            self.settings_dirty = true;
                            self.account = None;
                        }
                        ui.end_row();
                        ui.label(crate::i18n::tr("암호"));
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.pass_input).password(true).hint_text(crate::i18n::tr("6자 이상, 모든 기기에서 같게")).desired_width(260.0));
                            if ui.checkbox(&mut self.settings.remember_pass, crate::i18n::tr("이 기기에 기억")).changed() {
                                self.settings_dirty = true;
                            }
                        });
                        ui.end_row();
                        ui.label(crate::i18n::tr("클라우드 폴더"));
                        ui.horizontal(|ui| {
                            if ui.add(egui::TextEdit::singleline(&mut self.settings.sync_folder).hint_text(crate::i18n::tr("동기화되는 폴더를 고르세요")).desired_width(360.0)).changed() {
                                self.settings_dirty = true;
                            }
                            if ui.button(crate::i18n::tr("폴더 선택")).clicked() {
                                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                                    self.settings.sync_folder = p.display().to_string();
                                    self.settings_dirty = true;
                                }
                            }
                        });
                        ui.end_row();
                        let sug = Self::cloud_suggestions();
                        if !sug.is_empty() {
                            ui.label(muted(crate::i18n::tr("찾은 폴더")));
                            ui.horizontal_wrapped(|ui| {
                                for (l, p) in sug {
                                    if ui.small_button(format!("☁ {l}")).on_hover_text(p.display().to_string()).clicked() {
                                        self.settings.sync_folder = p.display().to_string();
                                        self.settings_dirty = true;
                                    }
                                }
                            });
                            ui.end_row();
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if self.account.is_none() {
                            if primary_button(ui, crate::i18n::tr("🔑 계정 연결하고 동기화")).clicked() {
                                self.connect_account();
                            }
                        } else {
                            if primary_button(ui, crate::i18n::tr("🔄 지금 동기화")).clicked() {
                                self.do_sync();
                            }
                            if ui.button(crate::i18n::tr("연결 해제")).clicked() {
                                self.account = None;
                                self.sync_msg = crate::i18n::tr("연결을 해제했습니다").into();
                            }
                        }
                        if ui.checkbox(&mut self.settings.auto_sync, crate::i18n::tr("5분마다 자동 동기화")).changed() {
                            self.settings_dirty = true;
                        }
                    });
                });
                if disabled {
                    ui.label(RichText::new(crate::i18n::tr("위에서 '같은 계정 기기와 동기화'를 고르면 쓸 수 있습니다.")).color(WARN()));
                }
                if !self.sync_msg.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&self.sync_msg).color(TEXT2()));
                }
                if self.settings.last_sync_ms > 0 {
                    ui.label(muted(crate::ft!("마지막 동기화: {}", "Đồng bộ gần nhất: {}", fmt_time(self.settings.last_sync_ms))));
                }
            });
        });
    }

    // ---------------- settings ----------------

    fn page_settings(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(bold(crate::i18n::tr("AI 연결"), 16.0));
                    ui.label(RichText::new(crate::ft!("지금: {}", "Hiện tại: {}", self.settings.provider.label())).color(TEXT2()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if primary_button(ui, crate::i18n::tr("AI 연결 열기")).clicked() {
                            self.page = Page::AiConnect;
                        }
                    });
                });
                ui.label(muted(crate::i18n::tr("구독 계정(Claude·ChatGPT), 내 PC의 AI(LM Studio·Ollama), API 키 중에서 고르고 모델을 정하는 화면이 따로 있습니다.")));
            });
            ui.add_space(6.0);
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(bold(crate::i18n::tr("문서 처리"), 16.0));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::tr("지식 조각 크기"));
                    let r = ui.add(egui::Slider::new(&mut self.settings.chunk_size, 200..=2000).step_by(50.0).suffix(crate::i18n::tr("자")));
                    if r.changed() {
                        self.settings_dirty = true;
                        self.index_dirty = true;
                    }
                });
                ui.label(muted(crate::i18n::tr("작게 하면 정확한 구절을, 크게 하면 앞뒤 맥락을 더 많이 찾습니다. 바꾸면 바로 다시 나눕니다.")));
                ui.add_space(4.0);
                ui.add_space(6.0);
                ui.label(bold(crate::i18n::tr("문서 해석 수준"), 14.5));
                let vision_on = self.settings.vision && self.settings.provider != Provider::None;
                egui::Grid::new("caps").num_columns(3).striped(true).spacing(vec2(18.0, 6.0)).show(ui, |ui| {
                    let rows: [(&str, &str, Color32, &str); 8] = [
                        (crate::i18n::tr("한글 HWP (구버전 5.x)"), crate::i18n::tr("지원"), GOOD(), crate::i18n::tr("본문·표·글상자를 직접 읽음. 암호 문서는 불가, 배포용 문서는 미리보기 글자만")),
                        (crate::i18n::tr("한글 HWPX · Word · PowerPoint · ODT"), crate::i18n::tr("지원"), GOOD(), crate::i18n::tr("문단과 표 구조를 그대로 보존")),
                        (crate::i18n::tr("표 안의 표"), crate::i18n::tr("지원"), GOOD(), crate::i18n::tr("바깥 표의 칸 안에 [표: …]로 남겨 문맥이 섞이지 않게 함")),
                        ("Excel", crate::i18n::tr("지원"), GOOD(), crate::i18n::tr("시트별로 행·열을 표로 옮김")),
                        (crate::i18n::tr("차트 (Word·PPT·Excel 안)"), crate::i18n::tr("지원"), GOOD(), crate::i18n::tr("차트에 담긴 항목과 수치를 AI 없이 직접 꺼냄")),
                        (crate::i18n::tr("그림·사진·그림으로 된 차트"), if vision_on { crate::i18n::tr("켜짐") } else { crate::i18n::tr("AI 필요") }, if vision_on { GOOD() } else { WARN() }, crate::i18n::tr("그림을 볼 수 있는 AI가 설명과 수치를 글로 바꿔 넣음 (문서당 최대 8장)")),
                        (crate::i18n::tr("그림 파일 (PNG·JPG) · 문서 사진"), if vision_on { crate::i18n::tr("켜짐") } else { crate::i18n::tr("AI 필요") }, if vision_on { GOOD() } else { WARN() }, crate::i18n::tr("AI 그림 해석으로 글자를 받아 적음")),
                        (crate::i18n::tr("스캔한 PDF"), crate::i18n::tr("미지원"), BAD(), crate::i18n::tr("PDF 속 그림을 꺼내는 기능이 아직 없음 — 페이지를 그림으로 저장해 넣으면 해석 가능")),
                    ];
                    for (what, state, col, how) in rows {
                        ui.label(RichText::new(what).color(TEXT()));
                        chip(ui, state, col);
                        ui.label(muted(how));
                        ui.end_row();
                    }
                });
            });
            ui.add_space(6.0);
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(bold(crate::i18n::tr("화면"), 16.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(crate::i18n::tr("테마")).color(TEXT2()));
                    for (k, l) in [("system", crate::i18n::tr("시스템 따라가기")), ("light", crate::i18n::tr("라이트")), ("dark", crate::i18n::tr("다크"))] {
                        if ui.selectable_label(self.settings.theme == k, l).clicked() {
                            self.settings.theme = k.into();
                            self.settings_dirty = true;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(crate::i18n::tr("언어")).color(TEXT2()));
                    for (k, l) in [("ko", "한국어"), ("vi", "Tiếng Việt")] {
                        if ui.selectable_label(self.settings.language == k, l).clicked() {
                            self.settings.language = k.into();
                            crate::i18n::set_vi(k == "vi");
                            self.settings_dirty = true;
                        }
                    }
                });
                ui.label(muted(crate::i18n::tr("ALMUS WorkBoard와 같은 색·글꼴(Inter, Noto Sans KR)·모서리 규칙을 따릅니다.")));
                ui.add_space(8.0);
                ui.label(bold(crate::i18n::tr("정보"), 16.0));
                ui.label(muted(crate::ft!("MyDocDesk {} · {} · 데이터 폴더 {}", "MyDocDesk {} · {} · thư mục dữ liệu {}", env!("CARGO_PKG_VERSION"), os_name(), self.store.dir.display())));
            });
        });
    }

    fn toast_area(&mut self, ctx: &egui::Context) {
        let Some((msg, col, t)) = self.toast.clone() else { return };
        if t.elapsed() > Duration::from_secs(5) {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(300));
        egui::Area::new(egui::Id::new("toast")).anchor(Align2::RIGHT_BOTTOM, [-24.0, -24.0]).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::default().fill(CARD_HI()).stroke(Stroke::new(1.0, col)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
                ui.set_max_width(460.0);
                ui.add(egui::Label::new(RichText::new(msg).color(TEXT())).wrap());
            });
        });
    }
}

fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}



/// Passages for an answer limited to the picked documents. Every picked document takes part:
/// when the question shares no words with one ("요약해 줘"), its opening passages are used,
/// more of them when only one document was picked.
fn pick_passages(index: &Index, q: &str, picked: &[usize], top_k: usize) -> Vec<(usize, f32)> {
    let k = top_k.max(picked.len().min(8));
    let mut hits = index.search(q, k, |di| picked.contains(&di));
    let per_doc = if picked.len() == 1 { top_k.max(4) } else { 2 };
    for &di in picked {
        let mut have = hits.iter().filter(|(ci, _)| index.chunks[*ci].doc == di).count();
        let want = if picked.len() == 1 { per_doc } else { 1 };
        if have >= want {
            continue;
        }
        for (ci, _) in index.chunks.iter().enumerate().filter(|(_, c)| c.doc == di) {
            if have >= per_doc {
                break;
            }
            if !hits.iter().any(|(x, _)| *x == ci) {
                hits.push((ci, 0.0));
                have += 1;
            }
        }
    }
    hits.truncate(14);
    hits
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[test]
    fn picked_documents_only_and_each_one_speaks() {
        let lib = demo::demo();
        let ix = Index::build(&lib, 600);
        let pos = |t: &str| lib.docs.iter().position(|d| d.title == t).unwrap();
        let (contract, meeting) = (pos("전세 계약 체크리스트"), pos("11월 프로젝트 회의록"));
        // a question that matches neither document still uses both
        let hits = pick_passages(&ix, "요약해 줘", &[contract, meeting], 5);
        let docs: std::collections::HashSet<usize> = hits.iter().map(|(c, _)| ix.chunks[*c].doc).collect();
        assert_eq!(docs, [contract, meeting].into_iter().collect());
        // a matching question never leaks other documents
        let hits = pick_passages(&ix, "보증금 출시일 콜레스테롤", &[contract], 5);
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|(c, _)| ix.chunks[*c].doc == contract));
    }
}
