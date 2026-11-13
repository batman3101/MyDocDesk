//! Finding AI that runs on this PC: Ollama, LM Studio, Jan, GPT4All, llama.cpp/LocalAI servers.

use crate::cli::{command, find_exe, run_with_input};
use crate::model::Provider;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct LocalModel {
    pub id: String,
    pub vision: bool,
}

#[derive(Clone, Debug)]
pub struct LocalAi {
    pub name: &'static str,
    pub provider: Provider,
    pub base_url: String,
    pub running: bool,
    /// Installed but its server is off (we can start it).
    pub installed: bool,
    pub models: Vec<LocalModel>,
    pub note: String,
}

fn get_json(url: &str) -> Option<Value> {
    ureq::AgentBuilder::new().timeout(Duration::from_millis(900)).build().get(url).call().ok()?.into_json().ok()
}

fn openai_models(base: &str) -> Option<Vec<LocalModel>> {
    let v = get_json(&format!("{base}/models"))?;
    Some(
        v["data"]
            .as_array()?
            .iter()
            .filter_map(|m| m["id"].as_str())
            .filter(|id| !id.contains("embed"))
            .map(|id| LocalModel { id: id.to_string(), vision: id.contains("vl") || id.contains("vision") || id.contains("llava") })
            .collect(),
    )
}

fn ollama_exe() -> Option<PathBuf> {
    find_exe("ollama").or_else(|| {
        let mac = PathBuf::from("/Applications/Ollama.app/Contents/Resources/ollama");
        mac.exists().then_some(mac)
    })
}

fn lms_exe() -> Option<PathBuf> {
    find_exe("lms")
}

/// LM Studio's models on disk, even while its server is off (`lms ls --json`).
fn lms_models() -> Vec<LocalModel> {
    let Some(p) = lms_exe() else { return vec![] };
    let mut c = command(&p);
    c.args(["ls", "--json"]);
    let Ok((_, out, _)) = run_with_input(c, "", Duration::from_secs(20)) else { return vec![] };
    let Ok(v) = serde_json::from_str::<Value>(out.trim()) else { return vec![] };
    v.as_array()
        .map(|a| {
            a.iter()
                .filter(|m| m["type"].as_str() != Some("embedding"))
                .filter_map(|m| Some(LocalModel { id: m["modelKey"].as_str()?.to_string(), vision: m["vision"].as_bool().unwrap_or(false) }))
                .collect()
        })
        .unwrap_or_default()
}

pub fn detect() -> Vec<LocalAi> {
    let mut out = Vec::new();

    // Ollama
    let base = "http://127.0.0.1:11434";
    let tags = get_json(&format!("{base}/api/tags"));
    let models: Vec<LocalModel> = tags
        .as_ref()
        .and_then(|v| v["models"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|m| m["name"].as_str())
        .map(|n| LocalModel { id: n.to_string(), vision: n.contains("llava") || n.contains("vl") || n.contains("vision") })
        .collect();
    let installed = ollama_exe().is_some();
    if tags.is_some() || installed {
        out.push(LocalAi {
            name: "Ollama",
            provider: Provider::Ollama,
            base_url: format!("{base}/v1"),
            running: tags.is_some(),
            installed,
            note: if tags.is_some() && models.is_empty() { crate::i18n::tr("받은 모델이 없습니다 — 'ollama pull llama3.1'").into() } else { String::new() },
            models,
        });
    }

    // LM Studio
    let base = "http://127.0.0.1:1234/v1";
    let live = openai_models(base);
    let installed = lms_exe().is_some();
    if live.is_some() || installed {
        let mut models = live.clone().unwrap_or_default();
        let disk = lms_models();
        if models.is_empty() {
            models = disk.clone();
        } else {
            for m in models.iter_mut() {
                if let Some(d) = disk.iter().find(|d| d.id == m.id) {
                    m.vision = d.vision;
                }
            }
        }
        out.push(LocalAi {
            name: "LM Studio",
            provider: Provider::LmStudio,
            base_url: base.into(),
            running: live.is_some(),
            installed,
            note: if live.is_none() { crate::i18n::tr("서버가 꺼져 있습니다 — '서버 켜기'를 누르면 켜고 고른 모델을 자동으로 불러옵니다").into() } else { String::new() },
            models,
        });
    }

    // other OpenAI-compatible local servers
    for (name, base) in [("Jan", "http://127.0.0.1:1337/v1"), ("GPT4All", "http://127.0.0.1:4891/v1"), ("llama.cpp / LocalAI", "http://127.0.0.1:8080/v1")] {
        if let Some(models) = openai_models(base) {
            out.push(LocalAi { name, provider: Provider::LocalOther, base_url: base.into(), running: true, installed: true, note: String::new(), models });
        }
    }
    out
}

/// Starts a local AI server that is installed but off.
pub fn start(p: Provider) -> Result<(), String> {
    match p {
        Provider::Ollama => {
            let exe = ollama_exe().ok_or(crate::i18n::tr("Ollama를 찾지 못했습니다"))?;
            command(&exe).arg("serve").spawn().map(|_| ()).map_err(|e| e.to_string())
        }
        Provider::LmStudio => {
            let exe = lms_exe().ok_or(crate::i18n::tr("LM Studio(lms)를 찾지 못했습니다"))?;
            let mut c = command(&exe);
            c.args(["server", "start"]);
            run_with_input(c, "", Duration::from_secs(60)).map(|_| ())
        }
        _ => Err(crate::i18n::tr("켤 수 없는 서버입니다").into()),
    }
}
