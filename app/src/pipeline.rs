//! Auto-collect: watched folders → read → split → index, handling only new, changed and
//! removed files (compared by modified time and size), with a run history.

use crate::extract;
use crate::model::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Instant;

pub const STAGES: [(&str, &str); 4] = [("수집", "연결 폴더에서 파일 찾기"), ("해석", "형식별로 글자·표 읽기"), ("조각", "검색용 조각으로 나누기"), ("색인", "질문하기에 바로 반영")];

#[derive(Clone)]
pub struct Known {
    pub id: String,
    pub path: String,
    pub mtime: i64,
    pub size: u64,
}

pub struct ScanResult {
    pub added: Vec<Document>,
    pub updated: Vec<Document>,
    pub removed: Vec<String>,
    pub run: PipelineRun,
}

pub enum Msg {
    Progress(String),
    Done(Box<ScanResult>),
}

pub fn under(path: &str, folder: &str) -> bool {
    let p = path.replace('\\', "/").to_lowercase();
    let mut f = folder.replace('\\', "/").to_lowercase();
    if !f.ends_with('/') {
        f.push('/');
    }
    p.starts_with(&f)
}

pub fn scan(folders: Vec<String>, known: Vec<Known>, store: Store, tx: Option<Sender<Msg>>) -> ScanResult {
    let started = Instant::now();
    let say = |m: String| {
        if let Some(t) = &tx {
            let _ = t.send(Msg::Progress(m));
        }
    };
    let by_path: HashMap<String, Known> = known.iter().map(|k| (k.path.to_lowercase(), k.clone())).collect();
    let mut run = PipelineRun { ms: now_ms(), folders: folders.len(), ..Default::default() };
    let (mut added, mut updated, mut removed) = (Vec::new(), Vec::new(), Vec::new());
    let mut seen: HashSet<String> = HashSet::new();
    for folder in &folders {
        let dir = Path::new(folder);
        if !dir.is_dir() {
            run.errors.push(format!("폴더를 찾을 수 없습니다: {folder}"));
            continue;
        }
        say(format!("수집: {folder}"));
        let mut files: Vec<PathBuf> = Vec::new();
        extract::collect_folder(dir, &mut files, 0);
        run.scanned += files.len();
        for (i, f) in files.iter().enumerate() {
            let key = f.display().to_string();
            seen.insert(key.to_lowercase());
            let size = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
            let mtime = extract::file_mtime(f);
            let prev = by_path.get(&key.to_lowercase());
            if let Some(k) = prev {
                if k.mtime == mtime && k.size == size {
                    continue;
                }
            }
            say(format!("해석 {}/{}: {}", i + 1, files.len(), f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
            match extract::document_from_file(f) {
                Ok((mut d, bytes, _media)) => {
                    if let Some(k) = prev {
                        d.id = k.id.clone();
                    }
                    if !store.read_only {
                        if let Err(e) = std::fs::write(store.file_path(&d), &bytes) {
                            // without its original the document isn't added: it's tried again next scan
                            run.failed += 1;
                            if run.errors.len() < 20 {
                                run.errors.push(format!("{}: 원본 복사 실패: {e}", f.display()));
                            }
                            continue;
                        }
                    }
                    if prev.is_some() {
                        updated.push(d);
                    } else {
                        added.push(d);
                    }
                }
                Err(e) => {
                    run.failed += 1;
                    if run.errors.len() < 20 {
                        run.errors.push(format!("{}: {e}", f.display()));
                    }
                }
            }
        }
    }
    // files that disappeared from a watched folder that still exists. Not being listed isn't
    // enough (a sub-folder may be unreadable, skipped or over the limit): the file itself must be gone.
    for k in &known {
        let in_live_folder = folders.iter().any(|f| under(&k.path, f) && Path::new(f).is_dir());
        let gone = matches!(std::fs::metadata(&k.path), Err(ref e) if e.kind() == std::io::ErrorKind::NotFound);
        if in_live_folder && !seen.contains(&k.path.to_lowercase()) && gone {
            removed.push(k.id.clone());
        }
    }
    run.added = added.len();
    run.updated = updated.len();
    run.removed = removed.len();
    run.seconds = started.elapsed().as_secs_f32();
    say("색인: 질문하기에 반영하는 중".into());
    ScanResult { added, updated, removed, run }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_new_changed_and_removed_files_are_processed() {
        let root = std::env::temp_dir().join(format!("mdd-pipe-{}", new_id()));
        let docs = root.join("docs");
        std::fs::create_dir_all(docs.join("sub")).unwrap();
        let store_dir = root.join("store");
        std::fs::create_dir_all(store_dir.join("files")).unwrap();
        let store = Store::at(store_dir);
        std::fs::write(docs.join("a.txt"), "첫 문서 내용").unwrap();
        std::fs::write(docs.join("sub").join("b.md"), "# 둘째\n내용").unwrap();
        let folder = docs.display().to_string();

        let r1 = scan(vec![folder.clone()], vec![], store.clone(), None);
        assert_eq!((r1.run.added, r1.run.updated, r1.run.removed), (2, 0, 0));
        let known: Vec<Known> = r1.added.iter().map(|d| Known { id: d.id.clone(), path: d.source_path.clone(), mtime: d.source_mtime, size: d.size_bytes }).collect();

        // nothing changed: nothing processed
        let r2 = scan(vec![folder.clone()], known.clone(), store.clone(), None);
        assert_eq!((r2.run.added, r2.run.updated, r2.run.removed), (0, 0, 0));

        // one changed, one removed, one new
        std::fs::write(docs.join("a.txt"), "첫 문서 내용이 바뀌었습니다").unwrap();
        std::fs::remove_file(docs.join("sub").join("b.md")).unwrap();
        std::fs::write(docs.join("c.txt"), "새 문서").unwrap();
        let r3 = scan(vec![folder], known, store, None);
        assert_eq!((r3.run.added, r3.run.updated, r3.run.removed), (1, 1, 1));
        assert!(r3.updated[0].text.contains("바뀌었습니다"));
        let _ = std::fs::remove_dir_all(&root);
    }

    fn temp(name: &str) -> (std::path::PathBuf, std::path::PathBuf, Store) {
        let root = std::env::temp_dir().join(format!("mdd-pipe-{name}-{}", new_id()));
        let docs = root.join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::create_dir_all(root.join("store").join("files")).unwrap();
        (root.clone(), docs, Store::at(root.join("store")))
    }
    fn known_of(d: &Document) -> Known {
        Known { id: d.id.clone(), path: d.source_path.clone(), mtime: d.source_mtime, size: d.size_bytes }
    }

    /// audit H-04: a file the scan didn't list (skipped, unreadable sub-folder, over a limit)
    /// is only "removed" when it is really gone.
    #[test]
    fn files_not_listed_but_still_there_are_not_removed() {
        let (root, docs, store) = temp("partial");
        std::fs::create_dir_all(docs.join(".private")).unwrap(); // the scan skips hidden folders
        let deep = docs.join("1").join("2").join("3").join("4").join("5").join("6").join("7").join("8");
        std::fs::create_dir_all(&deep).unwrap(); // and goes only so deep
        let hidden = docs.join(".private").join("a.txt");
        let too_deep = deep.join("b.txt");
        let gone = docs.join("gone.txt");
        for p in [&hidden, &too_deep, &gone] {
            std::fs::write(p, "내용").unwrap();
        }
        let known: Vec<Known> = [&hidden, &too_deep, &gone]
            .iter()
            .map(|p| known_of(&crate::extract::document_from_file(p).unwrap().0))
            .collect();
        std::fs::remove_file(&gone).unwrap();
        let r = scan(vec![docs.display().to_string()], known.clone(), store, None);
        assert_eq!(r.removed, vec![known[2].id.clone()], "only the file that is really gone");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// audit M-02: when the original can't be copied, the document is a failure, not an addition.
    #[test]
    fn original_copy_failure_is_counted_as_failure() {
        let (root, docs, _) = temp("copyfail");
        std::fs::write(docs.join("a.txt"), "내용").unwrap();
        let store = Store::at(root.join("store-without-files"));
        let r = scan(vec![docs.display().to_string()], vec![], store, None);
        assert_eq!(r.run.added, 0);
        assert!(r.added.is_empty());
        assert_eq!(r.run.failed, 1);
        assert!(!r.run.errors.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
