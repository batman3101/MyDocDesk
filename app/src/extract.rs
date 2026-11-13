//! Turning files into plain text ("knowledge compile"), and scoring how complex a document is.

use crate::docparse::{self, Parsed};
use crate::model::{new_id, now_ms, Document};
use std::path::Path;

pub const SUPPORTED: &[&str] = &[
    "txt", "md", "markdown", "csv", "tsv", "log", "json", "xml", "yaml", "yml", "ini", "toml",
    "html", "htm", "docx", "pptx", "xlsx", "hwpx", "odt", "pdf", "rs", "py", "js", "ts", "java",
    "c", "cpp", "h", "cs", "go", "sql", "sh", "rtf", "hwp", "png", "jpg", "jpeg", "webp",
];

pub const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "webp"];

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .map(|e| {
            let e = e.to_string_lossy().to_lowercase();
            SUPPORTED.contains(&e.as_str())
        })
        .unwrap_or(false)
}

fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let (t, _, _) = encoding_rs::UTF_16LE.decode(bytes);
        return t.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (t, _, _) = encoding_rs::EUC_KR.decode(bytes);
            t.into_owned()
        }
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

/// Removes tags from XML/HTML after turning the given closing tags into separators.
fn xml_to_text(xml: &str, breaks: &[(&str, &str)]) -> String {
    let mut s = xml.to_string();
    for (tag, rep) in breaks {
        s = s.replace(tag, rep);
    }
    let mut out = String::with_capacity(s.len() / 2);
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let text = unescape(&out);
    // tidy blank lines
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

fn html_to_text(html: &str) -> String {
    // drop scripts and styles
    let mut s = html.to_string();
    for tag in ["script", "style"] {
        loop {
            let lower = s.to_lowercase();
            let Some(a) = lower.find(&format!("<{}", tag)) else { break };
            let end_tag = format!("</{}>", tag);
            let b = lower[a..].find(&end_tag).map(|i| a + i + end_tag.len()).unwrap_or(s.len());
            s.replace_range(a..b, "");
        }
    }
    xml_to_text(
        &s,
        &[
            ("</p>", "\n"), ("<br>", "\n"), ("<br/>", "\n"), ("<br />", "\n"), ("</div>", "\n"),
            ("</li>", "\n"), ("</tr>", "\n"), ("</td>", " | "), ("</th>", " | "),
            ("</h1>", "\n"), ("</h2>", "\n"), ("</h3>", "\n"), ("<li>", "- "),
        ],
    )
}

fn pdf_text(bytes: &[u8]) -> Result<String, String> {
    let owned = bytes.to_vec();
    let r = std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&owned));
    match r {
        Ok(Ok(t)) => Ok(t),
        Ok(Err(e)) => Err(format!("PDF를 읽지 못했습니다: {e}")),
        Err(_) => Err("PDF 구조가 특이해서 글자를 꺼내지 못했습니다 (스캔 PDF일 수 있음)".into()),
    }
}

/// Reads a file's bytes into text plus structure facts (tables, tables inside tables,
/// pictures, charts) and the pictures worth describing with AI.
pub fn extract(bytes: &[u8], ext: &str) -> Result<Parsed, String> {
    let ext = ext.to_lowercase();
    let mut p = match ext.as_str() {
        "html" | "htm" => Parsed { text: html_to_text(&decode_text(bytes)), ..Default::default() },
        "docx" | "pptx" | "xlsx" | "hwpx" | "odt" => docparse::office(bytes, &ext)?,
        "hwp" => docparse::hwp(bytes)?,
        "pdf" => Parsed { text: pdf_text(bytes)?, ..Default::default() },
        e if IMAGE_EXT.contains(&e) => {
            let mime = docparse::image_mime(bytes).ok_or("그림 형식을 알아보지 못했습니다")?;
            return Ok(Parsed { images: 1, media: vec![docparse::Media { name: "image".into(), mime, bytes: bytes.to_vec() }], ..Default::default() });
        }
        _ => Parsed { text: decode_text(bytes), ..Default::default() },
    };
    if p.text.trim().is_empty() && p.media.is_empty() {
        return Err("문서에서 글자를 찾지 못했습니다 (그림만 있는 문서일 수 있음 — 설정에서 'AI 그림 해석'을 켜 보세요)".into());
    }
    if p.text.trim().is_empty() {
        p.note = Some("글자가 없어 그림 해석에만 의존합니다".into());
    }
    Ok(p)
}

/// 0..100: longer documents, tables and deep headings make a document harder to process.
pub fn complexity(text: &str) -> (u8, u32, u32) {
    let mut tables = 0u32;
    let mut headings = 0u32;
    let mut run = 0u32;
    for l in text.lines() {
        let t = l.trim();
        // a table is two or more rows in a row with cell separators
        let is_row = t.contains(" | ") || t.contains('\t');
        run = if is_row { run + 1 } else { 0 };
        if run == 2 {
            tables += 1;
        }
        if t.starts_with('#')
            || (t.len() < 60 && (t.starts_with("제") && t.contains('조')))
            || t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) && t.len() < 50 && t.contains(". ")
        {
            headings += 1;
        }
    }
    (score(text.chars().count(), tables, 0, headings, 0, 0), tables, headings)
}

/// 0..100 from length, tables, tables inside tables, headings, pictures and charts.
pub fn score(chars: usize, tables: u32, nested: u32, headings: u32, images: u32, charts: u32) -> u8 {
    let len_score = (chars as f32 / 20000.0 * 30.0).min(30.0);
    let table_score = (tables as f32 * 10.0).min(25.0);
    let nested_score = (nested as f32 * 10.0).min(20.0);
    let head_score = (headings as f32 * 2.0).min(15.0);
    let pic_score = (images as f32 * 2.0 + charts as f32 * 5.0).min(15.0);
    (len_score + table_score + nested_score + head_score + pic_score).round().min(100.0) as u8
}

pub fn complexity_label(c: u8) -> &'static str {
    match c {
        0..=29 => "쉬움",
        30..=59 => "보통",
        _ => "복잡",
    }
}

pub fn kind_label(ext: &str) -> String {
    match ext {
        "md" | "markdown" => "Markdown".into(),
        "txt" | "log" => "텍스트".into(),
        "docx" => "Word".into(),
        "pptx" => "PowerPoint".into(),
        "xlsx" => "Excel".into(),
        "hwpx" | "hwp" => "한글".into(),
        "png" | "jpg" | "jpeg" | "webp" => "그림".into(),
        "pdf" => "PDF".into(),
        "html" | "htm" => "웹 문서".into(),
        "csv" | "tsv" => "표(CSV)".into(),
        "memo" => "메모".into(),
        e => e.to_uppercase(),
    }
}

/// Reads one file into a new document. The caller copies the original file and may
/// add AI descriptions of the returned pictures.
pub fn document_from_file(path: &Path) -> Result<(Document, Vec<u8>, Vec<docparse::Media>), String> {
    let size = std::fs::metadata(path).map_err(|e| format!("파일을 열 수 없습니다: {e}"))?.len();
    if size > MAX_FILE_BYTES {
        return Err(format!("파일이 너무 큽니다 ({} MB 넘음)", MAX_FILE_BYTES / 1024 / 1024));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("파일을 열 수 없습니다: {e}"))?;
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let p = extract(&bytes, &ext)?;
    let file_name = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    let title = path.file_stem().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| file_name.clone());
    let mut d = make_doc(title, file_name, &ext, p.text, bytes.len() as u64, true);
    d.tables = d.tables.max(p.tables);
    d.nested_tables = p.nested;
    d.images = p.images;
    d.charts = p.charts;
    d.parse_note = p.note.unwrap_or_default();
    d.source_path = path.display().to_string();
    d.source_mtime = file_mtime(path);
    d.file_sha = crate::model::sha256_hex(&bytes);
    rescore(&mut d);
    Ok((d, bytes, p.media))
}

pub fn file_mtime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn rescore(d: &mut Document) {
    d.complexity = score(d.text.chars().count(), d.tables, d.nested_tables, d.headings, d.images, d.charts);
}

pub fn make_doc(title: String, file_name: String, ext: &str, text: String, size: u64, has_file: bool) -> Document {
    let (c, tables, headings) = complexity(&text);
    let now = now_ms();
    Document {
        id: new_id(),
        title,
        file_name,
        kind: kind_label(ext),
        text,
        tags: Vec::new(),
        added_ms: now,
        updated_ms: now,
        size_bytes: size,
        complexity: c,
        tables,
        headings,
        has_file,
        deleted: false,
        ..Default::default()
    }
}

/// At most this many files are collected from the folders given at once.
pub const MAX_FILES: usize = 2000;
/// Files bigger than this are not read.
pub const MAX_FILE_BYTES: u64 = 200 * 1024 * 1024;

/// Every supported file under a folder (depth-limited).
pub fn collect_folder(dir: &Path, out: &mut Vec<std::path::PathBuf>, depth: usize) {
    if depth > 6 || out.len() >= MAX_FILES {
        return;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if out.len() >= MAX_FILES {
                return;
            }
            let p = e.path();
            if p.is_dir() {
                let hidden = p.file_name().map(|n| n.to_string_lossy().starts_with('.')).unwrap_or(true);
                if !hidden {
                    collect_folder(&p, out, depth + 1);
                }
            } else if is_supported(&p) {
                out.push(p);
            }
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    /// audit M-10: the 2000-file limit holds inside a single folder too.
    #[test]
    fn collecting_stops_at_the_file_limit() {
        let root = std::env::temp_dir().join(format!("mdd-cap-{}", new_id()));
        std::fs::create_dir_all(&root).unwrap();
        for i in 0..(MAX_FILES + 5) {
            std::fs::write(root.join(format!("{i:04}.txt")), b"x").unwrap();
        }
        let mut found = Vec::new();
        collect_folder(&root, &mut found, 0);
        assert_eq!(found.len(), MAX_FILES);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// audit M-10: a file bigger than the limit is refused before it is read into memory.
    #[test]
    fn oversized_file_is_refused() {
        let root = std::env::temp_dir().join(format!("mdd-big-{}", new_id()));
        std::fs::create_dir_all(&root).unwrap();
        let p = root.join("big.txt");
        std::fs::File::create(&p).unwrap().set_len(MAX_FILE_BYTES + 1).unwrap();
        let err = document_from_file(&p).err().expect("refused");
        assert!(err.contains("너무 큽니다"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
