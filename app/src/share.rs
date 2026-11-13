//! The file share page: either written out as a folder (to put in a cloud folder or USB),
//! or served live to other devices on the same network with a secret link.

use crate::model::now_ms;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
pub struct ShareItem {
    pub name: String,
    pub kind: String,
    pub bytes: Arc<Vec<u8>>,
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn pct(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{:02X}", b));
        }
    }
    o
}

const CSS: &str = "body{margin:0;background:#030d17;color:#f4f5f7;font-family:'Pretendard','Malgun Gothic','Apple SD Gothic Neo',sans-serif}
.wrap{max-width:760px;margin:0 auto;padding:48px 20px}
.badge{display:inline-block;background:#2e7dff22;color:#6ea4ff;border-radius:100px;padding:4px 12px;font-size:12px}
h1{font-size:30px;margin:14px 0 6px}p.msg{color:#c1c7d0;line-height:1.6;white-space:pre-wrap}
.meta{color:#97a0af;font-size:13px;margin-bottom:28px}
.item{display:flex;align-items:center;gap:14px;background:#19232c;border-radius:10px;padding:16px 18px;margin:10px 0}
.icon{width:40px;height:40px;border-radius:8px;background:linear-gradient(135deg,#2e7dff,#7b5cff);display:flex;align-items:center;justify-content:center;font-size:12px;font-weight:700}
.name{flex:1}.name b{display:block;font-weight:600}.name span{color:#97a0af;font-size:13px}
a.btn{background:#2e7dff;color:#fff;text-decoration:none;border-radius:8px;padding:9px 16px;font-size:14px}
input{background:#19232c;border:1px solid #2b3946;color:#fff;border-radius:8px;padding:10px 12px;font-size:15px}
button{background:#2e7dff;color:#fff;border:0;border-radius:8px;padding:10px 18px;font-size:15px;cursor:pointer}
.foot{color:#757f88;font-size:12px;margin-top:40px}";

fn size_text(n: usize) -> String {
    crate::model::fmt_size(n as u64)
}

fn ext_of(name: &str) -> String {
    Path::new(name).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_else(|| "FILE".into())
}

pub fn page_html(title: &str, message: &str, items: &[ShareItem], link: impl Fn(usize) -> String, expires: Option<i64>) -> String {
    let mut rows = String::new();
    for (i, it) in items.iter().enumerate() {
        rows.push_str(&format!(
            "<div class=item><div class=icon>{}</div><div class=name><b>{}</b><span>{} · {}</span></div><a class=btn href=\"{}\" download>내려받기</a></div>",
            esc(&ext_of(&it.name).chars().take(4).collect::<String>()),
            esc(&it.name),
            esc(&it.kind),
            size_text(it.bytes.len()),
            link(i)
        ));
    }
    let exp = expires.map(|e| format!(" · {}까지 열람 가능", crate::model::fmt_time(e))).unwrap_or_default();
    format!(
        "<!doctype html><html lang=ko><head><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>{t}</title><style>{CSS}</style></head><body><div class=wrap><span class=badge>공유 문서</span><h1>{t}</h1><p class=msg>{m}</p><div class=meta>파일 {n}개{exp}</div>{rows}<div class=foot>MyDocDesk로 만든 공유 페이지입니다.</div></div></body></html>",
        t = esc(title),
        m = esc(message),
        n = items.len()
    )
}

fn pin_html(title: &str, wrong: bool) -> String {
    format!(
        "<!doctype html><html lang=ko><head><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>{t}</title><style>{CSS}</style></head><body><div class=wrap><span class=badge>공유 문서</span><h1>{t}</h1><p class=msg>보낸 사람이 알려 준 PIN을 입력해 주세요.</p>{w}<form method=get><input name=pin type=password autofocus placeholder=PIN> <button>열기</button></form></div></body></html>",
        t = esc(title),
        w = if wrong { "<p style='color:#ff6b6b'>PIN이 맞지 않습니다.</p>" } else { "" }
    )
}

fn simple_html(title: &str, msg: &str) -> String {
    format!("<!doctype html><html lang=ko><head><meta charset=utf-8><title>{t}</title><style>{CSS}</style></head><body><div class=wrap><h1>{t}</h1><p class=msg>{m}</p></div></body></html>", t = esc(title), m = esc(msg))
}

/// Writes index.html and the files into a new folder; returns the folder.
pub fn export_folder(parent: &Path, title: &str, message: &str, items: &[ShareItem]) -> Result<PathBuf, String> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M").to_string();
    let dir = parent.join(format!("공유-{}", stamp));
    std::fs::create_dir_all(dir.join("files")).map_err(|e| format!("폴더를 만들 수 없습니다: {e}"))?;
    let mut names = Vec::new();
    for it in items {
        let mut name = it.name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        let mut n = 1;
        while names.contains(&name) {
            name = format!("{}-{}", n, it.name);
            n += 1;
        }
        std::fs::write(dir.join("files").join(&name), it.bytes.as_slice()).map_err(|e| e.to_string())?;
        names.push(name);
    }
    let html = page_html(title, message, items, |i| format!("files/{}", pct(&names[i])), None);
    std::fs::write(dir.join("index.html"), html).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn lan_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.168.0.1:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

pub struct LiveShare {
    pub url: String,
    pub title: String,
    pub pin: String,
    pub expires_ms: i64,
    pub files: usize,
    stop: Arc<AtomicBool>,
    pub visits: Arc<AtomicU32>,
    pub downloads: Arc<AtomicU32>,
    pub log: Arc<Mutex<Vec<String>>>,
}

impl LiveShare {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
    pub fn expired(&self) -> bool {
        now_ms() > self.expires_ms
    }
}

impl Drop for LiveShare {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start_live(title: String, message: String, items: Vec<ShareItem>, pin: String, hours: u32) -> Result<LiveShare, String> {
    let server = tiny_http::Server::http("0.0.0.0:0").map_err(|e| format!("공유를 시작하지 못했습니다: {e}"))?;
    let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
    let mut tok = [0u8; 9];
    let _ = getrandom::getrandom(&mut tok);
    let token: String = tok.iter().map(|b| format!("{:02x}", b)).collect();
    let url = format!("http://{}:{}/s/{}/", lan_ip(), port, token);
    let expires_ms = now_ms() + hours.max(1) as i64 * 3_600_000;
    let stop = Arc::new(AtomicBool::new(false));
    let visits = Arc::new(AtomicU32::new(0));
    let downloads = Arc::new(AtomicU32::new(0));
    let log = Arc::new(Mutex::new(Vec::new()));
    let live = LiveShare {
        url,
        title: title.clone(),
        pin: pin.clone(),
        expires_ms,
        files: items.len(),
        stop: stop.clone(),
        visits: visits.clone(),
        downloads: downloads.clone(),
        log: log.clone(),
    };
    std::thread::spawn(move || {
        let prefix = format!("/s/{}/", token);
        while !stop.load(Ordering::SeqCst) {
            let req = match server.recv_timeout(Duration::from_millis(250)) {
                Ok(Some(r)) => r,
                Ok(None) => continue,
                Err(_) => break,
            };
            let who = req.remote_addr().map(|a| a.ip().to_string()).unwrap_or_default();
            let full = req.url().to_string();
            let (path, query) = full.split_once('?').unwrap_or((full.as_str(), ""));
            let given_pin = query
                .split('&')
                .find_map(|kv| kv.strip_prefix("pin="))
                .unwrap_or("")
                .to_string();
            let html = |s: String, code: u16| {
                tiny_http::Response::from_data(s.into_bytes())
                    .with_status_code(code)
                    .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap())
            };
            if !path.starts_with(&prefix) {
                let _ = req.respond(html(simple_html("찾을 수 없음", "링크가 올바르지 않습니다."), 404));
                continue;
            }
            if now_ms() > expires_ms {
                let _ = req.respond(html(simple_html("공유가 끝났습니다", "열람 기간이 지났습니다. 보낸 사람에게 다시 요청해 주세요."), 410));
                continue;
            }
            if !pin.is_empty() && given_pin != pin {
                let _ = req.respond(html(pin_html(&title, !given_pin.is_empty()), 200));
                continue;
            }
            let rest = &path[prefix.len()..];
            let pin_q = if pin.is_empty() { String::new() } else { format!("?pin={}", pct(&pin)) };
            if rest.is_empty() {
                visits.fetch_add(1, Ordering::SeqCst);
                if let Ok(mut l) = log.lock() {
                    l.push(format!("{}  {} 페이지 열람", chrono::Local::now().format("%H:%M:%S"), who));
                }
                let page = page_html(&title, &message, &items, |i| format!("{}f/{}{}", prefix, i, pin_q), Some(expires_ms));
                let _ = req.respond(html(page, 200));
            } else if let Some(n) = rest.strip_prefix("f/").and_then(|n| n.parse::<usize>().ok()) {
                if let Some(it) = items.get(n) {
                    downloads.fetch_add(1, Ordering::SeqCst);
                    if let Ok(mut l) = log.lock() {
                        l.push(format!("{}  {} '{}' 내려받기", chrono::Local::now().format("%H:%M:%S"), who, it.name));
                    }
                    let disp = format!("attachment; filename=\"download\"; filename*=UTF-8''{}", pct(&it.name));
                    let resp = tiny_http::Response::from_data(it.bytes.as_slice().to_vec())
                        .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/octet-stream"[..]).unwrap())
                        .with_header(tiny_http::Header::from_bytes(&b"Content-Disposition"[..], disp.as_bytes()).unwrap());
                    let _ = req.respond(resp);
                } else {
                    let _ = req.respond(html(simple_html("찾을 수 없음", "파일이 없습니다."), 404));
                }
            } else {
                let _ = req.respond(html(simple_html("찾을 수 없음", "링크가 올바르지 않습니다."), 404));
            }
        }
    });
    Ok(live)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn live_share_serves_page_pin_and_file() {
        let items = vec![ShareItem { name: "메모.txt".into(), kind: "텍스트".into(), bytes: Arc::new(b"hello".to_vec()) }];
        let live = start_live("테스트".into(), "안녕".into(), items, "1234".into(), 1).unwrap();
        let url = live.url.replace(&lan_ip(), "127.0.0.1");
        let page = ureq::get(&url).call().unwrap().into_string().unwrap();
        assert!(page.contains("PIN"));
        let page = ureq::get(&format!("{url}?pin=1234")).call().unwrap().into_string().unwrap();
        assert!(page.contains("메모.txt"));
        let mut body = Vec::new();
        ureq::get(&format!("{url}f/0?pin=1234")).call().unwrap().into_reader().read_to_end(&mut body).unwrap();
        assert_eq!(body, b"hello");
        assert!(ureq::get(&url.replace("/s/", "/x/")).call().is_err());
        assert_eq!(live.downloads.load(Ordering::SeqCst), 1);
        live.stop();
    }

    #[test]
    fn export_writes_page_and_files() {
        let dir = std::env::temp_dir().join(format!("mdd-share-{}", crate::model::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let items = vec![ShareItem { name: "a b.txt".into(), kind: "텍스트".into(), bytes: Arc::new(b"x".to_vec()) }];
        let out = export_folder(&dir, "제목", "", &items).unwrap();
        let html = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(html.contains("files/a%20b.txt"));
        assert!(out.join("files").join("a b.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
