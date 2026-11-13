//! Account sync between this computer and others through a cloud-synced folder.
//!
//! The account is a name plus a password. Everything written to the shared folder is
//! encrypted with a key made from both, so the cloud service only ever sees scrambled files.

use crate::model::{now_ms, os_name, DeviceInfo, Library, Store};
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

const CHECK_TEXT: &[u8] = b"mydocdesk-account-ok";

#[derive(Clone)]
pub struct Account {
    pub id: String,
    key: [u8; 32],
    pub dir: PathBuf,
}

pub struct SyncReport {
    pub pulled_docs: usize,
    pub pushed_files: usize,
    pub pulled_files: usize,
    pub total_docs: usize,
    pub new_account: bool,
    /// Shared files that couldn't be read (damaged, half-synced): skipped, not fatal.
    pub unreadable: usize,
    /// Originals of deleted documents that couldn't be removed from the shared folder (tried again next time).
    pub cleanup_failed: usize,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

fn encrypt(key: &[u8; 32], plain: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let mut out = nonce.to_vec();
    out.extend(cipher.encrypt(&nonce, plain).map_err(|_| crate::i18n::tr("암호화 실패").to_string())?);
    Ok(out)
}

fn decrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 24 {
        return Err(crate::i18n::tr("동기화 파일이 손상되었습니다").into());
    }
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XNonce::from_slice(&data[..24]);
    cipher.decrypt(nonce, &data[24..]).map_err(|_| crate::i18n::tr("암호가 맞지 않거나 파일이 손상되었습니다").to_string())
}

/// Opens (or creates) the account inside the cloud folder and checks the password.
pub fn unlock(sync_folder: &str, account_id: &str, password: &str) -> Result<(Account, bool), String> {
    let account_id = account_id.trim();
    if account_id.is_empty() || password.is_empty() {
        return Err(crate::i18n::tr("계정 이름과 암호를 모두 입력해 주세요").into());
    }
    if password.chars().count() < 6 {
        return Err(crate::i18n::tr("암호는 6자 이상으로 정해 주세요").into());
    }
    let root = PathBuf::from(sync_folder.trim());
    if sync_folder.trim().is_empty() || !root.is_dir() {
        return Err(crate::i18n::tr("동기화 폴더를 찾을 수 없습니다. 구글 드라이브·OneDrive·Dropbox·iCloud 폴더 안을 골라 주세요").into());
    }
    let mut key = [0u8; 32];
    let salt = format!("mydocdesk:{}", account_id.to_lowercase());
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), 120_000, &mut key);
    let name_hash = hex(&Sha256::digest(account_id.to_lowercase().as_bytes())[..8]);
    let dir = root.join("MyDocDesk-sync").join(name_hash);
    std::fs::create_dir_all(dir.join("files")).map_err(|e| crate::ft!("동기화 폴더에 쓸 수 없습니다: {e}", "Không ghi được vào thư mục đồng bộ: {e}", e = e))?;
    let check = dir.join("account.check");
    let mut new_account = false;
    if check.exists() {
        let data = std::fs::read(&check).map_err(|e| e.to_string())?;
        let plain = decrypt(&key, &data).map_err(|_| crate::i18n::tr("암호가 맞지 않습니다. 다른 기기에서 정한 암호를 입력해 주세요").to_string())?;
        if plain != CHECK_TEXT {
            return Err(crate::i18n::tr("암호가 맞지 않습니다").into());
        }
    } else {
        std::fs::write(&check, encrypt(&key, CHECK_TEXT)?).map_err(|e| e.to_string())?;
        new_account = true;
    }
    Ok((Account { id: account_id.to_string(), key, dir }, new_account))
}

/// Newer wins. Two different versions saved in the same millisecond are settled the same way
/// on every device (the one whose JSON sorts higher), so devices never stay apart.
fn merge_by_id<T: Clone + serde::Serialize>(local: &mut Vec<T>, remote: Vec<T>, id: impl Fn(&T) -> String, ts: impl Fn(&T) -> i64) -> usize {
    let mut pos: HashMap<String, usize> = local.iter().enumerate().map(|(i, x)| (id(x), i)).collect();
    let mut changed = 0;
    for r in remote {
        match pos.get(&id(&r)) {
            Some(&i) => {
                let (rt, lt) = (ts(&r), ts(&local[i]));
                let take = rt > lt || (rt == lt && serde_json::to_string(&r).unwrap_or_default() > serde_json::to_string(&local[i]).unwrap_or_default());
                if take {
                    local[i] = r;
                    changed += 1;
                }
            }
            None => {
                pos.insert(id(&r), local.len());
                local.push(r);
                changed += 1;
            }
        }
    }
    changed
}

pub fn merge(local: &mut Library, mut remote: Library) -> usize {
    // ids become file names: documents with anything but a plain id are refused
    remote.docs.retain(|d| crate::model::is_safe_id(&d.id));
    let pulled = merge_by_id(&mut local.docs, remote.docs, |d| d.id.clone(), |d| d.updated_ms);
    merge_by_id(&mut local.agents, remote.agents, |a| a.id.clone(), |a| a.updated_ms);
    merge_by_id(&mut local.evals, remote.evals, |e| e.id.clone(), |e| e.updated_ms);
    merge_by_id(&mut local.chats, remote.chats, |c| c.id.clone(), |c| c.ms);
    merge_by_id(&mut local.usage, remote.usage, |u| u.id.clone(), |u| u.ms);
    merge_by_id(&mut local.devices, remote.devices, |d| d.name.clone(), |d| d.last_sync_ms);
    local.chats.sort_by_key(|c| c.ms);
    local.usage.sort_by_key(|u| u.ms);
    local.chats_cleared_ms = local.chats_cleared_ms.max(remote.chats_cleared_ms);
    pulled
}

/// The part of this device's library file name: its id, or (no id yet) a hash of its name.
fn device_file_key(device: &str, device_id: &str) -> String {
    if crate::model::is_safe_id(device_id) {
        device_id.to_string()
    } else {
        crate::model::sha256_hex(device.as_bytes())[..16].to_string()
    }
}

/// The shared-folder name of a document's original (per version when its sha is known).
fn remote_file_name(d: &crate::model::Document) -> String {
    if crate::model::is_sha(&d.file_sha) {
        format!("{}-{}.enc", d.id, &d.file_sha[..16].to_lowercase())
    } else {
        format!("{}.enc", d.id)
    }
}

/// `<id>.enc` or `<id>-<sha16>.enc`.
fn is_version_of(name: &str, id: &str) -> bool {
    name == format!("{id}.enc") || (name.starts_with(&format!("{id}-")) && name.ends_with(".enc") && name.len() == id.len() + 1 + 16 + 4)
}

/// Two-way sync: read the account's library, merge, write it back, then exchange original files.
pub fn sync(acc: &Account, lib: &mut Library, store: &Store, device: &str, device_id: &str) -> Result<SyncReport, String> {
    // Every device writes only its own `library-<device>.enc` and reads all of them (and the
    // single `library.enc` the earlier version wrote). Cloud folders don't lock files, so one
    // shared file written by two devices ends in conflicts or half-written data.
    let own = format!("library-{}.enc", device_file_key(device, device_id));
    let mut names: Vec<String> = std::fs::read_dir(&acc.dir)
        .map(|rd| rd.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().to_string()).collect())
        .unwrap_or_default();
    names.retain(|n| n.starts_with("library") && n.ends_with(".enc"));
    names.sort();
    let new_account = names.is_empty();
    let (mut pulled_docs, mut unreadable) = (0, 0);
    for n in &names {
        let read = std::fs::read(acc.dir.join(n))
            .map_err(|e| e.to_string())
            .and_then(|data| decrypt(&acc.key, &data))
            .and_then(|plain| serde_json::from_slice::<Library>(&plain).map_err(|e| e.to_string()));
        match read {
            Ok(remote) => pulled_docs += merge(lib, remote),
            Err(_) => unreadable += 1, // damaged or still syncing: skipped this time, not fatal
        }
    }
    // this device
    let now = now_ms();
    match lib.devices.iter_mut().find(|d| d.name == device) {
        Some(d) => {
            d.last_sync_ms = now;
            d.os = os_name().into();
        }
        None => lib.devices.push(DeviceInfo { name: device.into(), os: os_name().into(), last_sync_ms: now }),
    }
    let plain = serde_json::to_vec(lib).map_err(|e| e.to_string())?;
    let tmp = acc.dir.join(format!("{own}.tmp"));
    std::fs::write(&tmp, encrypt(&acc.key, &plain)?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, acc.dir.join(&own)).map_err(|e| e.to_string())?;

    // original files: stored per version (`<id>-<first 16 of sha>.enc`), so a changed original
    // is a new file that every device fetches; documents from before the sha was kept use `<id>.enc`
    let (mut pushed_files, mut pulled_files) = (0, 0);
    let files_dir = acc.dir.join("files");
    let mut remote_names: Vec<String> = std::fs::read_dir(&files_dir).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
    for d in lib.docs.iter().filter(|d| d.has_file && !d.deleted && crate::model::is_safe_id(&d.id)) {
        let want = crate::model::is_sha(&d.file_sha).then(|| d.file_sha.to_lowercase());
        let name = remote_file_name(d);
        let local = store.file_path(d);
        let remote = files_dir.join(&name);
        let local_current = local.exists()
            && match &want {
                Some(sha) => std::fs::read(&local).map(|b| &crate::model::sha256_hex(&b) == sha).unwrap_or(false),
                None => true,
            };
        if local_current {
            if !remote.exists() {
                let bytes = std::fs::read(&local).map_err(|e| e.to_string())?;
                let tmp = files_dir.join(format!("{name}.tmp"));
                std::fs::write(&tmp, encrypt(&acc.key, &bytes)?).map_err(|e| e.to_string())?;
                std::fs::rename(&tmp, &remote).map_err(|e| e.to_string())?;
                remote_names.push(name.clone());
                pushed_files += 1;
            }
            // older versions of this original are no longer needed
            for old in remote_names.iter().filter(|n| **n != name && is_version_of(n, &d.id)) {
                let _ = std::fs::remove_file(files_dir.join(old));
            }
        } else if remote.exists() {
            let data = std::fs::read(&remote).map_err(|e| e.to_string())?;
            let bytes = decrypt(&acc.key, &data)?;
            if want.as_ref().is_some_and(|sha| &crate::model::sha256_hex(&bytes) != sha) {
                continue; // not the version the library describes: leave the local file alone
            }
            let tmp = local.with_extension("part");
            std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &local).map_err(|e| e.to_string())?;
            pulled_files += 1;
        }
    }
    // files of deleted documents are removed from the shared folder (every version); what can't
    // be removed is counted and tried again on the next sync
    let mut cleanup_failed = 0;
    for d in lib.docs.iter().filter(|d| d.deleted && crate::model::is_safe_id(&d.id)) {
        for old in remote_names.iter().filter(|n| is_version_of(n, &d.id)) {
            match std::fs::remove_file(files_dir.join(old)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => cleanup_failed += 1,
            }
        }
    }
    Ok(SyncReport {
        pulled_docs,
        pushed_files,
        pulled_files,
        total_docs: lib.live_docs().count(),
        new_account,
        unreadable,
        cleanup_failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Library, Store};

    #[test]
    fn two_devices_share_one_account() {
        let root = std::env::temp_dir().join(format!("mdd-sync-test-{}", crate::model::new_id()));
        let cloud = root.join("cloud");
        std::fs::create_dir_all(&cloud).unwrap();
        let mk_store = |n: &str| {
            let dir = root.join(n);
            std::fs::create_dir_all(dir.join("files")).unwrap();
            Store::at(dir)
        };
        let (sa, sb) = (mk_store("pc"), mk_store("mac"));
        // PC adds a document with an original file
        let mut a = Library::default();
        let mut d = crate::extract::make_doc("계약서".into(), "계약서.txt".into(), "txt", "보증금 2억".into(), 10, true);
        std::fs::write(sa.file_path(&d), b"original bytes").unwrap();
        d.tags.push("계약".into());
        a.docs.push(d.clone());
        let (acc_a, new) = unlock(cloud.to_str().unwrap(), "me", "secret123").unwrap();
        assert!(new);
        sync(&acc_a, &mut a, &sa, "PC", "PC").unwrap();
        // wrong password is refused
        assert!(unlock(cloud.to_str().unwrap(), "me", "wrong-pass").is_err());
        // Mac connects with the same account and receives the document and file
        let mut b = Library::default();
        let (acc_b, new_b) = unlock(cloud.to_str().unwrap(), "me", "secret123").unwrap();
        assert!(!new_b);
        let r = sync(&acc_b, &mut b, &sb, "Mac", "Mac").unwrap();
        assert_eq!(r.total_docs, 1);
        assert_eq!(std::fs::read(sb.file_path(&d)).unwrap(), b"original bytes");
        // Mac edits, PC gets the edit
        b.docs[0].title = "계약서 (수정)".into();
        b.docs[0].updated_ms += 10;
        sync(&acc_b, &mut b, &sb, "Mac", "Mac").unwrap();
        sync(&acc_a, &mut a, &sa, "PC", "PC").unwrap();
        assert_eq!(a.docs[0].title, "계약서 (수정)");
        assert_eq!(a.devices.len(), 2);
        // the cloud folder holds no readable text
        for e in std::fs::read_dir(&acc_a.dir).unwrap().flatten() {
            if e.path().is_file() {
                let raw = std::fs::read(e.path()).unwrap();
                assert!(!String::from_utf8_lossy(&raw).contains("보증금"));
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    fn temp_root(name: &str) -> std::path::PathBuf {
        let r = std::env::temp_dir().join(format!("mdd-sync-{name}-{}", crate::model::new_id()));
        std::fs::create_dir_all(r.join("cloud")).unwrap();
        r
    }
    fn temp_store(root: &std::path::Path, n: &str) -> Store {
        let dir = root.join(n);
        std::fs::create_dir_all(dir.join("files")).unwrap();
        Store::at(dir)
    }
    fn all_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() { all_files(&p, out) } else { out.push(p) }
        }
    }

    /// audit H-06: an account member can't make this device write or delete outside its files folder.
    #[test]
    fn remote_ids_cannot_leave_the_files_folder() {
        let r = temp_root("traversal");
        let local = temp_store(&r, "local");
        let (acc, _) = unlock(r.join("cloud").to_str().unwrap(), "audit", "strong-enough").unwrap();
        let bad = ["../escaped", "..\\escaped2", "C:\\Windows\\evil", "\\\\server\\share\\x", "a/b", ""];
        let mut docs: Vec<crate::model::Document> = bad
            .iter()
            .map(|id| crate::model::Document { id: id.to_string(), file_name: "doc.txt".into(), has_file: true, updated_ms: 1, ..Default::default() })
            .collect();
        // and a deleted one aimed at a file outside the account's files folder
        std::fs::write(acc.dir.join("victim.enc"), b"keep me").unwrap();
        docs.push(crate::model::Document { id: "../victim".into(), deleted: true, updated_ms: 1, ..Default::default() });
        let lib_bytes = serde_json::to_vec(&Library { docs, ..Default::default() }).unwrap();
        std::fs::write(acc.dir.join("library.enc"), encrypt(&acc.key, &lib_bytes).unwrap()).unwrap();
        std::fs::write(acc.dir.join("escaped.enc"), encrypt(&acc.key, b"payload").unwrap()).unwrap();
        let mut got = Library::default();
        sync(&acc, &mut got, &local, "victim", "victim").unwrap();
        assert!(got.docs.is_empty(), "documents with unsafe ids are not taken in");
        let mut files = Vec::new();
        all_files(&local.dir, &mut files);
        assert!(files.is_empty(), "nothing written on this device: {files:?}");
        assert_eq!(std::fs::read(acc.dir.join("victim.enc")).unwrap(), b"keep me", "nothing deleted outside");
        let _ = std::fs::remove_dir_all(r);
    }

    /// audit M-01: two devices that edited the same document in the same millisecond agree.
    #[test]
    fn same_time_edits_end_up_the_same_on_both_devices() {
        let mk = |title: &str| crate::model::Document { id: "same".into(), title: title.into(), updated_ms: 1000, ..Default::default() };
        let mut a = Library { docs: vec![mk("A")], ..Default::default() };
        let mut b = Library { docs: vec![mk("B")], ..Default::default() };
        merge(&mut a, Library { docs: vec![mk("B")], ..Default::default() });
        merge(&mut b, Library { docs: vec![mk("A")], ..Default::default() });
        assert_eq!(a.docs[0].title, b.docs[0].title);
    }

    /// audit H-03: a changed original (same document) reaches devices that had the old one, and new ones.
    #[test]
    fn changed_original_reaches_the_other_devices() {
        let r = temp_root("stale");
        let (sa, sb, sc) = (temp_store(&r, "a"), temp_store(&r, "b"), temp_store(&r, "c"));
        let (acc, _) = unlock(r.join("cloud").to_str().unwrap(), "audit", "strong-enough").unwrap();
        let mut a = Library::default();
        let mut d = crate::extract::make_doc("doc".into(), "doc.txt".into(), "txt", "v1 text".into(), 2, true);
        d.file_sha = crate::model::sha256_hex(b"v1");
        std::fs::write(sa.file_path(&d), b"v1").unwrap();
        a.docs.push(d.clone());
        sync(&acc, &mut a, &sa, "A", "A").unwrap();
        let mut b = Library::default();
        sync(&acc, &mut b, &sb, "B", "B").unwrap();
        assert_eq!(std::fs::read(sb.file_path(&d)).unwrap(), b"v1");

        // A gets a new version of the same file (like auto-collect does)
        std::fs::write(sa.file_path(&d), b"v2").unwrap();
        a.docs[0].text = "v2 text".into();
        a.docs[0].file_sha = crate::model::sha256_hex(b"v2");
        a.docs[0].updated_ms += 1;
        sync(&acc, &mut a, &sa, "A", "A").unwrap();

        sync(&acc, &mut b, &sb, "B", "B").unwrap();
        assert_eq!(b.docs[0].text, "v2 text");
        assert_eq!(std::fs::read(sb.file_path(&d)).unwrap(), b"v2", "the device with v1 gets v2");
        let mut c = Library::default();
        sync(&acc, &mut c, &sc, "C", "C").unwrap();
        assert_eq!(std::fs::read(sc.file_path(&d)).unwrap(), b"v2", "a new device gets v2");
        let _ = std::fs::remove_dir_all(r);
    }

    fn doc(title: &str) -> crate::model::Document {
        crate::extract::make_doc(title.into(), format!("{title}.txt"), "txt", format!("{title} text"), 1, false)
    }
    fn library_files(acc: &Account) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(&acc.dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.starts_with("library") && n.ends_with(".enc")).collect();
        v.sort();
        v
    }

    /// Cloud folders don't lock files: two devices writing one shared file at the same time end
    /// in conflicts or a half-written file. Each device writes only its own file.
    #[test]
    fn each_device_writes_only_its_own_file() {
        let r = temp_root("own-file");
        let (sa, sb) = (temp_store(&r, "a"), temp_store(&r, "b"));
        let (acc, _) = unlock(r.join("cloud").to_str().unwrap(), "me", "secret123").unwrap();
        let mut a = Library { docs: vec![doc("from-a")], ..Default::default() };
        let mut b = Library { docs: vec![doc("from-b")], ..Default::default() };
        sync(&acc, &mut a, &sa, "PC", "dev-a").unwrap();
        let a_files = library_files(&acc);
        assert_eq!(a_files.len(), 1);
        let a_bytes = std::fs::read(acc.dir.join(&a_files[0])).unwrap();
        sync(&acc, &mut b, &sb, "Mac", "dev-b").unwrap();
        assert_eq!(library_files(&acc).len(), 2, "one file per device");
        assert_eq!(std::fs::read(acc.dir.join(&a_files[0])).unwrap(), a_bytes, "B never writes A's file");
        assert!(b.docs.iter().any(|d| d.title == "from-a"));
        sync(&acc, &mut a, &sa, "PC", "dev-a").unwrap();
        assert!(a.docs.iter().any(|d| d.title == "from-b"));
        let _ = std::fs::remove_dir_all(r);
    }

    /// A damaged or half-synced shared file is skipped (and reported); it doesn't stop every device's sync.
    #[test]
    fn a_damaged_shared_file_does_not_stop_sync() {
        let r = temp_root("damaged");
        let (sa, sb) = (temp_store(&r, "a"), temp_store(&r, "b"));
        let (acc, _) = unlock(r.join("cloud").to_str().unwrap(), "me", "secret123").unwrap();
        let mut b = Library { docs: vec![doc("from-b")], ..Default::default() };
        sync(&acc, &mut b, &sb, "Mac", "dev-b").unwrap();
        std::fs::write(acc.dir.join("library.enc"), b"half written garbage that is not encrypted data").unwrap();
        let mut a = Library::default();
        let rep = sync(&acc, &mut a, &sa, "PC", "dev-a").expect("sync goes on");
        assert_eq!(rep.unreadable, 1);
        assert!(a.docs.iter().any(|d| d.title == "from-b"), "the readable files are still merged");
        let _ = std::fs::remove_dir_all(r);
    }

    /// Libraries written by the earlier version (one shared library.enc) are still read.
    #[test]
    fn the_old_shared_library_file_is_still_read() {
        let r = temp_root("legacy");
        let sa = temp_store(&r, "a");
        let (acc, _) = unlock(r.join("cloud").to_str().unwrap(), "me", "secret123").unwrap();
        let old = Library { docs: vec![doc("old-version")], ..Default::default() };
        std::fs::write(acc.dir.join("library.enc"), encrypt(&acc.key, &serde_json::to_vec(&old).unwrap()).unwrap()).unwrap();
        let mut a = Library::default();
        let rep = sync(&acc, &mut a, &sa, "PC", "dev-a").unwrap();
        assert!(!rep.new_account);
        assert!(a.docs.iter().any(|d| d.title == "old-version"));
        let _ = std::fs::remove_dir_all(r);
    }

    /// An original of a deleted document that can't be removed is reported (and tried again next sync).
    #[test]
    fn originals_that_cannot_be_removed_are_reported() {
        let r = temp_root("cleanup");
        let sa = temp_store(&r, "a");
        let (acc, _) = unlock(r.join("cloud").to_str().unwrap(), "me", "secret123").unwrap();
        let mut d = doc("gone");
        d.deleted = true;
        d.file_sha = crate::model::sha256_hex(b"x");
        // something that won't go away (a folder where the file would be)
        std::fs::create_dir_all(acc.dir.join("files").join(format!("{}-{}.enc", d.id, &d.file_sha[..16]))).unwrap();
        let mut a = Library { docs: vec![d], ..Default::default() };
        let rep = sync(&acc, &mut a, &sa, "PC", "dev-a").unwrap();
        assert_eq!(rep.cleanup_failed, 1);
        let _ = std::fs::remove_dir_all(r);
    }
}
