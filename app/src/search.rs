//! Splitting documents into passages and finding the passages that answer a question (BM25).

use crate::model::Library;
use std::collections::HashMap;

pub struct Chunk {
    pub doc: usize,
    pub ord: usize,
    pub text: String,
}

#[derive(Default)]
pub struct Index {
    pub chunks: Vec<Chunk>,
    tf: Vec<HashMap<String, u32>>,
    len: Vec<f32>,
    df: HashMap<String, u32>,
    avg: f32,
}

fn is_hangul(c: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&c)
}

pub fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in s.split(|c: char| !c.is_alphanumeric()) {
        if w.is_empty() {
            continue;
        }
        let w = w.to_lowercase();
        let chars: Vec<char> = w.chars().collect();
        if chars.len() == 1 && chars[0].is_ascii() {
            continue;
        }
        if chars.iter().any(|c| is_hangul(*c)) && chars.len() >= 2 {
            // Korean words carry endings ("계약서는", "계약서를"): pairs of letters match the stem.
            for p in chars.windows(2) {
                out.push(p.iter().collect());
            }
            if chars.len() <= 6 {
                out.push(w);
            }
        } else {
            out.push(w);
        }
    }
    out
}

pub fn split_chunks(text: &str, size: usize) -> Vec<String> {
    let size = size.clamp(200, 4000);
    let mut out = Vec::new();
    let mut cur = String::new();
    for para in text.split("\n\n").flat_map(|p| {
        // very long paragraphs are cut by lines
        if p.chars().count() > size {
            p.lines().map(|l| l.to_string()).collect::<Vec<_>>()
        } else {
            vec![p.to_string()]
        }
    }) {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        if !cur.is_empty() && cur.chars().count() + para.chars().count() > size {
            out.push(std::mem::take(&mut cur));
        }
        if para.chars().count() > size {
            // hard cut
            let chars: Vec<char> = para.chars().collect();
            for piece in chars.chunks(size) {
                out.push(piece.iter().collect());
            }
            continue;
        }
        if !cur.is_empty() {
            cur.push_str("\n\n");
        }
        cur.push_str(para);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

impl Index {
    pub fn build(lib: &Library, chunk_size: usize) -> Index {
        let mut ix = Index::default();
        for (di, d) in lib.live_docs() {
            let body = format!("{}\n\n{}", d.title, d.text);
            for (ord, c) in split_chunks(&body, chunk_size).into_iter().enumerate() {
                let toks = tokens(&c);
                let mut tf: HashMap<String, u32> = HashMap::new();
                for t in &toks {
                    *tf.entry(t.clone()).or_default() += 1;
                }
                for t in tf.keys() {
                    *ix.df.entry(t.clone()).or_default() += 1;
                }
                ix.len.push(toks.len().max(1) as f32);
                ix.tf.push(tf);
                ix.chunks.push(Chunk { doc: di, ord, text: c });
            }
        }
        let n = ix.len.len().max(1) as f32;
        ix.avg = ix.len.iter().sum::<f32>() / n;
        ix
    }

    pub fn chunks_of(&self, doc: usize) -> impl Iterator<Item = &Chunk> {
        self.chunks.iter().filter(move |c| c.doc == doc)
    }

    /// Best passages for a query: (chunk index, score), highest first.
    pub fn search(&self, query: &str, k: usize, allow: impl Fn(usize) -> bool) -> Vec<(usize, f32)> {
        let q = tokens(query);
        if q.is_empty() || self.chunks.is_empty() {
            return Vec::new();
        }
        let n = self.chunks.len() as f32;
        let (k1, b) = (1.4f32, 0.75f32);
        let mut scored: Vec<(usize, f32)> = Vec::new();
        for (i, tf) in self.tf.iter().enumerate() {
            if !allow(self.chunks[i].doc) {
                continue;
            }
            let mut s = 0.0;
            for t in &q {
                if let Some(&f) = tf.get(t) {
                    let df = *self.df.get(t).unwrap_or(&1) as f32;
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                    let f = f as f32;
                    s += idf * f * (k1 + 1.0) / (f + k1 * (1.0 - b + b * self.len[i] / self.avg.max(1.0)));
                }
            }
            if s > 0.0 {
                scored.push((i, s));
            }
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        scored
    }
}

pub fn excerpt(text: &str, max: usize) -> String {
    let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= max {
        t
    } else {
        let mut s: String = t.chars().take(max).collect();
        s.push('…');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_korean_with_endings_and_html_docx_text() {
        let lib = crate::demo::demo();
        let ix = Index::build(&lib, 600);
        let hits = ix.search("보증금이 얼마인가요", 3, |_| true);
        assert!(!hits.is_empty());
        assert_eq!(lib.docs[ix.chunks[hits[0].0].doc].title, "전세 계약 체크리스트");
        let t = crate::extract::extract(b"<html><script>x()</script><p>One</p><table><tr><td>A</td><td>B</td></tr></table></html>", "html").unwrap().text;
        assert!(t.contains("One") && t.contains("A | B") && !t.contains("x()"));
        assert!(crate::extract::extract(b"abc", "hwp").is_err());
        assert!(crate::extract::extract(b"abc", "png").is_err());
        let (_, tables, _) = crate::extract::complexity("a | b\nc | d\n\ntext");
        assert_eq!(tables, 1);
    }
}
