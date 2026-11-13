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
        "<!doctype html><html lang=ko><head><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>{t}</title><style>{CSS}</style></head><body><div class=wrap><span class=badge>공유 문서</span><h1>{t}</h1><p class=msg>보낸 사람이 알려 준 PIN을 입력해 주세요.</p>{w}<form method=post><input name=pin type=password autofocus placeholder=PIN autocomplete=off> <button>열기</button></form></div></body></html>",
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
    std::fs::create_dir_all(parent).map_err(|e| format!("폴더를 만들 수 없습니다: {e}"))?;
    // always a folder that didn't exist, so nothing from an earlier export comes along
    let base = format!("공유-{}", stamp);
    let mut dir = parent.join(&base);
    let mut n = 2;
    loop {
        match std::fs::create_dir(&dir) {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 1000 => {
                dir = parent.join(format!("{base}-{n}"));
                n += 1;
            }
            Err(e) => return Err(format!("폴더를 만들 수 없습니다: {e}")),
        }
    }
    std::fs::create_dir(dir.join("files")).map_err(|e| format!("폴더를 만들 수 없습니다: {e}"))?;
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

const COOKIE: &str = "mdd_s";
const MAX_PIN_TRIES: u32 = 5;
const PIN_BLOCK_SECS: u64 = 300;

/// `application/x-www-form-urlencoded` value: '+' is a space, %XX a byte.
fn form_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(v) => {
                    out.push(v);
                    i += 2;
                }
                None => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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
        // browsers that typed the right PIN (session cookie), and wrong tries per device
        let mut sessions: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut tries: std::collections::HashMap<String, (u32, Option<std::time::Instant>)> = std::collections::HashMap::new();
        while !stop.load(Ordering::SeqCst) {
            let mut req = match server.recv_timeout(Duration::from_millis(250)) {
                Ok(Some(r)) => r,
                Ok(None) => continue,
                Err(_) => break,
            };
            let who = req.remote_addr().map(|a| a.ip().to_string()).unwrap_or_default();
            let full = req.url().to_string();
            // the query is ignored: a PIN never travels in the address
            let path = full.split_once('?').map(|(p, _)| p).unwrap_or(full.as_str()).to_string();
            let path = path.as_str();
            let cookie_sid = req
                .headers()
                .iter()
                .filter(|h| h.field.equiv("Cookie"))
                .flat_map(|h| h.value.as_str().split(';').map(|kv| kv.trim().to_string()).collect::<Vec<_>>())
                .find_map(|kv| kv.strip_prefix(&format!("{COOKIE}=")).map(String::from));
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
            if !pin.is_empty() {
                let signed_in = cookie_sid.as_ref().is_some_and(|s| sessions.contains(s));
                if *req.method() == tiny_http::Method::Post {
                    // the PIN form; guessing is limited per device (IP)
                    let now = std::time::Instant::now();
                    let entry = tries.entry(who.clone()).or_insert((0, None));
                    if entry.1.is_some_and(|until| until > now) {
                        let _ = req.respond(html(simple_html("잠시 뒤에 다시", "PIN을 여러 번 틀려 잠시 막았습니다. 몇 분 뒤에 다시 해 주세요."), 429));
                        continue;
                    }
                    let mut body = String::new();
                    let _ = std::io::Read::read_to_string(&mut std::io::Read::take(req.as_reader(), 4096), &mut body);
                    let given = body.split('&').find_map(|kv| kv.strip_prefix("pin=")).map(form_decode).unwrap_or_default();
                    if given == pin {
                        *entry = (0, None);
                        let mut sid = [0u8; 16];
                        let _ = getrandom::getrandom(&mut sid);
                        let sid: String = sid.iter().map(|b| format!("{:02x}", b)).collect();
                        sessions.insert(sid.clone());
                        let set = format!("{COOKIE}={sid}; Path={prefix}; HttpOnly; SameSite=Strict");
                        let resp = tiny_http::Response::empty(303)
                            .with_header(tiny_http::Header::from_bytes(&b"Location"[..], prefix.as_bytes()).unwrap())
                            .with_header(tiny_http::Header::from_bytes(&b"Set-Cookie"[..], set.as_bytes()).unwrap());
                        let _ = req.respond(resp);
                    } else {
                        entry.0 += 1;
                        if entry.0 >= MAX_PIN_TRIES {
                            *entry = (0, Some(now + Duration::from_secs(PIN_BLOCK_SECS)));
                        }
                        if let Ok(mut l) = log.lock() {
                            l.push(format!("{}  {} PIN 틀림", chrono::Local::now().format("%H:%M:%S"), who));
                        }
                        let _ = req.respond(html(pin_html(&title, true), 200));
                    }
                    continue;
                }
                if !signed_in {
                    let _ = req.respond(html(pin_html(&title, false), 200));
                    continue;
                }
            }
            let rest = &path[prefix.len()..];
            if rest.is_empty() {
                visits.fetch_add(1, Ordering::SeqCst);
                if let Ok(mut l) = log.lock() {
                    l.push(format!("{}  {} 페이지 열람", chrono::Local::now().format("%H:%M:%S"), who));
                }
                let page = page_html(&title, &message, &items, |i| format!("{}f/{}", prefix, i), Some(expires_ms));
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

    fn local(live: &LiveShare) -> String {
        live.url.replace(&lan_ip(), "127.0.0.1")
    }
    fn client() -> ureq::Agent {
        ureq::AgentBuilder::new().redirects(0).build()
    }
    /// Sends the PIN form; returns (status, the session cookie if one was given).
    fn send_pin(url: &str, pin: &str) -> (u16, Option<String>) {
        let r = match client().post(url).set("Content-Type", "application/x-www-form-urlencoded").send_string(&format!("pin={pin}")) {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(e) => panic!("{e}"),
        };
        let cookie = r.header("Set-Cookie").map(|c| c.split(';').next().unwrap_or("").to_string());
        (r.status(), cookie)
    }
    fn get(url: &str, cookie: Option<&str>) -> (u16, Vec<u8>) {
        let mut req = client().get(url);
        if let Some(c) = cookie {
            req = req.set("Cookie", c);
        }
        let r = match req.call() {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(e) => panic!("{e}"),
        };
        let status = r.status();
        let mut body = Vec::new();
        r.into_reader().read_to_end(&mut body).unwrap();
        (status, body)
    }

    #[test]
    fn live_share_serves_page_pin_and_file() {
        let items = vec![ShareItem { name: "메모.txt".into(), kind: "텍스트".into(), bytes: Arc::new(b"hello".to_vec()) }];
        let live = start_live("테스트".into(), "안녕".into(), items, "1234".into(), 1).unwrap();
        let url = local(&live);
        let (_, page) = get(&url, None);
        assert!(String::from_utf8_lossy(&page).contains("PIN"));
        let (code, cookie) = send_pin(&url, "1234");
        assert_eq!(code, 303, "a right PIN sends the browser back to the page");
        let cookie = cookie.expect("a session cookie");
        let (_, page) = get(&url, Some(&cookie));
        let page = String::from_utf8_lossy(&page).to_string();
        assert!(page.contains("메모.txt"));
        assert!(!page.contains("1234"), "the PIN is not written into the page or its links");
        let (_, body) = get(&format!("{url}f/0"), Some(&cookie));
        assert_eq!(body, b"hello");
        assert_eq!(get(&url.replace("/s/", "/x/"), None).0, 404);
        assert_eq!(live.downloads.load(Ordering::SeqCst), 1);
        live.stop();
    }

    /// The PIN in the address (old links, browser history, proxies) opens nothing.
    #[test]
    fn pin_in_the_address_opens_nothing() {
        let items = vec![ShareItem { name: "a.txt".into(), kind: "t".into(), bytes: Arc::new(b"SECRET".to_vec()) }];
        let live = start_live("t".into(), "".into(), items, "1234".into(), 1).unwrap();
        let url = local(&live);
        let (_, page) = get(&format!("{url}?pin=1234"), None);
        assert!(!String::from_utf8_lossy(&page).contains("a.txt"));
        let (_, body) = get(&format!("{url}f/0?pin=1234"), None);
        assert_ne!(body, b"SECRET");
        let (_, body) = get(&format!("{url}f/0"), Some("mdd_s=made-up"));
        assert_ne!(body, b"SECRET", "a made-up cookie opens nothing");
        live.stop();
    }

    /// Guessing is stopped: after 5 wrong PINs the right one is refused for a while.
    #[test]
    fn wrong_pins_are_limited() {
        let items = vec![ShareItem { name: "a.txt".into(), kind: "t".into(), bytes: Arc::new(b"x".to_vec()) }];
        let live = start_live("t".into(), "".into(), items, "1234".into(), 1).unwrap();
        let url = local(&live);
        for _ in 0..5 {
            let (code, cookie) = send_pin(&url, "0000");
            assert!(cookie.is_none());
            assert_eq!(code, 200);
        }
        let (code, cookie) = send_pin(&url, "1234");
        assert_eq!(code, 429, "too many tries");
        assert!(cookie.is_none());
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

    /// audit M-06: every export is a new folder, so files picked before can't come along.
    #[test]
    fn each_export_is_a_fresh_folder() {
        let parent = std::env::temp_dir().join(format!("mdd-export-{}", crate::model::new_id()));
        std::fs::create_dir_all(&parent).unwrap();
        let item = |n: &str, b: &[u8]| ShareItem { name: n.into(), kind: "text".into(), bytes: Arc::new(b.to_vec()) };
        let first = export_folder(&parent, "First", "", &[item("secret.txt", b"SECRET")]).unwrap();
        let second = export_folder(&parent, "Second", "", &[item("public.txt", b"PUBLIC")]).unwrap();
        assert_ne!(first, second);
        assert!(!second.join("files").join("secret.txt").exists());
        assert!(second.join("files").join("public.txt").exists());
        let _ = std::fs::remove_dir_all(&parent);
    }
}
