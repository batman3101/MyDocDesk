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

/// Ids become file names, so only short plain ones are accepted (new_id() makes hex).
/// Documents from other devices with anything else are not taken in.
pub fn is_safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A SHA-256 as written by sha256_hex (anything else is treated as "not recorded").
pub fn is_sha(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// SHA-256 of some bytes, as 64 hex letters.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{:02x}", b)).collect()
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
    /// SHA-256 of that original (hex). Empty for documents made before it was recorded.
    pub file_sha: String,
    pub deleted: bool,
}

impl Document {
    /// `<id>.<ext>` inside the files folder. Both parts are reduced to plain letters, digits,
    /// '-' and '_' so no id or file name can point outside that folder.
    pub fn stored_file_name(&self) -> String {
        let plain = |s: &str, max: usize| -> String { s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(max).collect() };
        let ext = Path::new(&self.file_name).extension().map(|e| plain(&e.to_string_lossy(), 10)).filter(|e| !e.is_empty()).unwrap_or_else(|| "bin".into());
        let id = plain(&self.id, 64);
        format!("{}.{}", if id.is_empty() { "invalid".to_string() } else { id }, ext)
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
    /// The 4-digit lock (the app's login): a salted hash, empty = not picked yet. This device only.
    pub lock_pin: String,
    /// This installation's own id (its library file in the sync folder). Made once, never synced.
    pub device_id: String,
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
            lock_pin: String::new(),
            device_id: new_id(),
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

/// What reading a data file gave: `value` None = no usable file (a fresh start).
pub struct Loaded<T> {
    pub value: Option<T>,
    /// Something was wrong with the file (tell the person).
    pub warning: Option<String>,
    /// The file couldn't be read nor kept aside: don't write over it this run.
    pub block_writes: bool,
}

/// A place for secrets (API keys, the remembered sync password) outside settings.json.
pub trait Secrets: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    fn delete(&self, key: &str) -> Result<(), String>;
}

/// Secrets kept in memory only (demo/screenshots and tests: the OS store is never touched).
#[derive(Default)]
pub struct MemSecrets(pub std::sync::Mutex<std::collections::HashMap<String, String>>);

impl Secrets for MemSecrets {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().map_err(|e| e.to_string())?.get(key).cloned())
    }
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.0.lock().map_err(|e| e.to_string())?.insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        self.0.lock().map_err(|e| e.to_string())?.remove(key);
        Ok(())
    }
}

/// The system's own secret store: Windows Credential Manager or the macOS Keychain.
#[cfg(any(windows, target_os = "macos"))]
pub struct OsSecrets;

#[cfg(any(windows, target_os = "macos"))]
impl OsSecrets {
    const SERVICE: &'static str = "MyDocDesk";
    fn entry(key: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(Self::SERVICE, key).map_err(|e| e.to_string())
    }
}

#[cfg(any(windows, target_os = "macos"))]
impl Secrets for OsSecrets {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        match Self::entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        Self::entry(key)?.set_password(value).map_err(|e| e.to_string())
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Puts one secret in the store (or removes it when `value` is None). True when the file
/// doesn't need to hold it: stored, or nothing to store. Unchanged values aren't rewritten.
fn put_secret(sec: &dyn Secrets, key: &str, value: Option<&str>) -> bool {
    match value {
        Some(v) => sec.get(key).ok().flatten().as_deref() == Some(v) || sec.set(key, v).is_ok(),
        None => {
            let _ = sec.delete(key);
            true
        }
    }
}

/// Where this device keeps its data.
#[derive(Clone)]
pub struct Store {
    pub dir: PathBuf,
    /// Demo mode (screenshots): nothing is written.
    pub read_only: bool,
    /// Where secrets go instead of settings.json (None: no secret store on this system).
    pub secrets: Option<std::sync::Arc<dyn Secrets>>,
}

impl Store {
    pub fn open() -> Store {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        let dir = base.join("MyDocDesk");
        let _ = std::fs::create_dir_all(dir.join("files"));
        #[cfg(any(windows, target_os = "macos"))]
        let secrets: Option<std::sync::Arc<dyn Secrets>> = Some(std::sync::Arc::new(OsSecrets));
        #[cfg(not(any(windows, target_os = "macos")))]
        let secrets: Option<std::sync::Arc<dyn Secrets>> = None;
        Store { dir, read_only: false, secrets }
    }
    pub fn demo() -> Store {
        Store { dir: std::env::temp_dir().join("MyDocDesk-demo"), read_only: true, secrets: Some(std::sync::Arc::new(MemSecrets::default())) }
    }
    /// A writable store in `dir` (no secret store). Tests use it.
    #[cfg(test)]
    pub fn at(dir: PathBuf) -> Store {
        Store { dir, read_only: false, secrets: None }
    }
    #[cfg(test)]
    pub fn with_secrets(mut self, s: std::sync::Arc<dyn Secrets>) -> Store {
        self.secrets = Some(s);
        self
    }
    pub fn files_dir(&self) -> PathBuf {
        self.dir.join("files")
    }
    pub fn file_path(&self, d: &Document) -> PathBuf {
        self.files_dir().join(d.stored_file_name())
    }
    fn parse<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<Option<T>, String> {
        match std::fs::read(path) {
            Ok(b) => serde_json::from_slice(&b).map(Some).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    /// Reads a data file. A damaged or unreadable file is never taken as "empty": it is copied
    /// aside (`<name>.corrupt-<time>`), and the backup from the save before is used if it reads.
    fn load<T: for<'a> Deserialize<'a>>(&self, name: &str) -> Loaded<T> {
        let path = self.dir.join(name);
        let err = match Self::parse::<T>(&path) {
            Ok(v) => return Loaded { value: v, warning: None, block_writes: false },
            Err(e) => e,
        };
        let aside = self.dir.join(format!("{name}.corrupt-{}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
        let kept = std::fs::copy(&path, &aside).is_ok();
        let backup = Self::parse::<T>(&self.dir.join(format!("{name}.bak"))).ok().flatten();
        let shown = aside.display().to_string();
        let warning = match (&backup, kept) {
            (Some(_), true) => crate::ft!("{name} 파일이 손상되어 직전 백업으로 되살렸습니다 (손상본: {shown}) — {err}", "Tệp {name} bị hỏng nên đã khôi phục từ bản sao lưu trước (bản hỏng: {shown}) — {err}", name = name, shown = shown, err = err),
            (None, true) => crate::ft!("{name} 파일이 손상되어 빈 상태로 시작합니다. 손상본은 {shown}에 보관했습니다 — {err}", "Tệp {name} bị hỏng nên bắt đầu trống. Bản hỏng được giữ ở {shown} — {err}", name = name, shown = shown, err = err),
            (_, false) => crate::ft!("{name} 파일을 읽지 못해 이번 실행에서는 저장하지 않습니다 — {err}", "Không đọc được tệp {name} nên lần chạy này sẽ không lưu — {err}", name = name, err = err),
        };
        Loaded { value: backup, warning: Some(warning), block_writes: !kept }
    }
    /// Writes through a temporary file, then keeps the same bytes as `<name>.bak`: the backup is
    /// always the last save that worked, so a file damaged afterwards loses nothing.
    fn write_json<T: Serialize>(&self, name: &str, v: &T) -> Result<(), String> {
        if self.read_only {
            return Ok(());
        }
        let bytes = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
        let path = self.dir.join(name);
        let tmp = self.dir.join(format!("{}.tmp", name));
        std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        let bak_tmp = self.dir.join(format!("{name}.bak.tmp"));
        if std::fs::write(&bak_tmp, &bytes).is_ok() {
            let _ = std::fs::rename(&bak_tmp, self.dir.join(format!("{name}.bak")));
        }
        Ok(())
    }
    pub fn load_library(&self) -> Loaded<Library> {
        self.load("library.json")
    }
    pub fn save_library(&self, l: &Library) -> Result<(), String> {
        self.write_json("library.json", l)
    }
    /// Settings, with the secrets filled back in from the secret store.
    pub fn load_settings(&self) -> Loaded<Settings> {
        let mut l = self.load::<Settings>("settings.json");
        if let (Some(s), Some(sec)) = (l.value.as_mut(), &self.secrets) {
            for p in Provider::ALL {
                let id = p.id();
                if s.api_keys.get(id).is_none_or(|k| k.trim().is_empty()) {
                    if let Ok(Some(v)) = sec.get(&format!("api:{id}")) {
                        s.api_keys.insert(id.into(), v);
                    }
                }
            }
            if s.api_key.is_empty() {
                if let Ok(Some(v)) = sec.get("api:legacy") {
                    s.api_key = v;
                }
            }
            if s.remember_pass && s.saved_pass.is_empty() {
                if let Ok(Some(v)) = sec.get("sync-pass") {
                    s.saved_pass = v;
                }
            }
        }
        l
    }
    /// Writes settings.json without the secrets (API keys, remembered sync password): they go
    /// to the secret store. A secret the store won't take stays in the file, so nothing is lost.
    pub fn save_settings(&self, s: &Settings) -> Result<(), String> {
        let mut file = s.clone();
        if let Some(sec) = &self.secrets {
            let sec = sec.as_ref();
            let ids: std::collections::BTreeSet<String> = Provider::ALL.iter().map(|p| p.id().to_string()).chain(s.api_keys.keys().cloned()).collect();
            for id in ids {
                let v = s.api_keys.get(&id).map(String::as_str).filter(|k| !k.trim().is_empty());
                if put_secret(sec, &format!("api:{id}"), v) {
                    file.api_keys.remove(&id);
                }
            }
            if put_secret(sec, "api:legacy", Some(s.api_key.as_str()).filter(|k| !k.trim().is_empty())) {
                file.api_key.clear();
            }
            if put_secret(sec, "sync-pass", (s.remember_pass && !s.saved_pass.is_empty()).then_some(s.saved_pass.as_str())) {
                file.saved_pass.clear();
            }
        }
        self.write_json("settings.json", &file)
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;

    fn temp_store(name: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("mdd-store-{name}-{}", new_id()));
        std::fs::create_dir_all(dir.join("files")).unwrap();
        Store::at(dir)
    }
    fn one_doc(title: &str) -> Library {
        Library { docs: vec![Document { id: new_id(), title: title.into(), ..Default::default() }], ..Default::default() }
    }
    fn corrupt_copies(st: &Store, name: &str) -> Vec<PathBuf> {
        std::fs::read_dir(&st.dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(&format!("{name}.corrupt-"))).collect()
    }

    #[test]
    fn missing_files_are_a_fresh_start_not_an_error() {
        let st = temp_store("missing");
        let l = st.load_library();
        assert!(l.value.is_none() && l.warning.is_none() && !l.block_writes);
        let s = st.load_settings();
        assert!(s.value.is_none() && s.warning.is_none() && !s.block_writes);
    }

    #[test]
    fn corrupt_library_is_kept_aside_and_restored_from_backup() {
        let st = temp_store("corrupt-lib");
        st.save_library(&one_doc("first")).unwrap();
        st.save_library(&one_doc("second")).unwrap(); // the first one becomes the backup
        std::fs::write(st.dir.join("library.json"), b"{truncated").unwrap();
        let l = st.load_library();
        assert_eq!(l.value.expect("restored").docs[0].title, "second", "the last good save is restored");
        assert!(l.warning.is_some(), "the person is told");
        assert!(!l.block_writes);
        let kept = corrupt_copies(&st, "library.json");
        assert_eq!(kept.len(), 1, "the damaged file is kept aside");
        assert_eq!(std::fs::read(&kept[0]).unwrap(), b"{truncated");
    }

    #[test]
    fn corrupt_library_without_backup_keeps_the_damaged_copy() {
        let st = temp_store("corrupt-nobak");
        std::fs::write(st.dir.join("library.json"), b"{truncated").unwrap();
        let l = st.load_library();
        assert!(l.value.is_none(), "nothing to restore");
        assert!(l.warning.is_some());
        assert_eq!(corrupt_copies(&st, "library.json").len(), 1, "saving an empty library later can't destroy it");
    }

    #[test]
    fn corrupt_settings_restore_the_lock_number() {
        let st = temp_store("corrupt-set");
        let s = Settings { lock_pin: "salt$hash".into(), ..Default::default() };
        st.save_settings(&s).unwrap();
        st.save_settings(&s).unwrap();
        std::fs::write(st.dir.join("settings.json"), b"{trunc").unwrap();
        let l = st.load_settings();
        assert_eq!(l.value.expect("restored").lock_pin, "salt$hash");
        assert!(l.warning.is_some());
    }

    #[test]
    fn the_backup_is_always_the_last_save_that_worked() {
        let st = temp_store("bak-latest");
        st.save_library(&one_doc("good")).unwrap();
        std::fs::write(st.dir.join("library.json"), b"{bad").unwrap(); // damaged from outside
        st.save_library(&one_doc("after")).unwrap();
        let bak: Library = serde_json::from_slice(&std::fs::read(st.dir.join("library.json.bak")).unwrap()).unwrap();
        assert_eq!(bak.docs[0].title, "after", "never the damaged bytes, always the newest good save");
        std::fs::write(st.dir.join("library.json"), b"{bad again").unwrap();
        assert_eq!(st.load_library().value.unwrap().docs[0].title, "after");
    }

    struct FailingSecrets;
    impl Secrets for FailingSecrets {
        fn get(&self, _: &str) -> Result<Option<String>, String> {
            Err("no secret store".into())
        }
        fn set(&self, _: &str, _: &str) -> Result<(), String> {
            Err("no secret store".into())
        }
        fn delete(&self, _: &str) -> Result<(), String> {
            Err("no secret store".into())
        }
    }
    fn with_mem(name: &str) -> (Store, std::sync::Arc<MemSecrets>) {
        let mem = std::sync::Arc::new(MemSecrets::default());
        (temp_store(name).with_secrets(mem.clone()), mem)
    }
    fn file_text(st: &Store) -> String {
        std::fs::read_to_string(st.dir.join("settings.json")).unwrap()
    }
    fn secret_settings() -> Settings {
        let mut s = Settings { remember_pass: true, saved_pass: "sync-pass-123".into(), api_key: "legacy-key-123".into(), ..Default::default() };
        s.api_keys.insert("openai".into(), "sk-openai-123".into());
        s.api_keys.insert("gemini".into(), "AIza-gemini-123".into());
        s
    }

    /// API keys and the remembered sync password live in the OS secret store, not in settings.json
    /// (nor in its .bak copy); loading puts them back.
    #[test]
    fn secrets_are_kept_out_of_the_settings_file() {
        let (st, mem) = with_mem("secrets");
        st.save_settings(&secret_settings()).unwrap();
        for f in ["settings.json", "settings.json.bak"] {
            let t = std::fs::read_to_string(st.dir.join(f)).unwrap();
            for secret in ["sk-openai-123", "AIza-gemini-123", "legacy-key-123", "sync-pass-123"] {
                assert!(!t.contains(secret), "{secret} is in {f}");
            }
        }
        assert_eq!(mem.get("api:openai").unwrap().as_deref(), Some("sk-openai-123"));
        let back = st.load_settings().value.unwrap();
        assert_eq!(back.key_for(Provider::OpenAi), "sk-openai-123");
        assert_eq!(back.key_for(Provider::Gemini), "AIza-gemini-123");
        assert_eq!(back.api_key, "legacy-key-123");
        assert_eq!(back.saved_pass, "sync-pass-123");
    }

    /// Settings saved by the earlier version (keys in the file) move into the secret store on the next save.
    #[test]
    fn plaintext_from_before_moves_into_the_secret_store() {
        let (st, mem) = with_mem("migrate");
        std::fs::write(st.dir.join("settings.json"), serde_json::to_vec(&secret_settings()).unwrap()).unwrap();
        let loaded = st.load_settings().value.unwrap();
        assert_eq!(loaded.key_for(Provider::OpenAi), "sk-openai-123");
        st.save_settings(&loaded).unwrap();
        assert!(!file_text(&st).contains("sk-openai-123"));
        assert_eq!(mem.get("api:openai").unwrap().as_deref(), Some("sk-openai-123"));
    }

    /// A key the person removed is removed from the secret store too; a password no longer
    /// remembered is not kept anywhere.
    #[test]
    fn removed_secrets_are_deleted() {
        let (st, mem) = with_mem("remove");
        st.save_settings(&secret_settings()).unwrap();
        let mut s = secret_settings();
        s.api_keys.remove("gemini");
        s.remember_pass = false;
        s.saved_pass.clear();
        st.save_settings(&s).unwrap();
        assert_eq!(mem.get("api:gemini").unwrap(), None);
        assert_eq!(mem.get("sync-pass").unwrap(), None);
        assert_eq!(st.load_settings().value.unwrap().key_for(Provider::Gemini), "");
    }

    /// If the secret store can't be used, keys stay in the file rather than being lost.
    #[test]
    fn keys_are_not_lost_when_the_secret_store_fails() {
        let st = temp_store("failing").with_secrets(std::sync::Arc::new(FailingSecrets));
        st.save_settings(&secret_settings()).unwrap();
        assert_eq!(st.load_settings().value.unwrap().key_for(Provider::OpenAi), "sk-openai-123");
    }
}

#[cfg(all(test, any(windows, target_os = "macos")))]
mod live_secrets {
    use super::*;

    /// `MDD_LIVE=1 cargo test live_os_secret -- --ignored`: writes, reads and removes one test entry
    /// in the real Windows Credential Manager / macOS Keychain.
    #[test]
    #[ignore]
    fn live_os_secret_store_round_trip() {
        if std::env::var("MDD_LIVE").is_err() {
            return;
        }
        let key = format!("test:{}", new_id());
        assert_eq!(OsSecrets.get(&key).unwrap(), None);
        OsSecrets.set(&key, "value-1").unwrap();
        assert_eq!(OsSecrets.get(&key).unwrap().as_deref(), Some("value-1"));
        OsSecrets.delete(&key).unwrap();
        assert_eq!(OsSecrets.get(&key).unwrap(), None);
        OsSecrets.delete(&key).unwrap(); // removing what isn't there is fine
    }
}
