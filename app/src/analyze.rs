//! One-document analysis: extract → structure → content (pictures, personal info) → result
//! (summary, reliability, complexity levels, Markdown and JSON output).

use crate::ai;
use crate::docparse::Media;
use crate::extract;
use crate::model::*;
use crate::search::tokens;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Instant;

#[derive(Clone, Copy)]
pub struct Opts {
    pub summary: bool,
    pub vision: bool,
    pub metrics: bool,
    pub mask_pii: bool,
    pub save: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { summary: true, vision: true, metrics: true, mask_pii: false, save: true }
    }
}

pub const STEPS: [(&str, &str); 4] = [
    ("글자 꺼내기", "형식에 맞춰 본문 읽기"),
    ("구조 파악", "제목·표·표 안의 표"),
    ("그림·민감정보", "그림 해석·개인정보 가림"),
    ("결과 정리", "요약·지표·MD·JSON"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Heading,
    Paragraph,
    List,
    Table,
    Chart,
    Figure,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Heading => "heading",
            Kind::Paragraph => "paragraph",
            Kind::List => "list",
            Kind::Table => "table",
            Kind::Chart => "chart",
            Kind::Figure => "figure",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Block {
    pub kind: Kind,
    /// 1 plain text · 2 headings and lists · 3 tables · 4 tables in tables, charts, pictures
    pub level: u8,
    pub text: String,
    pub rows: Vec<Vec<String>>,
}

pub struct Analysis {
    pub doc: Document,
    pub bytes: Vec<u8>,
    pub blocks: Vec<Block>,
    pub markdown: String,
    pub json: String,
    pub summary: Vec<String>,
    pub summary_by_ai: bool,
    pub confidence: u8,
    pub levels: [u32; 4],
    pub masked: usize,
    pub notes: Vec<String>,
    pub usage: Vec<UsageEntry>,
    pub seconds: f32,
}

pub enum Msg {
    Step(usize, f32, String),
    Done(Box<Result<Analysis, String>>),
}

// ---------------- structure ----------------

fn is_list(t: &str) -> bool {
    t.starts_with("- ") || t.starts_with("• ") || t.starts_with("○") || t.starts_with("·") || t.starts_with("□") || t.starts_with("■") || {
        let d: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        !d.is_empty() && d.len() <= 2 && t[d.len()..].starts_with(". ") && t.chars().count() > 60
    }
}

fn is_heading(t: &str) -> bool {
    t.starts_with('#') || {
        let n = t.chars().count();
        let d: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        (!d.is_empty() && d.len() <= 2 && t[d.len()..].starts_with(". ") && n <= 60)
            || (t.starts_with('제') && t.contains('조') && t.contains('(') && n <= 40)
            || (t.starts_with("# 슬라이드") || t.starts_with("# 시트"))
    }
}

pub fn blocks(text: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let mut para = String::new();
    let flush = |para: &mut String, out: &mut Vec<Block>| {
        if !para.trim().is_empty() {
            out.push(Block { kind: Kind::Paragraph, level: 1, text: para.trim().to_string(), rows: vec![] });
        }
        para.clear();
    };
    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() {
            flush(&mut para, &mut out);
            i += 1;
        } else if t.contains(" | ") || t.ends_with(" |") {
            flush(&mut para, &mut out);
            let mut rows = Vec::new();
            while i < lines.len() && (lines[i].contains(" | ") || lines[i].trim_end().ends_with(" |")) {
                let cells: Vec<String> = lines[i].trim().trim_end_matches('|').split(" | ").map(|c| c.trim().to_string()).collect();
                rows.push(cells);
                i += 1;
            }
            let nested = rows.iter().flatten().any(|c| c.contains("[표:"));
            out.push(Block { kind: Kind::Table, level: if nested { 4 } else { 3 }, text: String::new(), rows });
        } else if t.starts_with("[차트") {
            flush(&mut para, &mut out);
            let mut s = String::new();
            while i < lines.len() && !lines[i].trim().is_empty() {
                s.push_str(lines[i].trim());
                s.push('\n');
                i += 1;
            }
            out.push(Block { kind: Kind::Chart, level: 4, text: s.trim().to_string(), rows: vec![] });
        } else if t.starts_with("## 그림") || t == "[그림]" {
            flush(&mut para, &mut out);
            let mut s = String::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("## ") {
                if !s.is_empty() || !lines[i].trim().is_empty() {
                    s.push_str(lines[i]);
                    s.push('\n');
                }
                i += 1;
            }
            out.push(Block { kind: Kind::Figure, level: 4, text: if s.trim().is_empty() { "(그림 — 해석 안 함)".into() } else { s.trim().to_string() }, rows: vec![] });
        } else if is_heading(t) {
            flush(&mut para, &mut out);
            out.push(Block { kind: Kind::Heading, level: 2, text: t.trim_start_matches('#').trim().to_string(), rows: vec![] });
            i += 1;
        } else if is_list(t) {
            flush(&mut para, &mut out);
            let mut s = String::new();
            while i < lines.len() && is_list(lines[i].trim()) {
                s.push_str(lines[i].trim());
                s.push('\n');
                i += 1;
            }
            out.push(Block { kind: Kind::List, level: 2, text: s.trim().to_string(), rows: vec![] });
        } else {
            if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(t);
            i += 1;
        }
    }
    flush(&mut para, &mut out);
    out
}

pub fn level_counts(b: &[Block]) -> [u32; 4] {
    let mut l = [0u32; 4];
    for x in b {
        l[(x.level.clamp(1, 4) - 1) as usize] += 1;
    }
    l
}

// ---------------- outputs ----------------

pub fn markdown(title: &str, b: &[Block]) -> String {
    let mut s = format!("# {}\n\n", title);
    for x in b {
        match x.kind {
            Kind::Heading => s.push_str(&format!("## {}\n\n", x.text)),
            Kind::Paragraph => s.push_str(&format!("{}\n\n", x.text)),
            Kind::List => {
                for l in x.text.lines() {
                    let l = l.trim_start_matches(['-', '•', '·']).trim();
                    s.push_str(&format!("- {}\n", l));
                }
                s.push('\n');
            }
            Kind::Table => {
                let cols = x.rows.iter().map(|r| r.len()).max().unwrap_or(1);
                for (ri, r) in x.rows.iter().enumerate() {
                    let mut cells: Vec<String> = r.iter().map(|c| c.replace('|', "\\|")).collect();
                    cells.resize(cols, String::new());
                    s.push_str(&format!("| {} |\n", cells.join(" | ")));
                    if ri == 0 {
                        s.push_str(&format!("|{}\n", " --- |".repeat(cols)));
                    }
                }
                s.push('\n');
            }
            Kind::Chart => s.push_str(&format!("> **차트**\n> {}\n\n", x.text.replace('\n', "\n> "))),
            Kind::Figure => s.push_str(&format!("> **그림**\n> {}\n\n", x.text.replace('\n', "\n> "))),
        }
    }
    s
}

fn json_out(a: &Analysis) -> String {
    let blocks: Vec<serde_json::Value> = a
        .blocks
        .iter()
        .map(|b| {
            if b.kind == Kind::Table {
                json!({"type": "table", "level": b.level, "rows": b.rows})
            } else {
                json!({"type": b.kind.label(), "level": b.level, "content": b.text})
            }
        })
        .collect();
    let v = json!({
        "file": a.doc.file_name,
        "format": a.doc.kind,
        "summary": a.summary,
        "metrics": {
            "reliability": a.confidence,
            "complexity": a.doc.complexity,
            "levels": {"L1": a.levels[0], "L2": a.levels[1], "L3": a.levels[2], "L4": a.levels[3]},
            "tables": a.doc.tables,
            "nested_tables": a.doc.nested_tables,
            "images": a.doc.images,
            "images_described": a.doc.ai_pictures,
            "charts": a.doc.charts,
            "masked_personal_info": a.masked
        },
        "notes": a.notes,
        "blocks": blocks
    });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

// ---------------- content ----------------

/// Hides resident numbers, phone numbers, e-mails and card numbers. Returns how many.
pub fn mask_pii(text: &str) -> (String, usize) {
    let rules: [(&str, &str); 4] = [
        (r"(^|\D)(\d{6})[-\s]?([1-4])\d{6}(\D|$)", "${1}${2}-${3}******${4}"),
        (r"(^|\D)(\d{4})[-\s](\d{4})[-\s](\d{4})[-\s](\d{4})(\D|$)", "${1}****-****-****-${5}${6}"),
        (r"(^|\D)(01[016789]|0\d{1,2})[-.\s](\d{3,4})[-.\s](\d{4})(\D|$)", "${1}${2}-****-${4}${5}"),
        (r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", "***@***"),
    ];
    let mut s = text.to_string();
    let mut n = 0;
    for (pat, rep) in rules {
        let re = regex::Regex::new(pat).expect("valid pattern");
        n += re.find_iter(&s).count();
        s = re.replace_all(&s, rep).into_owned();
    }
    (s, n)
}

/// The three sentences that share the most words with the whole document (no AI needed).
pub fn key_sentences(text: &str, n: usize) -> Vec<String> {
    let body: String = text.lines().filter(|l| !l.contains(" | ")).collect::<Vec<_>>().join(" ");
    let mut sents: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in body.chars() {
        cur.push(ch);
        if ch == '.' || ch == '?' || ch == '!' || ch == '。' {
            if cur.trim().chars().count() > 15 {
                sents.push(cur.trim().to_string());
            }
            cur.clear();
        }
    }
    if cur.trim().chars().count() > 15 {
        sents.push(cur.trim().to_string());
    }
    let mut freq: HashMap<String, f32> = HashMap::new();
    for t in tokens(&body) {
        *freq.entry(t).or_default() += 1.0;
    }
    let mut scored: Vec<(usize, f32)> = sents
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let toks = tokens(s);
            let sc: f32 = toks.iter().map(|t| freq.get(t).copied().unwrap_or(0.0)).sum::<f32>() / (toks.len() as f32 + 4.0);
            (i, sc + if i < 3 { 0.5 } else { 0.0 })
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut pick: Vec<usize> = scored.iter().take(n).map(|x| x.0).collect();
    pick.sort();
    pick.into_iter().map(|i| crate::search::excerpt(&sents[i], 140)).collect()
}

/// An estimate of how completely the file was read (0..100).
pub fn reliability(doc: &Document, media_total: usize, ext: &str) -> (u8, Vec<String>) {
    let mut c: f32 = 100.0;
    let mut notes = Vec::new();
    let chars = doc.text.chars().count().max(1);
    if doc.parse_note.contains("미리보기") {
        c -= 45.0;
        notes.push("본문 대신 미리보기 글자만 읽었습니다".into());
    }
    let bad = doc.text.chars().filter(|c| *c == '\u{FFFD}').count();
    if bad > 0 {
        c -= (bad as f32 / chars as f32 * 500.0).min(30.0);
        notes.push(format!("깨진 글자 {}개", bad));
    }
    let unread = media_total.saturating_sub(doc.ai_pictures as usize);
    if unread > 0 {
        c -= (unread as f32 * 4.0).min(20.0);
        notes.push(format!("해석하지 않은 그림 {}개", unread));
    }
    if ext == "pdf" && (chars as f32) < doc.size_bytes as f32 / 1024.0 * 3.0 {
        c -= 30.0;
        notes.push("파일 크기에 비해 글자가 적습니다 (스캔 PDF일 수 있음)".into());
    }
    if chars < 40 {
        c -= 25.0;
        notes.push("읽은 글자가 아주 적습니다".into());
    }
    (c.clamp(0.0, 100.0).round() as u8, notes)
}

fn ai_summary(settings: &Settings, title: &str, text: &str) -> Result<(Vec<String>, ai::AiReply), String> {
    let body: String = text.chars().take(7000).collect();
    let rep = ai::ask(
        settings,
        "당신은 문서 분석가입니다. 문서를 한국어 짧은 문장 3개로 요약하세요. 각 문장은 '- '로 시작하고, 문서의 목적, 담긴 핵심 정보, 쓰임새를 차례로 말하세요.",
        &format!("문서 제목: {title}\n\n{body}"),
    )?;
    let lines: Vec<String> = rep.text.lines().map(|l| l.trim().trim_start_matches(['-', '•', '*']).trim().to_string()).filter(|l| !l.is_empty()).take(5).collect();
    Ok((lines, rep))
}

fn usage_of(settings: &Settings, rep: &ai::AiReply) -> UsageEntry {
    UsageEntry {
        id: new_id(),
        ms: now_ms(),
        model: rep.model.clone(),
        prompt_tokens: rep.prompt_tokens,
        completion_tokens: rep.completion_tokens,
        cost_usd: ai::cost(settings, rep.prompt_tokens, rep.completion_tokens),
        agent: "문서 분석".into(),
    }
}

pub fn ai_ready(settings: &Settings) -> bool {
    settings.provider != Provider::None && !(settings.local_only && settings.provider.is_cloud())
}

/// Steps 2-4 for a document already read. `settings` None = no AI at all.
pub fn build(mut doc: Document, bytes: Vec<u8>, media: Vec<Media>, ext: &str, opts: &Opts, settings: Option<&Settings>, step: &dyn Fn(usize, f32, String)) -> Analysis {
    let started = Instant::now();
    let mut usage = Vec::new();
    let mut notes = Vec::new();
    step(1, 0.35, "구조를 나누는 중".into());
    let ai_ok = settings.map(ai_ready).unwrap_or(false);

    step(2, 0.45, "그림과 민감정보를 처리하는 중".into());
    if opts.vision && !media.is_empty() {
        if let (true, Some(s)) = (ai_ok, settings) {
            let n = media.len().min(8);
            for (mi, m) in media.iter().take(n).enumerate() {
                step(2, 0.45 + 0.3 * mi as f32 / n as f32, format!("그림 {}/{} 해석 중", mi + 1, n));
                match ai::describe_image(s, m.mime, &m.bytes) {
                    Ok(rep) => {
                        usage.push(usage_of(s, &rep));
                        let short = m.name.rsplit('/').next().unwrap_or("");
                        doc.text.push_str(&format!("\n\n## 그림 {} (AI 해석 · {})\n{}", mi + 1, short, rep.text.trim()));
                        doc.ai_pictures += 1;
                    }
                    Err(e) => {
                        notes.push(format!("그림 해석 중단: {e}"));
                        break;
                    }
                }
            }
        } else {
            notes.push(format!("그림 {}개는 AI가 연결되지 않아 해석하지 않았습니다", media.len()));
        }
    }
    let mut masked = 0;
    if opts.mask_pii {
        let (t, n) = mask_pii(&doc.text);
        doc.text = t;
        masked = n;
    }
    let (_, tables, headings) = extract::complexity(&doc.text);
    doc.tables = doc.tables.max(tables);
    doc.headings = headings;
    extract::rescore(&mut doc);
    if doc.text.trim().is_empty() {
        doc.text = "(읽은 글자가 없습니다)".into();
    }

    step(3, 0.85, "요약과 결과 파일을 만드는 중".into());
    let blocks = blocks(&doc.text);
    let levels = level_counts(&blocks);
    let (mut summary, mut by_ai) = (Vec::new(), false);
    if opts.summary {
        if let (true, Some(s)) = (ai_ok, settings) {
            match ai_summary(s, &doc.title, &doc.text) {
                Ok((lines, rep)) => {
                    usage.push(usage_of(s, &rep));
                    summary = lines;
                    by_ai = true;
                }
                Err(e) => notes.push(format!("AI 요약 실패, 핵심 문장으로 대신함: {e}")),
            }
        }
        if summary.is_empty() {
            summary = key_sentences(&doc.text, 3);
        }
    }
    let (confidence, mut rel_notes) = reliability(&doc, media.len(), ext);
    rel_notes.extend(notes);
    let md = markdown(&doc.title, &blocks);
    let mut a = Analysis {
        doc,
        bytes,
        blocks,
        markdown: md,
        json: String::new(),
        summary,
        summary_by_ai: by_ai,
        confidence: if opts.metrics { confidence } else { 0 },
        levels,
        masked,
        notes: rel_notes,
        usage,
        seconds: 0.0,
    };
    a.json = json_out(&a);
    a.seconds = started.elapsed().as_secs_f32();
    step(3, 1.0, "완료".into());
    a
}

pub fn run(path: PathBuf, opts: Opts, settings: Settings, tx: Sender<Msg>) {
    let started = Instant::now();
    let step = |i: usize, p: f32, m: String| {
        let _ = tx.send(Msg::Step(i, p, m));
    };
    step(0, 0.05, "파일을 여는 중".into());
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let r = extract::document_from_file(&path).map(|(doc, bytes, media)| {
        step(0, 0.25, format!("글자 {}자를 꺼냈습니다", doc.text.chars().count()));
        let mut a = build(doc, bytes, media, &ext, &opts, Some(&settings), &step);
        a.seconds = started.elapsed().as_secs_f32();
        a
    });
    let _ = tx.send(Msg::Done(Box::new(r)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_markdown_and_masking() {
        let text = "# 근로계약서\n\n1. 근로자 인적사항\n성명 | 생년월일 | 연락처\n홍길동 | 900101 | [표: 휴대폰 · 010-1234-5678]\n\n본 계약은 양 당사자가 서명한 날부터 효력이 생긴다. 근무시간은 하루 8시간으로 한다.\n\n[차트 · 막대] 월별\n- 계열 1: 1월=3";
        let b = blocks(text);
        let kinds: Vec<Kind> = b.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, vec![Kind::Heading, Kind::Heading, Kind::Table, Kind::Paragraph, Kind::Chart]);
        assert_eq!(b[2].level, 4);
        let md = markdown("t", &b);
        assert!(md.contains("| 성명 | 생년월일 | 연락처 |\n| --- | --- | --- |"), "{md}");
        let (m, n) = mask_pii("주민번호 900101-1234567, 전화 010-1234-5678, 메일 a.b@ex.com");
        assert_eq!(n, 3, "{m}");
        assert!(m.contains("900101-1******") && m.contains("010-****-5678") && m.contains("***@***"), "{m}");
        assert_eq!(key_sentences(text, 2).len(), 2);
    }
}
