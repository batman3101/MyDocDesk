//! Subscriptions through each company's official command-line tool.
//!
//! Claude, ChatGPT and Gemini subscriptions don't come with an API key for other apps. What
//! they do come with is an official tool you log into with the same account: Claude Code
//! (`claude`), Codex (`codex`) and Gemini CLI (`gemini`). This module finds those tools and asks
//! them one question at a time in their "answer and exit" mode, so answers come out of the
//! person's own subscription. Login tokens are never read or copied: the tools handle login.

use crate::ai::AiReply;
use crate::model::Provider;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct CliInfo {
    pub provider: Provider,
    pub path: Option<PathBuf>,
    pub version: String,
    /// The tool's own login file exists (only its presence is checked, never its contents).
    pub logged_in: Option<bool>,
}

pub fn exe_name(p: Provider) -> &'static str {
    match p {
        Provider::ClaudeSub => "claude",
        Provider::ChatGptSub => "codex",
        Provider::GeminiSub => "gemini",
        _ => "",
    }
}

fn extra_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(h) = dirs::home_dir() {
        v.push(h.join(".local").join("bin"));
        v.push(h.join(".claude").join("local"));
        v.push(h.join(".npm-global").join("bin"));
        v.push(h.join(".lmstudio").join("bin"));
        v.push(h.join(".bun").join("bin"));
        v.push(h.join(".volta").join("bin"));
    }
    if let Ok(a) = std::env::var("APPDATA") {
        v.push(PathBuf::from(a).join("npm"));
    }
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        v.push(PathBuf::from(&l).join("Programs").join("Ollama"));
    }
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        v.push(PathBuf::from(d));
    }
    v
}

/// Finds a program on PATH or in the usual install folders.
pub fn find_exe(name: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat"] } else { &[""] };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend(extra_dirs());
    for d in dirs {
        for e in exts {
            let p = d.join(format!("{name}{e}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

pub fn command(path: &Path) -> Command {
    let mut c = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // no console window
    }
    c
}

/// Runs a command with stdin text and a deadline; returns (exit ok, stdout, stderr).
pub fn run_with_input(mut cmd: Command, input: &str, timeout: Duration) -> Result<(bool, String, String), String> {
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| crate::ft!("프로그램을 실행하지 못했습니다: {e}", "Không chạy được chương trình: {e}", e = e))?;
    if let Some(mut si) = child.stdin.take() {
        let data = input.as_bytes().to_vec();
        std::thread::spawn(move || {
            let _ = si.write_all(&data);
        });
    }
    let mut so = child.stdout.take();
    let mut se = child.stderr.take();
    let out_t = std::thread::spawn(move || {
        let mut s = Vec::new();
        if let Some(o) = so.as_mut() {
            let _ = o.read_to_end(&mut s);
        }
        s
    });
    let err_t = std::thread::spawn(move || {
        let mut s = Vec::new();
        if let Some(o) = se.as_mut() {
            let _ = o.read_to_end(&mut s);
        }
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                return Err(crate::ft!("{}초 안에 답이 오지 않아 멈췄습니다", "Đã dừng vì không có trả lời trong {} giây", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let out = String::from_utf8_lossy(&out_t.join().unwrap_or_default()).into_owned();
    let err = String::from_utf8_lossy(&err_t.join().unwrap_or_default()).into_owned();
    Ok((status.success(), out, err))
}

fn login_marker(p: Provider) -> Option<bool> {
    let h = dirs::home_dir()?;
    let files: Vec<PathBuf> = match p {
        Provider::ClaudeSub => vec![h.join(".claude").join(".credentials.json")],
        Provider::ChatGptSub => vec![h.join(".codex").join("auth.json")],
        Provider::GeminiSub => vec![h.join(".gemini").join("oauth_creds.json")],
        _ => vec![],
    };
    if files.iter().any(|f| f.exists()) {
        Some(true)
    } else if cfg!(target_os = "macos") && p == Provider::ClaudeSub {
        None // kept in the macOS keychain: can't tell without asking
    } else {
        Some(false)
    }
}

pub fn detect() -> Vec<CliInfo> {
    [Provider::ClaudeSub, Provider::ChatGptSub, Provider::GeminiSub]
        .iter()
        .map(|&p| {
            let path = find_exe(exe_name(p));
            let version = path
                .as_ref()
                .and_then(|x| {
                    let mut c = command(x);
                    c.arg("--version");
                    run_with_input(c, "", Duration::from_secs(15)).ok()
                })
                .map(|(_, o, _)| o.lines().next().unwrap_or("").trim().chars().take(60).collect())
                .unwrap_or_default();
            CliInfo { logged_in: if path.is_some() { login_marker(p) } else { None }, provider: p, path, version }
        })
        .collect()
}

fn safe_model(m: &str) -> Option<String> {
    let m = m.trim();
    if m.is_empty() {
        return None;
    }
    m.chars().all(|c| c.is_ascii_alphanumeric() || "-._:/[]".contains(c)).then(|| m.to_string())
}

fn estimate(s: &str) -> u64 {
    (s.chars().count() as u64 / 3).max(1)
}

fn work_dir() -> PathBuf {
    let d = std::env::temp_dir().join("MyDocDesk-ai");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Turns a tool's error output into a plain-language reason.
fn explain(p: Provider, err: &str, out: &str) -> String {
    let all = format!("{err}\n{out}");
    let low = all.to_lowercase();
    if low.contains("ineligibletier") || low.contains("no longer supported") {
        return crate::i18n::tr("구글이 Gemini CLI의 개인 계정(무료·구독) 로그인을 더 이상 지원하지 않습니다. Gemini는 'Google Gemini API' 키로 연결해 주세요 (AI Studio에서 무료 한도 키 발급).").into();
    }
    if low.contains("not logged in") || low.contains("please log in") || low.contains("login") && low.contains("required") || low.contains("unauthorized") || low.contains("401") {
        return crate::ft!("{}에 로그인되어 있지 않습니다. '로그인' 버튼으로 한 번 로그인해 주세요.", "Chưa đăng nhập {}. Hãy bấm 'Đăng nhập' để đăng nhập một lần.", p.short());
    }
    if low.contains("rate limit") || low.contains("usage limit") || low.contains("429") || low.contains("quota") {
        return crate::ft!("{} 사용 한도에 도달했습니다. 잠시 뒤 다시 하거나 다른 연결을 고르세요.", "Đã chạm hạn mức sử dụng {}. Hãy thử lại sau hoặc chọn kết nối khác.", p.short());
    }
    let tail: String = all.trim().lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" / ");
    crate::ft!("{} 실행 오류: {}", "Lỗi chạy {}: {}", p.short(), tail.chars().take(300).collect::<String>())
}

/// Asks one question through the subscription tool. `image` only works with Codex.
pub fn ask(p: Provider, path: &Path, model: &str, system: &str, user: &str, image: Option<&Path>) -> Result<AiReply, String> {
    let prompt = format!("[지시]\n{system}\n\n{user}");
    let model = safe_model(model);
    let dir = work_dir();
    let timeout = Duration::from_secs(240);
    match p {
        Provider::ClaudeSub => {
            if image.is_some() {
                return Err(crate::i18n::tr("Claude 구독 연결은 그림 해석을 지원하지 않습니다 (API·LM Studio·ChatGPT 구독은 가능)").into());
            }
            let mut c = command(path);
            c.current_dir(&dir).args([
                "-p",
                "--output-format",
                "json",
                "--tools",
                "",
                "--no-session-persistence",
                "--strict-mcp-config",
                "--disable-slash-commands",
                "--setting-sources",
                "",
                "--system-prompt",
                "You are a concise assistant for a personal document app. Follow the [지시] block in the user message.",
            ]);
            if let Some(m) = &model {
                c.args(["--model", m]);
            }
            let (ok, out, err) = run_with_input(c, &prompt, timeout)?;
            let v: serde_json::Value = serde_json::from_str(out.trim()).map_err(|_| explain(p, &err, &out))?;
            if !ok || v["is_error"].as_bool().unwrap_or(false) {
                return Err(explain(p, &err, &v["result"].as_str().unwrap_or(&out).to_string()));
            }
            let text = v["result"].as_str().unwrap_or("").to_string();
            let u = &v["usage"];
            let input = u["input_tokens"].as_u64().unwrap_or(0) + u["cache_creation_input_tokens"].as_u64().unwrap_or(0) + u["cache_read_input_tokens"].as_u64().unwrap_or(0);
            Ok(AiReply {
                prompt_tokens: if input > 0 { input } else { estimate(&prompt) },
                completion_tokens: u["output_tokens"].as_u64().unwrap_or_else(|| estimate(&text)),
                text,
                model: model.unwrap_or_else(|| crate::i18n::tr("Claude 기본 모델").into()),
            })
        }
        Provider::ChatGptSub => {
            let last = dir.join(format!("codex-last-{}.txt", crate::model::new_id()));
            let mut c = command(path);
            c.current_dir(&dir).args(["exec", "--skip-git-repo-check", "--ephemeral", "-s", "read-only", "-c", "project_doc_max_bytes=0", "-o"]).arg(&last);
            if let Some(m) = &model {
                c.args(["-m", m]);
            }
            if let Some(img) = image {
                c.arg("-i").arg(img);
            }
            let (ok, out, err) = run_with_input(c, &prompt, timeout)?;
            let text = std::fs::read_to_string(&last).unwrap_or_default();
            let _ = std::fs::remove_file(&last);
            if !ok || text.trim().is_empty() {
                return Err(explain(p, &err, &out));
            }
            // "tokens used\n30,265"
            let all = format!("{out}\n{err}");
            let used = all
                .lines()
                .skip_while(|l| !l.contains("tokens used"))
                .nth(1)
                .and_then(|l| l.trim().replace(',', "").parse::<u64>().ok());
            let completion = estimate(&text);
            Ok(AiReply {
                prompt_tokens: used.map(|u| u.saturating_sub(completion)).unwrap_or_else(|| estimate(&prompt)),
                completion_tokens: completion,
                text: text.trim().to_string(),
                model: model.unwrap_or_else(|| crate::i18n::tr("ChatGPT 기본 모델").into()),
            })
        }
        Provider::GeminiSub => {
            if image.is_some() {
                return Err(crate::i18n::tr("Gemini 구독 연결은 그림 해석을 지원하지 않습니다").into());
            }
            let mut c = command(path);
            c.current_dir(&dir).args(["-p", "위 입력의 [지시]를 따르세요.", "-o", "json", "--approval-mode", "plan", "--skip-trust"]);
            if let Some(m) = &model {
                c.args(["-m", m]);
            }
            let (ok, out, err) = run_with_input(c, &prompt, timeout)?;
            let start = out.find('{');
            let v: Option<serde_json::Value> = start.and_then(|i| serde_json::from_str(&out[i..]).ok());
            let text = v.as_ref().and_then(|v| v["response"].as_str()).unwrap_or("").to_string();
            if !ok || text.trim().is_empty() {
                return Err(explain(p, &err, &out));
            }
            Ok(AiReply { prompt_tokens: estimate(&prompt), completion_tokens: estimate(&text), text, model: model.unwrap_or_else(|| crate::i18n::tr("Gemini 기본 모델").into()) })
        }
        _ => Err(crate::i18n::tr("구독 연결이 아닙니다").into()),
    }
}

/// Opens a terminal window running the tool's own login, so the person signs in themselves.
pub fn open_login(p: Provider, path: &Path) -> Result<(), String> {
    let arg = match p {
        Provider::ChatGptSub => "login",
        _ => "",
    };
    #[cfg(windows)]
    {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", "cmd", "/K"]).arg(path);
        if !arg.is_empty() {
            c.arg(arg);
        }
        c.spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let script = std::env::temp_dir().join(format!("mydocdesk-login-{}.command", exe_name(p)));
        std::fs::write(&script, format!("#!/bin/sh\n\"{}\" {}\n", path.display(), arg)).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
        }
        open::that(&script).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// `MDD_LIVE=1 cargo test live_ -- --ignored --nocapture` (asks the real tools; uses a little subscription quota)
    #[test]
    #[ignore]
    fn live_detect_and_ask() {
        if std::env::var("MDD_LIVE").is_err() {
            return;
        }
        for l in crate::local::detect() {
            println!("local: {} running={} installed={} models={:?}", l.name, l.running, l.installed, l.models.iter().map(|m| (&m.id, m.vision)).collect::<Vec<_>>());
        }
        for c in detect() {
            println!("cli: {:?} path={:?} version={} logged_in={:?}", c.provider, c.path.as_ref().map(|p| p.file_name()), c.version, c.logged_in);
        }
        for (p, m) in [(Provider::ClaudeSub, "haiku"), (Provider::ChatGptSub, "")] {
            let path = find_exe(exe_name(p)).unwrap();
            let t = Instant::now();
            match ask(p, &path, m, "짧게 답하세요.", "'연결됨'이라고만 답하세요.", None) {
                Ok(r) => println!("{:?}: {:?} in={} out={} model={} ({:.1}s)", p, r.text, r.prompt_tokens, r.completion_tokens, r.model, t.elapsed().as_secs_f32()),
                Err(e) => println!("{:?}: ERROR {e}", p),
            }
        }
        let path = find_exe("gemini").unwrap();
        println!("gemini: {:?}", ask(Provider::GeminiSub, &path, "", "짧게", "'연결됨'", None).err());
    }
}

#[cfg(test)]
mod live_local {
    #[test]
    #[ignore]
    fn live_lmstudio() {
        if std::env::var("MDD_LIVE").is_err() {
            return;
        }
        let mut s = crate::model::Settings { provider: crate::model::Provider::LmStudio, local_only: true, ..Default::default() };
        println!("models: {:?}", crate::ai::list_models(&s, s.provider));
        s.model = "qwen3.5-4b".into();
        let t = std::time::Instant::now();
        match crate::ai::ask(&s, "짧게 한국어로 답하세요.", "'연결됨'이라고만 답하세요.") {
            Ok(r) => println!("lmstudio: {:?} ({:.1}s)", r.text.chars().take(80).collect::<String>(), t.elapsed().as_secs_f32()),
            Err(e) => println!("lmstudio: ERROR {e}"),
        }
    }
}
