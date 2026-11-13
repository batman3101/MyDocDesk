//! Data the app keeps: the library (synced between devices) and the settings (this device only).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn new_id() -> String {
    let mut b = [0u8; 8];
    let _ = getrandom::getrandom(&mut b);
    let mut s = format!("{:x}", now_ms());
    for x in b {
        s.push_str(&format!("{:02x}", x));
    }
    s
}

pub fn fmt_time(ms: i64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(t) => t.format("%Y-%m-%d %H:%M").to_string(),
        _ => "-".into(),
    }
}

pub fn fmt_day(ms: i64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(t) => t.format("%m/%d").to_string(),
        _ => "-".into(),
    }
}

pub fn fmt_size(b: u64) -> String {
    if b >= 1 << 20 {
        format!("{:.1} MB", b as f64 / (1u64 << 20) as f64)
    } else if b >= 1024 {
        format!("{:.0} KB", b as f64 / 1024.0)
    } else {
        format!("{} B", b)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub file_name: String,
    pub kind: String,
    pub text: String,
    pub tags: Vec<String>,
    pub added_ms: i64,
    pub updated_ms: i64,
    pub size_bytes: u64,
    pub complexity: u8,
    pub tables: u32,
    pub headings: u32,
    /// Tables inside table cells.
    pub nested_tables: u32,
    pub images: u32,
    pub charts: u32,
    /// Pictures an AI described and added to the text.
    pub ai_pictures: u32,
    /// Anything the reader couldn't do fully (e.g. a protected HWP).
    pub parse_note: String,
    /// Where the file came from on this device, and its modified time then (for auto-collect).
    pub source_path: String,
    pub source_mtime: i64,
    /// An original file copy is kept in the files folder.
    pub has_file: bool,
    pub deleted: bool,
}

impl Document {
    pub fn stored_file_name(&self) -> String {
        let ext = Path::new(&self.file_name)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| "bin".into());
        format!("{}.{}", self.id, ext)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instruction: String,
    /// Only documents with this tag (empty = all documents).
    pub tag_filter: String,
    pub top_k: usize,
    /// Model for this agent only (empty = the one in settings).
    pub model: String,
    /// Answer format: free, brief, report, table, json.
    pub format: String,
    /// Hide personal information before it reaches the AI and in its answers.
    pub guardrail: bool,
    /// Warn when an answer gives no evidence numbers.
    pub cite_check: bool,
    pub updated_ms: i64,
    pub deleted: bool,
}

pub const FORMATS: [(&str, &str, &str); 5] = [
    ("free", "자유롭게", ""),
    ("brief", "짧게 핵심만", "세 문장 이내로 짧게 답하세요."),
    ("report", "보고서체", "개조식 보고서처럼 '- '로 시작하는 항목과 '~함/~임' 말투로 정리하세요."),
    ("table", "표로 정리", "가능하면 '항목 | 내용' 형식의 표로 정리하세요."),
    ("json", "JSON (다른 프로그램용)", "설명 없이 {\"answer\": 문자열, \"facts\": [문자열], \"sources\": [번호]} 형식의 JSON 하나만 출력하세요."),
];

pub fn format_label(f: &str) -> &'static str {
    crate::i18n::tr(FORMATS.iter().find(|x| x.0 == f).map(|x| x.1).unwrap_or("자유롭게"))
}

pub fn format_rule(f: &str) -> &'static str {
    FORMATS.iter().find(|x| x.0 == f).map(|x| x.2).unwrap_or("")
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct EvalResult {
    pub passed: bool,
    pub score: f32,
    pub found_in: String,
    pub ms: i64,
    /// Filled by the AI answer check.
    pub answer: String,
    pub answer_score: Option<f32>,
    pub cites: Option<bool>,
    pub pii_safe: Option<bool>,
    pub judge: Option<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct EvalCase {
    pub id: String,
    pub question: String,
    /// Comma separated words the right passage should contain.
    pub expected: String,
    pub last: Option<EvalResult>,
    pub updated_ms: i64,
    pub deleted: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SourceRef {
    pub doc_id: String,
    pub title: String,
    pub excerpt: String,
    pub score: f32,
    /// Which passage of the document (1-based) — for tracing an answer to its source.
    pub chunk: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct ChatMessage {
    pub id: String,
    pub role: String, // "user" | "assistant"
    pub text: String,
    pub sources: Vec<SourceRef>,
    pub agent: String,
    pub model: String,
    pub ms: i64,
    /// Warnings shown under an answer (guardrail, missing evidence).
    pub flags: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct UsageEntry {
    pub id: String,
    pub ms: i64,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cost_usd: f64,
    /// Which agent (or job) used it.
    pub agent: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct PipelineRun {
    pub ms: i64,
    pub folders: usize,
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub failed: usize,
    pub seconds: f32,
    pub errors: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct DeviceInfo {
    pub name: String,
    pub os: String,
    pub last_sync_ms: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Library {
    pub docs: Vec<Document>,
    pub agents: Vec<Agent>,
    pub evals: Vec<EvalCase>,
    pub chats: Vec<ChatMessage>,
    pub usage: Vec<UsageEntry>,
    pub devices: Vec<DeviceInfo>,
    /// Chats cleared at this time are hidden on every device.
    pub chats_cleared_ms: i64,
}

impl Library {
    pub fn live_docs(&self) -> impl Iterator<Item = (usize, &Document)> {
        self.docs.iter().enumerate().filter(|(_, d)| !d.deleted)
    }
    pub fn live_agents(&self) -> impl Iterator<Item = (usize, &Agent)> {
        self.agents.iter().enumerate().filter(|(_, a)| !a.deleted)
    }
    pub fn live_evals(&self) -> impl Iterator<Item = (usize, &EvalCase)> {
        self.evals.iter().enumerate().filter(|(_, e)| !e.deleted)
    }
    pub fn visible_chats(&self) -> impl Iterator<Item = &ChatMessage> {
        let c = self.chats_cleared_ms;
        self.chats.iter().filter(move |m| m.ms > c)
    }
    pub fn all_tags(&self) -> Vec<String> {
        let mut t: Vec<String> = self
            .live_docs()
            .flat_map(|(_, d)| d.tags.iter().cloned())
            .collect();
        t.sort();
        t.dedup();
        t
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum Provider {
    #[default]
    None,
    /// Local AI on this PC.
    Ollama,
    LmStudio,
    LocalOther,
    /// Pay-as-you-go API keys.
    OpenAi,
    Anthropic,
    Gemini,
    /// Subscriptions, through each company's official command-line tool logged in with the account.
    ClaudeSub,
    ChatGptSub,
    GeminiSub,
}

/// How a provider is reached.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Off,
    Local,
    Api,
    Subscription,
}

impl Provider {
    #[allow(dead_code)]
    pub const ALL: [Provider; 10] = [
        Provider::None,
        Provider::ClaudeSub,
        Provider::ChatGptSub,
        Provider::GeminiSub,
        Provider::Ollama,
        Provider::LmStudio,
        Provider::LocalOther,
        Provider::OpenAi,
        Provider::Anthropic,
        Provider::Gemini,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Provider::None => "none",
            Provider::Ollama => "ollama",
            Provider::LmStudio => "lmstudio",
            Provider::LocalOther => "local",
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
            Provider::Gemini => "gemini",
            Provider::ClaudeSub => "claude-sub",
            Provider::ChatGptSub => "chatgpt-sub",
            Provider::GeminiSub => "gemini-sub",
        }
    }
    pub fn label(self) -> &'static str {
        crate::i18n::tr(self.label_ko())
    }
    fn label_ko(self) -> &'static str {
        match self {
            Provider::None => "사용 안 함 (내 PC 검색만)",
            Provider::Ollama => "Ollama (내 PC)",
            Provider::LmStudio => "LM Studio (내 PC)",
            Provider::LocalOther => "기타 로컬 AI 서버 (내 PC)",
            Provider::OpenAi => "OpenAI API (ChatGPT 모델)",
            Provider::Anthropic => "Anthropic API (Claude 모델)",
            Provider::Gemini => "Google Gemini API",
            Provider::ClaudeSub => "Claude 구독 (Claude Code 로그인)",
            Provider::ChatGptSub => "ChatGPT 구독 (Codex 로그인)",
            Provider::GeminiSub => "Gemini 구독 (Gemini CLI 로그인)",
        }
    }
    pub fn short(self) -> &'static str {
        crate::i18n::tr(self.short_ko())
    }
    fn short_ko(self) -> &'static str {
        match self {
            Provider::None => "AI 검색만",
            Provider::Ollama => "Ollama",
            Provider::LmStudio => "LM Studio",
            Provider::LocalOther => "로컬 AI",
            Provider::OpenAi => "OpenAI API",
            Provider::Anthropic => "Claude API",
            Provider::Gemini => "Gemini API",
            Provider::ClaudeSub => "Claude 구독",
            Provider::ChatGptSub => "ChatGPT 구독",
            Provider::GeminiSub => "Gemini 구독",
        }
    }
    pub fn route(self) -> Route {
        match self {
            Provider::None => Route::Off,
            Provider::Ollama | Provider::LmStudio | Provider::LocalOther => Route::Local,
            Provider::OpenAi | Provider::Anthropic | Provider::Gemini => Route::Api,
            Provider::ClaudeSub | Provider::ChatGptSub | Provider::GeminiSub => Route::Subscription,
        }
    }
    /// Questions leave this computer.
    pub fn is_cloud(self) -> bool {
        matches!(self.route(), Route::Api | Route::Subscription)
    }
    pub fn needs_key(self) -> bool {
        self.route() == Route::Api
    }
    pub fn default_url(self) -> &'static str {
        match self {
            Provider::Ollama => "http://localhost:11434/v1",
            Provider::LmStudio => "http://localhost:1234/v1",
            Provider::LocalOther => "http://localhost:8080/v1",
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::Anthropic => "https://api.anthropic.com/v1",
            Provider::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            _ => "",
        }
    }
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Ollama => "llama3.1",
            Provider::OpenAi => "gpt-4o-mini",
            Provider::Anthropic => "claude-sonnet-5-5",
            Provider::Gemini => "gemini-2.5-flash",
            Provider::ClaudeSub => "sonnet",
            _ => "",
        }
    }
    /// Model names to offer when the service can't list them (subscriptions); free text is always allowed.
    pub fn preset_models(self) -> &'static [&'static str] {
        match self {
            Provider::ClaudeSub => &["sonnet", "opus", "haiku"],
            Provider::ChatGptSub => &["gpt-5", "gpt-5-codex", "gpt-5-mini"],
            Provider::GeminiSub => &["gemini-2.5-pro", "gemini-2.5-flash"],
            _ => &[],
        }
    }
    /// Where to read about the official tool or get a key.
    pub fn help_url(self) -> &'static str {
        match self {
            Provider::ClaudeSub => "https://docs.claude.com/en/docs/claude-code/setup",
            Provider::ChatGptSub => "https://developers.openai.com/codex/cli",
            Provider::GeminiSub => "https://github.com/google-gemini/gemini-cli",
            Provider::Ollama => "https://ollama.com/download",
            Provider::LmStudio => "https://lmstudio.ai",
            Provider::OpenAi => "https://platform.openai.com/api-keys",
            Provider::Anthropic => "https://console.anthropic.com/settings/keys",
            Provider::Gemini => "https://aistudio.google.com/apikey",
            _ => "",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// Nothing leaves this computer: no cloud AI, no sync.
    pub local_only: bool,
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
    /// Old single key (kept so earlier settings still load).
    pub api_key: String,
    /// One key per API service, by Provider::id(). Stored on this device only.
    pub api_keys: std::collections::BTreeMap<String, String>,
    /// Where each subscription tool was found (by Provider::id()).
    pub cli_paths: std::collections::BTreeMap<String, String>,
    pub price_in_per_m: f64,
    pub price_out_per_m: f64,
    pub chunk_size: usize,
    /// Describe pictures, charts and scans with a vision-capable AI while importing.
    pub vision: bool,
    pub vision_model: String,
    pub account_id: String,
    pub sync_folder: String,
    pub remember_pass: bool,
    pub saved_pass: String,
    pub auto_sync: bool,
    pub last_sync_ms: i64,
    pub device_name: String,
    /// Folders on this device that are collected automatically.
    pub watch_folders: Vec<String>,
    pub auto_scan: bool,
    pub runs: Vec<PipelineRun>,
    /// Screen theme: system, light or dark.
    pub theme: String,
    /// Screen language: ko or vi.
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            local_only: true,
            provider: Provider::None,
            base_url: String::new(),
            model: String::new(),
            api_key: String::new(),
            api_keys: Default::default(),
            cli_paths: Default::default(),
            price_in_per_m: 3.0,
            price_out_per_m: 15.0,
            chunk_size: 600,
            vision: false,
            vision_model: String::new(),
            account_id: String::new(),
            sync_folder: String::new(),
            remember_pass: false,
            saved_pass: String::new(),
            auto_sync: true,
            last_sync_ms: 0,
            device_name: default_device_name(),
            watch_folders: Vec::new(),
            auto_scan: true,
            runs: Vec::new(),
            theme: "system".into(),
            language: "ko".into(),
        }
    }
}

impl Settings {
    pub fn key_for(&self, p: Provider) -> String {
        self.api_keys.get(p.id()).cloned().filter(|k| !k.trim().is_empty()).unwrap_or_else(|| {
            if matches!(p, Provider::OpenAi | Provider::Anthropic) { self.api_key.clone() } else { String::new() }
        })
    }
}

pub fn os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    }
}

pub fn default_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| format!("내 {}", if cfg!(target_os = "macos") { "Mac" } else { "PC" }))
}

/// Where this device keeps its data.
#[derive(Clone)]
pub struct Store {
    pub dir: PathBuf,
    /// Demo mode (screenshots): nothing is written.
    pub read_only: bool,
}

impl Store {
    pub fn open() -> Store {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        let dir = base.join("MyDocDesk");
        let _ = std::fs::create_dir_all(dir.join("files"));
        Store { dir, read_only: false }
    }
    pub fn demo() -> Store {
        Store { dir: std::env::temp_dir().join("MyDocDesk-demo"), read_only: true }
    }
    pub fn files_dir(&self) -> PathBuf {
        self.dir.join("files")
    }
    pub fn file_path(&self, d: &Document) -> PathBuf {
        self.files_dir().join(d.stored_file_name())
    }
    fn read_json<T: for<'a> Deserialize<'a> + Default>(&self, name: &str) -> T {
        std::fs::read(self.dir.join(name))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    fn write_json<T: Serialize>(&self, name: &str, v: &T) -> Result<(), String> {
        if self.read_only {
            return Ok(());
        }
        let bytes = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
        let tmp = self.dir.join(format!("{}.tmp", name));
        std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, self.dir.join(name)).map_err(|e| e.to_string())
    }
    pub fn load_library(&self) -> Library {
        self.read_json("library.json")
    }
    pub fn save_library(&self, l: &Library) -> Result<(), String> {
        self.write_json("library.json", l)
    }
    pub fn load_settings(&self) -> Option<Settings> {
        let p = self.dir.join("settings.json");
        if !p.exists() {
            return None;
        }
        Some(self.read_json("settings.json"))
    }
    pub fn save_settings(&self, s: &Settings) -> Result<(), String> {
        self.write_json("settings.json", s)
    }
}
