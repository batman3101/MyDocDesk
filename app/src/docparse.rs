//! Structure-aware document reading: old binary HWP, tables inside tables, chart data,
//! spreadsheets, and the pictures a document carries (for optional AI description).

use std::io::{Cursor, Read};

#[derive(Default, Debug)]
pub struct Parsed {
    pub text: String,
    pub tables: u32,
    pub nested: u32,
    pub images: u32,
    pub charts: u32,
    pub media: Vec<Media>,
    pub note: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Media {
    pub name: String,
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

pub fn image_mime(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.starts_with(b"GIF8") {
        Some("image/gif")
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Pictures worth describing: real images, not tiny icons or bullets.
pub fn keep_media(m: &mut Vec<Media>, name: &str, bytes: Vec<u8>) {
    if m.len() >= 12 || bytes.len() < 6 * 1024 || bytes.len() > 8 * 1024 * 1024 {
        return;
    }
    if let Some(mime) = image_mime(&bytes) {
        m.push(Media { name: name.to_string(), mime, bytes });
    }
}

// ---------------- a tiny XML event reader ----------------

pub enum Ev<'a> {
    Open(&'a str, &'a str, bool),
    Close(&'a str),
    Text(&'a str),
}

pub fn xml_events<'a>(xml: &'a str, mut f: impl FnMut(Ev<'a>)) {
    let mut i = 0;
    while let Some(off) = xml[i..].find('<') {
        let lt = i + off;
        if lt > i {
            f(Ev::Text(&xml[i..lt]));
        }
        if xml[lt..].starts_with("<!--") {
            i = xml[lt..].find("-->").map(|e| lt + e + 3).unwrap_or(xml.len());
            continue;
        }
        if xml[lt..].starts_with("<![CDATA[") {
            let end = xml[lt..].find("]]>").map(|e| lt + e).unwrap_or(xml.len());
            f(Ev::Text(&xml[lt + 9..end]));
            i = (end + 3).min(xml.len());
            continue;
        }
        let Some(gt) = xml[lt..].find('>') else { break };
        let gt = lt + gt;
        let tag = &xml[lt + 1..gt];
        i = gt + 1;
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            f(Ev::Close(name.trim()));
            continue;
        }
        let self_close = tag.ends_with('/');
        let body = tag.trim_end_matches('/');
        let (name, attrs) = match body.find(|c: char| c.is_whitespace()) {
            Some(p) => (&body[..p], &body[p..]),
            None => (body, ""),
        };
        f(Ev::Open(name, attrs, self_close));
        if self_close {
            f(Ev::Close(name));
        }
    }
    if i < xml.len() {
        f(Ev::Text(&xml[i..]));
    }
}

pub fn attr<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!(" {key}=\"");
    let p = attrs.find(&pat)? + pat.len();
    let e = attrs[p..].find('"')? + p;
    Some(&attrs[p..e])
}

pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let tail = &rest[p..];
        if let Some(e) = tail.find(';').filter(|e| *e < 10) {
            let ent = &tail[1..e];
            let rep = match ent {
                "lt" => Some('<'),
                "gt" => Some('>'),
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
                _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            if let Some(c) = rep {
                out.push(c);
                rest = &tail[e + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

// ---------------- tables (also tables inside tables) ----------------

/// Element names of one document format.
pub struct Names {
    pub tbl: &'static str,
    pub tr: &'static str,
    pub tc: &'static str,
    pub p: &'static [&'static str],
    pub t: &'static [&'static str],
    pub tab: &'static [&'static str],
    pub br: &'static [&'static str],
    pub img: &'static [&'static str],
    pub chart: &'static [&'static str],
}

pub const DOCX: Names = Names { tbl: "w:tbl", tr: "w:tr", tc: "w:tc", p: &["w:p"], t: &["w:t"], tab: &["w:tab"], br: &["w:br", "w:cr"], img: &["w:drawing", "w:pict"], chart: &["c:chart"] };
pub const HWPX: Names = Names { tbl: "hp:tbl", tr: "hp:tr", tc: "hp:tc", p: &["hp:p"], t: &["hp:t"], tab: &["hp:tab"], br: &["hp:lineBreak"], img: &["hp:pic"], chart: &["hp:chart"] };
pub const PPTX: Names = Names { tbl: "a:tbl", tr: "a:tr", tc: "a:tc", p: &["a:p"], t: &["a:t"], tab: &[], br: &["a:br"], img: &["p:pic"], chart: &["c:chart"] };
pub const ODT: Names = Names { tbl: "table:table", tr: "table:table-row", tc: "table:table-cell", p: &["text:p", "text:h"], t: &["text:p", "text:h"], tab: &["text:tab"], br: &["text:line-break"], img: &["draw:image"], chart: &[] };

#[derive(Default)]
struct TableCtx {
    rows: Vec<Vec<String>>,
    cell: Option<String>,
}

/// Renders collected rows: a top-level table becomes one line per row ("a | b | c"),
/// a table inside a cell becomes "[표: a · b / c · d]" so the cell keeps it.
fn render_rows(rows: &[Vec<String>], nested: bool) -> String {
    let rows: Vec<&Vec<String>> = rows.iter().filter(|r| r.iter().any(|c| !c.is_empty())).collect();
    if nested {
        let body: Vec<String> = rows.iter().map(|r| r.join(" · ")).collect();
        format!(" [표: {}] ", body.join(" / "))
    } else {
        let mut s = String::from("\n");
        for r in rows {
            s.push_str(&r.join(" | "));
            if r.len() == 1 {
                s.push_str(" |");
            }
            s.push('\n');
        }
        s.push('\n');
        s
    }
}

fn clean_cell(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn structured_xml(xml: &str, n: &Names, p: &mut Parsed) -> String {
    let mut out = String::new();
    let mut stack: Vec<TableCtx> = Vec::new();
    let mut in_t = 0usize;
    fn target<'a>(out: &'a mut String, stack: &'a mut [TableCtx]) -> (&'a mut String, bool) {
        match stack.last_mut().and_then(|t| t.cell.as_mut()) {
            Some(c) => (c, true),
            None => (out, false),
        }
    }
    xml_events(xml, |ev| match ev {
        Ev::Text(t) => {
            if in_t > 0 {
                let (buf, _) = target(&mut out, &mut stack);
                buf.push_str(&unescape(t));
            }
        }
        Ev::Open(name, _, self_close) => {
            if n.t.contains(&name) && !self_close {
                in_t += 1;
            }
            if n.tab.contains(&name) {
                let (buf, cell) = target(&mut out, &mut stack);
                buf.push(if cell { ' ' } else { '\t' });
            } else if n.br.contains(&name) {
                let (buf, cell) = target(&mut out, &mut stack);
                buf.push(if cell { ' ' } else { '\n' });
            } else if n.img.contains(&name) {
                p.images += 1;
            } else if n.chart.contains(&name) {
                p.charts += 1;
            } else if name == n.tbl {
                if stack.is_empty() {
                    p.tables += 1;
                } else {
                    p.nested += 1;
                }
                stack.push(TableCtx::default());
            } else if name == n.tr {
                if let Some(t) = stack.last_mut() {
                    t.rows.push(Vec::new());
                }
            } else if name == n.tc {
                if let Some(t) = stack.last_mut() {
                    t.cell = Some(String::new());
                }
            }
        }
        Ev::Close(name) => {
            if n.t.contains(&name) {
                in_t = in_t.saturating_sub(1);
            }
            if n.p.contains(&name) {
                let (buf, cell) = target(&mut out, &mut stack);
                buf.push(if cell { ' ' } else { '\n' });
            } else if name == n.tc {
                if let Some(t) = stack.last_mut() {
                    if let Some(c) = t.cell.take() {
                        if t.rows.is_empty() {
                            t.rows.push(Vec::new());
                        }
                        t.rows.last_mut().unwrap().push(clean_cell(&c));
                    }
                }
            } else if name == n.tbl {
                if let Some(t) = stack.pop() {
                    let nested = !stack.is_empty();
                    let s = render_rows(&t.rows, nested);
                    let (buf, _) = target(&mut out, &mut stack);
                    buf.push_str(&s);
                }
            }
        }
    });
    tidy(&out)
}

pub fn tidy(text: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut blank = 0;
    for l in text.lines() {
        let t = l.trim_end();
        if t.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        lines.push(t);
    }
    lines.join("\n").trim().to_string()
}

// ---------------- charts ----------------

/// Reads a DrawingML chart part (word/charts/chart1.xml...) into words and numbers.
pub fn chart_text(xml: &str) -> String {
    #[derive(Default)]
    struct Ser {
        name: String,
        cats: Vec<String>,
        vals: Vec<String>,
    }
    let mut title = String::new();
    let mut kind = String::new();
    let mut series: Vec<Ser> = Vec::new();
    let (mut in_title, mut in_ser, mut in_tx, mut in_cat, mut in_val, mut in_v, mut in_at) = (false, false, false, false, false, false, false);
    let mut cur = Ser::default();
    xml_events(xml, |ev| match ev {
        Ev::Open(name, _, sc) => match name {
            "c:title" if !in_ser => in_title = true,
            "c:ser" => {
                in_ser = true;
                cur = Ser::default();
            }
            "c:tx" if in_ser => in_tx = true,
            "c:cat" | "c:xVal" => in_cat = true,
            "c:val" | "c:yVal" => in_val = true,
            "c:v" if !sc => in_v = true,
            "a:t" if !sc => in_at = true,
            n if kind.is_empty() && n.starts_with("c:") && n.ends_with("Chart") && n != "c:chart" => {
                kind = match n {
                    "c:barChart" | "c:bar3DChart" => "막대",
                    "c:lineChart" | "c:line3DChart" => "꺾은선",
                    "c:pieChart" | "c:pie3DChart" | "c:doughnutChart" => "원형",
                    "c:areaChart" => "영역",
                    "c:scatterChart" => "분산형",
                    "c:radarChart" => "방사형",
                    _ => "기타",
                }
                .into()
            }
            _ => {}
        },
        Ev::Close(name) => match name {
            "c:title" => in_title = false,
            "c:ser" => {
                in_ser = false;
                series.push(std::mem::take(&mut cur));
            }
            "c:tx" => in_tx = false,
            "c:cat" | "c:xVal" => in_cat = false,
            "c:val" | "c:yVal" => in_val = false,
            "c:v" => in_v = false,
            "a:t" => in_at = false,
            _ => {}
        },
        Ev::Text(t) => {
            let t = unescape(t.trim());
            if t.is_empty() {
                return;
            }
            if in_title && in_at && !in_ser {
                title.push_str(&t);
            } else if in_ser && in_v {
                if in_tx {
                    cur.name = t;
                } else if in_cat {
                    cur.cats.push(t);
                } else if in_val {
                    cur.vals.push(t);
                }
            } else if in_ser && in_tx && in_at {
                cur.name = t;
            }
        }
    });
    if series.is_empty() {
        return String::new();
    }
    let mut s = format!("[차트{}] {}\n", if kind.is_empty() { String::new() } else { format!(" · {kind}") }, if title.is_empty() { "(제목 없음)" } else { &title });
    for (i, se) in series.iter().enumerate() {
        let name = if se.name.is_empty() { format!("계열 {}", i + 1) } else { se.name.clone() };
        let pts: Vec<String> = se
            .vals
            .iter()
            .enumerate()
            .take(40)
            .map(|(k, v)| match se.cats.get(k) {
                Some(c) => format!("{c}={v}"),
                None => v.clone(),
            })
            .collect();
        s.push_str(&format!("- {}: {}\n", name, pts.join(", ")));
    }
    s
}

// ---------------- zip-based office formats ----------------

fn read_entry(z: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let mut f = z.by_name(name).ok()?;
    let mut b = Vec::new();
    f.read_to_end(&mut b).ok()?;
    Some(b)
}

fn read_str(z: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    read_entry(z, name).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn sorted(z: &zip::ZipArchive<Cursor<&[u8]>>, prefix: &str, suffix: &str) -> Vec<String> {
    let mut names: Vec<String> = z.file_names().filter(|n| n.starts_with(prefix) && n.ends_with(suffix)).map(|s| s.to_string()).collect();
    names.sort_by_key(|n| {
        let digits: String = n.chars().filter(|c| c.is_ascii_digit()).collect();
        (digits.parse::<u64>().unwrap_or(0), n.clone())
    });
    names
}

fn xlsx(z: &mut zip::ZipArchive<Cursor<&[u8]>>, p: &mut Parsed) -> String {
    let mut shared: Vec<String> = Vec::new();
    if let Some(x) = read_str(z, "xl/sharedStrings.xml") {
        let mut cur = String::new();
        let mut in_t = false;
        xml_events(&x, |ev| match ev {
            Ev::Open("t", _, false) => in_t = true,
            Ev::Close("t") => in_t = false,
            Ev::Close("si") => shared.push(std::mem::take(&mut cur)),
            Ev::Text(t) if in_t => cur.push_str(&unescape(t)),
            _ => {}
        });
    }
    let mut sheet_names: Vec<String> = Vec::new();
    if let Some(x) = read_str(z, "xl/workbook.xml") {
        xml_events(&x, |ev| {
            if let Ev::Open("sheet", a, _) = ev {
                sheet_names.push(unescape(attr(a, "name").unwrap_or("")));
            }
        });
    }
    let mut out = String::new();
    for (si, name) in sorted(z, "xl/worksheets/sheet", ".xml").iter().enumerate() {
        let Some(x) = read_str(z, name) else { continue };
        let mut rows: Vec<Vec<String>> = Vec::new();
        let (mut ctype, mut val, mut in_v) = (String::new(), String::new(), false);
        xml_events(&x, |ev| match ev {
            Ev::Open("row", _, _) => rows.push(Vec::new()),
            Ev::Open("c", a, _) => {
                ctype = attr(a, "t").unwrap_or("").to_string();
                val.clear();
            }
            Ev::Open("v", _, false) | Ev::Open("t", _, false) => in_v = true,
            Ev::Close("v") | Ev::Close("t") => in_v = false,
            Ev::Text(t) if in_v => val.push_str(&unescape(t)),
            Ev::Close("c") => {
                let v = if ctype == "s" { val.trim().parse::<usize>().ok().and_then(|i| shared.get(i).cloned()).unwrap_or_default() } else { val.clone() };
                if let Some(r) = rows.last_mut() {
                    r.push(clean_cell(&v));
                }
            }
            _ => {}
        });
        rows.truncate(3000);
        if rows.iter().any(|r| r.iter().any(|c| !c.is_empty())) {
            p.tables += 1;
            out.push_str(&format!("# 시트: {}\n", sheet_names.get(si).cloned().unwrap_or_else(|| format!("{}", si + 1))));
            out.push_str(&render_rows(&rows, false));
        }
    }
    out
}

pub fn office(bytes: &[u8], ext: &str) -> Result<Parsed, String> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("압축 형식을 읽지 못했습니다: {e}"))?;
    let mut p = Parsed::default();
    let mut text = String::new();
    match ext {
        "docx" => {
            let x = read_str(&mut z, "word/document.xml").ok_or("본문(word/document.xml)이 없습니다")?;
            text = structured_xml(&x, &DOCX, &mut p);
        }
        "pptx" => {
            for (i, n) in sorted(&z, "ppt/slides/slide", ".xml").iter().enumerate() {
                if let Some(x) = read_str(&mut z, n) {
                    text.push_str(&format!("# 슬라이드 {}\n", i + 1));
                    text.push_str(&structured_xml(&x, &PPTX, &mut p));
                    text.push_str("\n\n");
                }
            }
        }
        "xlsx" => text = xlsx(&mut z, &mut p),
        "hwpx" => {
            for n in sorted(&z, "Contents/section", ".xml") {
                if let Some(x) = read_str(&mut z, &n) {
                    text.push_str(&structured_xml(&x, &HWPX, &mut p));
                    text.push('\n');
                }
            }
        }
        "odt" => {
            let x = read_str(&mut z, "content.xml").ok_or("content.xml이 없습니다")?;
            text = structured_xml(&x, &ODT, &mut p);
        }
        _ => {}
    }
    // charts keep their own data parts: read the numbers directly
    let mut charts = String::new();
    let mut chart_count = 0;
    for prefix in ["word/charts/chart", "ppt/charts/chart", "xl/charts/chart", "Chart/chart", "Charts/chart"] {
        for n in sorted(&z, prefix, ".xml") {
            if let Some(x) = read_str(&mut z, &n) {
                let c = chart_text(&x);
                if !c.is_empty() {
                    chart_count += 1;
                    charts.push_str(&c);
                    charts.push('\n');
                }
            }
        }
    }
    if chart_count > 0 {
        p.charts = p.charts.max(chart_count);
        text.push_str("\n\n## 차트 데이터\n");
        text.push_str(&charts);
    }
    // pictures, for optional AI description
    let media_names: Vec<String> = z
        .file_names()
        .filter(|n| ["word/media/", "ppt/media/", "xl/media/", "BinData/"].iter().any(|p| n.starts_with(p)))
        .map(|s| s.to_string())
        .collect();
    for n in media_names {
        if let Some(b) = read_entry(&mut z, &n) {
            keep_media(&mut p.media, &n, b);
        }
    }
    if ext == "xlsx" || ext == "hwpx" {
        p.images = p.images.max(p.media.len() as u32);
    }
    p.text = tidy(&text);
    Ok(p)
}

// ---------------- old binary HWP (HWP 5.x) ----------------

const PARA_HEADER: u16 = 66;
const PARA_TEXT: u16 = 67;
const CTRL_HEADER: u16 = 71;
const LIST_HEADER: u16 = 72;
const TABLE: u16 = 77;
const PICTURE: u16 = 85;

struct Rec<'a> {
    tag: u16,
    level: u16,
    data: &'a [u8],
}

fn records(buf: &[u8]) -> Vec<Rec<'_>> {
    let mut v = Vec::new();
    let mut pos = 0;
    while pos + 4 <= buf.len() {
        let h = u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
        pos += 4;
        let tag = (h & 0x3FF) as u16;
        let level = ((h >> 10) & 0x3FF) as u16;
        let mut size = (h >> 20) as usize;
        if size == 0xFFF {
            if pos + 4 > buf.len() {
                break;
            }
            size = u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]) as usize;
            pos += 4;
        }
        let end = (pos + size).min(buf.len());
        v.push(Rec { tag, level, data: &buf[pos..end] });
        pos = end;
    }
    v
}

fn ctrl_id(s: &[u8; 4]) -> u32 {
    ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | s[3] as u32
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

/// The text of one PARA_TEXT record; control characters are skipped by their HWP widths.
fn para_text(d: &[u8]) -> String {
    let w: Vec<u16> = d.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let mut out: Vec<u16> = Vec::with_capacity(w.len());
    let mut i = 0;
    while i < w.len() {
        let c = w[i];
        match c {
            9 => {
                out.push(9);
                i += 8;
            }
            1..=8 | 11 | 12 | 14..=23 => i += 8,
            10 => {
                out.push(10);
                i += 1;
            }
            24 => {
                out.push('-' as u16);
                i += 1;
            }
            30 | 31 => {
                out.push(32);
                i += 1;
            }
            0..=31 => i += 1,
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf16_lossy(&out)
}

fn subtree_end(recs: &[Rec], i: usize) -> usize {
    let lv = recs[i].level;
    let mut j = i + 1;
    while j < recs.len() && recs[j].level > lv {
        j += 1;
    }
    j
}

struct HwpCtx {
    tables: u32,
    nested: u32,
    images: u32,
}

/// One paragraph, with the tables and text boxes anchored in it. `depth` = how many tables we're inside.
fn hwp_para(recs: &[Rec], k: usize, depth: usize, cx: &mut HwpCtx) -> String {
    let end = subtree_end(recs, k);
    let lv = recs[k].level;
    let mut text = String::new();
    let mut extra = String::new();
    let mut j = k + 1;
    while j < end {
        let r = &recs[j];
        if r.level == lv + 1 && r.tag == PARA_TEXT {
            text.push_str(&para_text(r.data));
        } else if r.level == lv + 1 && r.tag == CTRL_HEADER && r.data.len() >= 4 {
            let id = u32::from_le_bytes([r.data[0], r.data[1], r.data[2], r.data[3]]);
            let ce = subtree_end(recs, j);
            if id == ctrl_id(b"tbl ") {
                if depth == 0 {
                    cx.tables += 1;
                } else {
                    cx.nested += 1;
                }
                let rows = hwp_table(recs, j, depth + 1, cx);
                extra.push_str(&render_rows(&rows, depth > 0));
            } else if id != ctrl_id(b"head") && id != ctrl_id(b"foot") {
                // text boxes, shapes, footnotes: the paragraphs inside them
                let mut m = j + 1;
                let mut has_pic = false;
                while m < ce {
                    if recs[m].tag == PICTURE {
                        has_pic = true;
                    }
                    if recs[m].tag == PARA_HEADER {
                        let t = hwp_para(recs, m, depth, cx);
                        if !t.trim().is_empty() {
                            extra.push_str(&format!(" {} ", t.trim()));
                        }
                        m = subtree_end(recs, m);
                    } else {
                        m += 1;
                    }
                }
                if has_pic {
                    cx.images += 1;
                    extra.push_str(" [그림] ");
                }
            }
            j = ce;
            continue;
        }
        j += 1;
    }
    let mut s = text.trim_end().to_string();
    if !extra.is_empty() {
        if depth > 0 {
            s.push_str(&extra);
        } else {
            s.push('\n');
            s.push_str(extra.trim_matches(' '));
        }
    }
    s
}

fn hwp_table(recs: &[Rec], c: usize, depth: usize, cx: &mut HwpCtx) -> Vec<Vec<String>> {
    let end = subtree_end(recs, c);
    let lv = recs[c].level + 1;
    let mut cols = 0usize;
    let mut cells: Vec<(Option<u16>, String)> = Vec::new();
    let mut j = c + 1;
    while j < end {
        let r = &recs[j];
        if r.level == lv && r.tag == TABLE {
            cols = u16_at(r.data, 6).unwrap_or(0) as usize;
        } else if r.level == lv && r.tag == LIST_HEADER {
            cells.push((u16_at(r.data, 10), String::new()));
        } else if r.level == lv && r.tag == PARA_HEADER {
            let t = hwp_para(recs, j, depth, cx);
            if let Some(cell) = cells.last_mut() {
                if !cell.1.is_empty() {
                    cell.1.push(' ');
                }
                cell.1.push_str(t.trim());
            }
            j = subtree_end(recs, j);
            continue;
        }
        j += 1;
    }
    // group cells into rows by their row address; fall back to the column count
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut last_row: Option<u16> = None;
    let addressed = cells.iter().all(|c| c.0.is_some()) && cells.windows(2).all(|w| w[0].0 <= w[1].0);
    for (i, (row, t)) in cells.into_iter().enumerate() {
        let new_row = if addressed { row != last_row } else { cols > 0 && i % cols == 0 || rows.is_empty() };
        if new_row || rows.is_empty() {
            rows.push(Vec::new());
        }
        last_row = row;
        rows.last_mut().unwrap().push(clean_cell(&t));
    }
    rows
}

pub fn hwp(bytes: &[u8]) -> Result<Parsed, String> {
    let mut cf = cfb::CompoundFile::open(Cursor::new(bytes)).map_err(|_| "HWP 파일 구조를 읽지 못했습니다 (손상되었거나 HWP 3.0 이전 형식일 수 있음)".to_string())?;
    let mut hdr = Vec::new();
    cf.open_stream("/FileHeader").map_err(|_| "HWP 머리말(FileHeader)이 없습니다".to_string())?.read_to_end(&mut hdr).map_err(|e| e.to_string())?;
    if hdr.len() < 40 || !hdr.starts_with(b"HWP Document File") {
        return Err("HWP 5.0 형식이 아닙니다".into());
    }
    let flags = u32::from_le_bytes([hdr[36], hdr[37], hdr[38], hdr[39]]);
    let compressed = flags & 1 != 0;
    let inflate = |raw: Vec<u8>| -> Vec<u8> {
        if !compressed {
            return raw;
        }
        let mut out = Vec::new();
        match flate2::read::DeflateDecoder::new(&raw[..]).read_to_end(&mut out) {
            Ok(_) => out,
            Err(_) => raw,
        }
    };
    let mut p = Parsed::default();
    if flags & 2 != 0 {
        return Err("암호가 걸린 HWP 문서입니다. 한글에서 암호를 풀고 저장한 뒤 넣어 주세요.".into());
    }
    let distribution = flags & 4 != 0;

    let mut names: Vec<(u32, String)> = Vec::new();
    let mut bins: Vec<String> = Vec::new();
    for e in cf.walk() {
        if !e.is_stream() {
            continue;
        }
        let path = e.path().to_string_lossy().replace('\\', "/");
        if let Some(n) = path.strip_prefix("/BodyText/Section") {
            names.push((n.parse().unwrap_or(0), path.clone()));
        } else if path.starts_with("/BinData/") {
            bins.push(path.clone());
        }
    }
    names.sort();
    let mut cx = HwpCtx { tables: 0, nested: 0, images: 0 };
    let mut text = String::new();
    if !distribution {
        for (_, path) in &names {
            let mut raw = Vec::new();
            if cf.open_stream(path).and_then(|mut s| s.read_to_end(&mut raw)).is_err() {
                continue;
            }
            let buf = inflate(raw);
            let recs = records(&buf);
            let base = recs.iter().map(|r| r.level).min().unwrap_or(0);
            let mut k = 0;
            while k < recs.len() {
                if recs[k].tag == PARA_HEADER && recs[k].level == base {
                    let t = hwp_para(&recs, k, 0, &mut cx);
                    text.push_str(&t);
                    text.push('\n');
                    k = subtree_end(&recs, k);
                } else {
                    k += 1;
                }
            }
        }
    }
    for path in bins {
        let mut raw = Vec::new();
        if cf.open_stream(&path).and_then(|mut s| s.read_to_end(&mut raw)).is_ok() {
            let b = if image_mime(&raw).is_some() { raw } else { inflate(raw) };
            keep_media(&mut p.media, &path, b);
        }
    }
    if text.trim().is_empty() {
        // preview text: the first page or so, kept even in distribution documents
        let mut raw = Vec::new();
        if cf.open_stream("/PrvText").and_then(|mut s| s.read_to_end(&mut raw)).is_ok() {
            let w: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            text = String::from_utf16_lossy(&w).replace('<', "").replace('>', "");
            p.note = Some(if distribution {
                "배포용(보안) 문서라 본문이 암호화되어 있어 미리보기 글자만 읽었습니다".into()
            } else {
                "본문을 읽지 못해 미리보기 글자만 읽었습니다".into()
            });
        }
    }
    p.text = tidy(&text);
    p.tables = cx.tables;
    p.nested = cx.nested;
    p.images = cx.images.max(p.media.len() as u32);
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_nested_table_kept_inside_its_cell() {
        let xml = r#"<w:document><w:body><w:p><w:r><w:t>제목</w:t></w:r></w:p>
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>항목</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>내용</w:t></w:r></w:p></w:tc></w:tr>
            <w:tr><w:tc><w:p><w:r><w:t>세부</w:t></w:r></w:p></w:tc><w:tc>
              <w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p></w:tc></w:tr>
                     <w:tr><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
            </w:tc></w:tr></w:tbl><w:p><w:r><w:t>끝 &amp; 마무리</w:t></w:r></w:p></w:body></w:document>"#;
        let mut p = Parsed::default();
        let t = structured_xml(xml, &DOCX, &mut p);
        assert!(t.contains("항목 | 내용"), "{t}");
        assert!(t.contains("세부 | [표: A · 1 / B · 2]"), "{t}");
        assert!(t.contains("끝 & 마무리"));
        assert_eq!((p.tables, p.nested), (1, 1));
    }

    #[test]
    fn chart_numbers_are_read() {
        let xml = r#"<c:chartSpace><c:chart><c:title><c:tx><c:rich><a:p><a:r><a:t>월별 매출</a:t></a:r></a:p></c:rich></c:tx></c:title>
          <c:plotArea><c:barChart><c:ser><c:tx><c:strRef><c:strCache><c:pt><c:v>2026</c:v></c:pt></c:strCache></c:strRef></c:tx>
          <c:cat><c:strRef><c:strCache><c:pt><c:v>1월</c:v></c:pt><c:pt><c:v>2월</c:v></c:pt></c:strCache></c:strRef></c:cat>
          <c:val><c:numRef><c:numCache><c:pt><c:v>120</c:v></c:pt><c:pt><c:v>150</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#;
        let t = chart_text(xml);
        assert!(t.contains("막대") && t.contains("월별 매출") && t.contains("2026: 1월=120, 2월=150"), "{t}");
    }

    fn rec(tag: u16, level: u16, data: &[u8]) -> Vec<u8> {
        let h = (tag as u32) | ((level as u32) << 10) | ((data.len() as u32) << 20);
        let mut v = h.to_le_bytes().to_vec();
        v.extend_from_slice(data);
        v
    }
    fn utf16(s: &str) -> Vec<u8> {
        let mut v: Vec<u8> = s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        v.extend_from_slice(&13u16.to_le_bytes());
        v
    }
    fn cell(row: u16) -> Vec<u8> {
        let mut d = vec![0u8; 8];
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&row.to_le_bytes());
        d.extend_from_slice(&[0u8; 20]);
        d
    }

    #[test]
    fn binary_hwp_with_nested_table() {
        let tbl = {
            let mut d = b" lbt".to_vec();
            d.extend_from_slice(&[0u8; 8]);
            d
        };
        let table_rec = |rows: u16, cols: u16| {
            let mut d = vec![0u8; 4];
            d.extend_from_slice(&rows.to_le_bytes());
            d.extend_from_slice(&cols.to_le_bytes());
            d
        };
        let mut s = Vec::new();
        s.extend(rec(PARA_HEADER, 0, &[0; 4]));
        s.extend(rec(PARA_TEXT, 1, &utf16("혼인신고서")));
        s.extend(rec(PARA_HEADER, 0, &[0; 4]));
        s.extend(rec(CTRL_HEADER, 1, &tbl));
        s.extend(rec(TABLE, 2, &table_rec(2, 2)));
        for (row, txt) in [(0u16, "구분"), (0, "내용"), (1, "성명")] {
            s.extend(rec(LIST_HEADER, 2, &cell(row)));
            s.extend(rec(PARA_HEADER, 2, &[0; 4]));
            s.extend(rec(PARA_TEXT, 3, &utf16(txt)));
        }
        // last cell holds a table
        s.extend(rec(LIST_HEADER, 2, &cell(1)));
        s.extend(rec(PARA_HEADER, 2, &[0; 4]));
        s.extend(rec(CTRL_HEADER, 3, &tbl));
        s.extend(rec(TABLE, 4, &table_rec(1, 2)));
        for txt in ["한글", "영문"] {
            s.extend(rec(LIST_HEADER, 4, &cell(0)));
            s.extend(rec(PARA_HEADER, 4, &[0; 4]));
            s.extend(rec(PARA_TEXT, 5, &utf16(txt)));
        }
        s.extend(rec(PARA_HEADER, 0, &[0; 4]));
        s.extend(rec(PARA_TEXT, 1, &utf16("끝")));

        let mut compressed = Vec::new();
        {
            use std::io::Write;
            let mut e = flate2::write::DeflateEncoder::new(&mut compressed, flate2::Compression::default());
            e.write_all(&s).unwrap();
            e.finish().unwrap();
        }
        let mut hdr = b"HWP Document File".to_vec();
        hdr.resize(36, 0);
        hdr.extend_from_slice(&1u32.to_le_bytes());
        hdr.resize(256, 0);
        let mut cf = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
        {
            use std::io::Write;
            cf.create_stream("/FileHeader").unwrap().write_all(&hdr).unwrap();
            cf.create_storage("/BodyText").unwrap();
            cf.create_stream("/BodyText/Section0").unwrap().write_all(&compressed).unwrap();
        }
        let bytes = cf.into_inner().into_inner();
        let p = hwp(&bytes).unwrap();
        assert!(p.text.starts_with("혼인신고서"), "{}", p.text);
        assert!(p.text.contains("구분 | 내용"), "{}", p.text);
        assert!(p.text.contains("성명 | [표: 한글 · 영문]"), "{}", p.text);
        assert!(p.text.trim_end().ends_with("끝"));
        assert_eq!((p.tables, p.nested), (1, 1));
    }
}
