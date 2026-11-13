//! Asking a language model by any of three routes:
//! local AI on this PC (Ollama, LM Studio...), an API key (OpenAI, Anthropic, Gemini),
//! or a subscription through the company's official command-line tool (see cli.rs).

use crate::model::{Provider, Route, Settings};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

pub struct AiReply {
    pub text: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub model: String,
}

fn estimate_tokens(s: &str) -> u64 {
    (s.chars().count() as u64 / 3).max(1)
}

fn base_for(settings: &Settings, p: Provider) -> String {
    let b = if settings.provider == p && !settings.base_url.trim().is_empty() {
        settings.base_url.trim().to_string()
    } else {
        p.default_url().to_string()
    };
    b.trim_end_matches('/').to_string()
}

fn base(settings: &Settings) -> String {
    base_for(settings, settings.provider)
}

pub fn model_name(settings: &Settings) -> String {
    if settings.model.trim().is_empty() {
        settings.provider.default_model().to_string()
    } else {
        settings.model.trim().to_string()
    }
}

fn http_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, r) => {
            let body = r.into_string().unwrap_or_default();
            let short: String = body.chars().take(300).collect();
            match code {
                401 | 403 => crate::ft!("API 키가 맞지 않거나 권한이 없습니다 ({code}). 설정을 확인해 주세요. {short}", "Khóa API không đúng hoặc không có quyền ({code}). Hãy kiểm tra cài đặt. {short}", code = code, short = short),
                404 => crate::ft!("주소나 모델 이름을 찾지 못했습니다 (404). {short}", "Không tìm thấy địa chỉ hoặc tên mô hình (404). {short}", short = short),
                429 => crate::ft!("요청이 너무 많거나 한도를 넘었습니다 (429). {short}", "Quá nhiều yêu cầu hoặc vượt hạn mức (429). {short}", short = short),
                _ => crate::ft!("AI 서버 오류 {code}: {short}", "Lỗi máy chủ AI {code}: {short}", code = code, short = short),
            }
        }
        ureq::Error::Transport(t) => crate::ft!("AI 서버에 연결하지 못했습니다: {t}. (내 PC의 AI라면 프로그램·서버가 켜져 있는지 확인해 주세요)", "Không kết nối được máy chủ AI: {t}. (Nếu là AI trên máy, hãy kiểm tra chương trình · máy chủ đã bật chưa)", t = t),
    }
}

fn blocked(settings: &Settings) -> Option<String> {
    blocked_for(settings, settings.provider)
}

/// "This PC only" stops every request to a cloud service (subscriptions and APIs), whatever it is for.
fn blocked_for(settings: &Settings, p: Provider) -> Option<String> {
    (settings.local_only && p.is_cloud())
        .then(|| crate::i18n::tr("'내 PC 전용 모드'가 켜져 있어 클라우드 AI(구독·API)로 보내지 않았습니다. '계정·동기화'에서 끄거나 내 PC의 AI를 고르세요.").into())
}

/// Where the subscription tool lives (remembered from detection, else searched again).
pub fn cli_path(settings: &Settings, p: Provider) -> Option<PathBuf> {
    settings
        .cli_paths
        .get(p.id())
        .map(PathBuf::from)
        .filter(|x| x.is_file())
        .or_else(|| crate::cli::find_exe(crate::cli::exe_name(p)))
}

fn openai_chat(settings: &Settings, model: &str, messages: Value, timeout: u64) -> Result<Value, String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(timeout)).build();
    let mut req = agent.post(&format!("{}/chat/completions", base(settings))).set("Content-Type", "application/json");
    let key = settings.key_for(settings.provider);
    if !key.trim().is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", key.trim()));
    }
    req.send_json(json!({ "model": model, "messages": messages, "temperature": 0.2 })).map_err(http_err)?.into_json().map_err(|e| e.to_string())
}

pub fn ask(settings: &Settings, system: &str, user: &str) -> Result<AiReply, String> {
    if let Some(e) = blocked(settings) {
        return Err(e);
    }
    let model = model_name(settings);
    match settings.provider {
        Provider::None => Err(crate::i18n::tr("AI가 연결되지 않았습니다").into()),
        Provider::Ollama | Provider::LmStudio | Provider::LocalOther | Provider::OpenAi | Provider::Gemini => {
            if settings.provider.needs_key() && settings.key_for(settings.provider).trim().is_empty() {
                return Err(crate::ft!("{} 키가 없습니다. 설정 > AI 연결에서 키를 넣어 주세요.", "Chưa có khóa {}. Hãy nhập khóa trong Cài đặt > Kết nối AI.", settings.provider.short()));
            }
            if model.is_empty() {
                return Err(crate::i18n::tr("모델을 골라 주세요 (설정 > AI 연결)").into());
            }
            let v = openai_chat(settings, &model, json!([{"role": "system", "content": system}, {"role": "user", "content": user}]), 180)?;
            let text = v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
            if text.is_empty() {
                return Err(crate::ft!("AI가 빈 답을 보냈습니다: {}", "AI trả về câu trả lời rỗng: {}", v.to_string().chars().take(200).collect::<String>()));
            }
            Ok(AiReply {
                prompt_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or_else(|| estimate_tokens(system) + estimate_tokens(user)),
                completion_tokens: v["usage"]["completion_tokens"].as_u64().unwrap_or_else(|| estimate_tokens(&text)),
                text,
                model,
            })
        }
        Provider::Anthropic => {
            let key = settings.key_for(Provider::Anthropic);
            if key.trim().is_empty() {
                return Err(crate::i18n::tr("Anthropic API 키가 없습니다. 설정 > AI 연결에서 키를 넣어 주세요.").into());
            }
            let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(180)).build();
            let v: Value = agent
                .post(&format!("{}/messages", base(settings)))
                .set("Content-Type", "application/json")
                .set("x-api-key", key.trim())
                .set("anthropic-version", "2023-06-01")
                .send_json(json!({ "model": model, "max_tokens": 1500, "system": system, "messages": [{"role": "user", "content": user}] }))
                .map_err(http_err)?
                .into_json()
                .map_err(|e| e.to_string())?;
            let text: String = v["content"].as_array().map(|a| a.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("")).unwrap_or_default();
            if text.is_empty() {
                return Err(crate::ft!("AI가 빈 답을 보냈습니다: {}", "AI trả về câu trả lời rỗng: {}", v.to_string().chars().take(200).collect::<String>()));
            }
            Ok(AiReply {
                prompt_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or_else(|| estimate_tokens(user)),
                completion_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or_else(|| estimate_tokens(&text)),
                text,
                model,
            })
        }
        p @ (Provider::ClaudeSub | Provider::ChatGptSub | Provider::GeminiSub) => {
            let path = cli_path(settings, p).ok_or_else(|| crate::ft!("{}에 필요한 공식 프로그램({})을 찾지 못했습니다. 설치 방법을 확인해 주세요.", "Không tìm thấy chương trình chính thức ({1}) cần cho {0}. Hãy xem hướng dẫn cài đặt.", p.short(), crate::cli::exe_name(p)))?;
            crate::cli::ask(p, &path, &settings.model, system, user, None)
        }
    }
}

/// Subscriptions are already paid for; local AI is free.
pub fn cost(settings: &Settings, p: u64, c: u64) -> f64 {
    if settings.provider.route() != Route::Api {
        return 0.0;
    }
    p as f64 / 1_000_000.0 * settings.price_in_per_m + c as f64 / 1_000_000.0 * settings.price_out_per_m
}

/// The models a service offers right now. Local AI and APIs are asked live; subscriptions
/// can't be listed, so their usual names are offered (any other name can be typed in).
pub fn list_models(settings: &Settings, p: Provider) -> Result<Vec<String>, String> {
    if let Some(e) = blocked_for(settings, p) {
        return Err(e);
    }
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build();
    let key = settings.key_for(p);
    let mut out: Vec<String> = match p.route() {
        Route::Off => vec![],
        Route::Subscription => p.preset_models().iter().map(|s| s.to_string()).collect(),
        _ if p.needs_key() && key.trim().is_empty() => return Err(crate::ft!("{} 키를 먼저 넣어 주세요", "Hãy nhập khóa {} trước", p.short())),
        _ if p == Provider::Anthropic => {
            let v: Value = agent
                .get(&format!("{}/models?limit=100", base_for(settings, p)))
                .set("x-api-key", key.trim())
                .set("anthropic-version", "2023-06-01")
                .call()
                .map_err(http_err)?
                .into_json()
                .map_err(|e| e.to_string())?;
            v["data"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect()).unwrap_or_default()
        }
        _ => {
            let mut req = agent.get(&format!("{}/models", base_for(settings, p)));
            if !key.trim().is_empty() {
                req = req.set("Authorization", &format!("Bearer {}", key.trim()));
            }
            let v: Value = req.call().map_err(http_err)?.into_json().map_err(|e| e.to_string())?;
            v["data"]
                .as_array()
                .map(|a| a.iter().filter_map(|m| m["id"].as_str()).map(|id| id.trim_start_matches("models/").to_string()).collect())
                .unwrap_or_default()
        }
    };
    // chat models first; drop embeddings, speech and image-generation models from API lists
    out.retain(|m| !m.to_lowercase().contains("embed"));
    if p.route() == Route::Api {
        out.retain(|m| {
            let l = m.to_lowercase();
            !["embed", "tts", "whisper", "dall-e", "moderation", "image", "audio", "realtime", "transcribe", "aqa", "imagen", "veo"].iter().any(|x| l.contains(x))
        });
    }
    out.sort();
    out.dedup();
    Ok(out)
}

pub fn vision_model(settings: &Settings) -> String {
    if !settings.vision_model.trim().is_empty() {
        return settings.vision_model.trim().to_string();
    }
    match settings.provider {
        Provider::Ollama if settings.model.trim().is_empty() => "llava".into(),
        _ => model_name(settings),
    }
}

pub const PICTURE_PROMPT: &str = "이 그림은 문서 안에 들어 있던 것입니다. 한국어로 설명하세요. 차트나 표라면 제목, 축, 항목과 수치를 빠짐없이 표 형식(항목 | 값)으로 옮기고, 글자가 있으면 그대로 받아 적으세요. 사진이면 무엇이 보이는지 2~3문장으로 쓰세요.";

/// Describes one picture with a vision-capable model.
pub fn describe_image(settings: &Settings, mime: &str, bytes: &[u8]) -> Result<AiReply, String> {
    use base64::Engine;
    if let Some(e) = blocked(settings) {
        return Err(e);
    }
    let model = vision_model(settings);
    let p = settings.provider;
    if p == Provider::ChatGptSub {
        // Codex can attach a picture file
        let ext = mime.rsplit('/').next().unwrap_or("png");
        let file = std::env::temp_dir().join(format!("mydocdesk-img-{}.{ext}", crate::model::new_id()));
        std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
        let path = cli_path(settings, p).ok_or(crate::i18n::tr("Codex 프로그램을 찾지 못했습니다"))?;
        let r = crate::cli::ask(p, &path, &settings.vision_model, crate::i18n::tr("그림을 보고 답하세요."), PICTURE_PROMPT, Some(&file));
        let _ = std::fs::remove_file(&file);
        return r;
    }
    if p.route() == Route::Subscription {
        return Err(crate::ft!("{} 연결은 그림 해석을 지원하지 않습니다 (LM Studio·Ollama의 그림 모델, API, ChatGPT 구독은 가능)", "Kết nối {} không hỗ trợ đọc hình (mô hình đọc hình của LM Studio · Ollama, API và thuê bao ChatGPT thì được)", p.short()));
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let v: Value = match p {
        Provider::None => return Err(crate::i18n::tr("AI가 연결되지 않았습니다").into()),
        Provider::Anthropic => ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(240))
            .build()
            .post(&format!("{}/messages", base(settings)))
            .set("Content-Type", "application/json")
            .set("x-api-key", settings.key_for(p).trim())
            .set("anthropic-version", "2023-06-01")
            .send_json(json!({
                "model": model,
                "max_tokens": 1500,
                "messages": [{"role": "user", "content": [
                    {"type": "image", "source": {"type": "base64", "media_type": mime, "data": b64}},
                    {"type": "text", "text": PICTURE_PROMPT}
                ]}]
            }))
            .map_err(http_err)?
            .into_json()
            .map_err(|e| e.to_string())?,
        _ => openai_chat(
            settings,
            &model,
            json!([{"role": "user", "content": [
                {"type": "text", "text": PICTURE_PROMPT},
                {"type": "image_url", "image_url": {"url": format!("data:{mime};base64,{b64}")}}
            ]}]),
            240,
        )?,
    };
    let text = if p == Provider::Anthropic {
        v["content"].as_array().map(|a| a.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("")).unwrap_or_default()
    } else {
        v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string()
    };
    if text.trim().is_empty() {
        return Err(crate::i18n::tr("AI가 그림 설명을 비워 보냈습니다 (그림을 볼 수 있는 모델인지 확인해 주세요)").into());
    }
    let (pt, ct) = if p == Provider::Anthropic {
        (v["usage"]["input_tokens"].as_u64(), v["usage"]["output_tokens"].as_u64())
    } else {
        (v["usage"]["prompt_tokens"].as_u64(), v["usage"]["completion_tokens"].as_u64())
    };
    Ok(AiReply { prompt_tokens: pt.unwrap_or(800), completion_tokens: ct.unwrap_or_else(|| estimate_tokens(&text)), text, model })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// audit M-07: with "this PC only" on, not even the model list is asked from a cloud service.
    #[test]
    fn local_only_sends_no_model_list_request() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let s = Settings { provider: Provider::OpenAi, base_url: format!("http://{}", server.server_addr()), api_key: "test-dummy".into(), ..Default::default() };
        assert!(s.local_only);
        let r = list_models(&s, Provider::OpenAi);
        assert!(r.is_err(), "refused: {r:?}");
        assert!(server.recv_timeout(Duration::from_millis(300)).unwrap().is_none(), "no request reached the service");
        // local AI is still listed (it doesn't leave this PC)
        let local = Settings { provider: Provider::Ollama, base_url: format!("http://{}", server.server_addr()), ..Default::default() };
        let h = std::thread::spawn(move || {
            let req = server.recv_timeout(Duration::from_secs(5)).unwrap().expect("local request");
            let _ = req.respond(tiny_http::Response::from_string(r#"{"data":[{"id":"llama"}]}"#).with_header(tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap()));
        });
        assert_eq!(list_models(&local, Provider::Ollama).unwrap(), vec!["llama"]);
        h.join().unwrap();
    }
}
