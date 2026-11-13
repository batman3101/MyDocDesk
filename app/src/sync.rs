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

fn merge_by_id<T: Clone>(local: &mut Vec<T>, remote: Vec<T>, id: impl Fn(&T) -> String, ts: impl Fn(&T) -> i64) -> usize {
    let mut pos: HashMap<String, usize> = local.iter().enumerate().map(|(i, x)| (id(x), i)).collect();
    let mut changed = 0;
    for r in remote {
        match pos.get(&id(&r)) {
            Some(&i) => {
                if ts(&r) > ts(&local[i]) {
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

pub fn merge(local: &mut Library, remote: Library) -> usize {
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

/// Two-way sync: read the account's library, merge, write it back, then exchange original files.
pub fn sync(acc: &Account, lib: &mut Library, store: &Store, device: &str) -> Result<SyncReport, String> {
    let lib_path = acc.dir.join("library.enc");
    let mut pulled_docs = 0;
    let new_account = !lib_path.exists();
    if lib_path.exists() {
        let data = std::fs::read(&lib_path).map_err(|e| e.to_string())?;
        let plain = decrypt(&acc.key, &data)?;
        let remote: Library = serde_json::from_slice(&plain).map_err(|e| crate::ft!("동기화 데이터를 읽지 못했습니다: {e}", "Không đọc được dữ liệu đồng bộ: {e}", e = e))?;
        pulled_docs = merge(lib, remote);
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
    let tmp = acc.dir.join("library.enc.tmp");
    std::fs::write(&tmp, encrypt(&acc.key, &plain)?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &lib_path).map_err(|e| e.to_string())?;

    // original files
    let (mut pushed_files, mut pulled_files) = (0, 0);
    for d in lib.docs.iter().filter(|d| d.has_file && !d.deleted) {
        let local = store.file_path(d);
        let remote = acc.dir.join("files").join(format!("{}.enc", d.id));
        if local.exists() && !remote.exists() {
            let bytes = std::fs::read(&local).map_err(|e| e.to_string())?;
            std::fs::write(&remote, encrypt(&acc.key, &bytes)?).map_err(|e| e.to_string())?;
            pushed_files += 1;
        } else if !local.exists() && remote.exists() {
            let data = std::fs::read(&remote).map_err(|e| e.to_string())?;
            let bytes = decrypt(&acc.key, &data)?;
            std::fs::write(&local, bytes).map_err(|e| e.to_string())?;
            pulled_files += 1;
        }
    }
    // files of deleted documents are removed from the shared folder
    for d in lib.docs.iter().filter(|d| d.deleted) {
        let _ = std::fs::remove_file(acc.dir.join("files").join(format!("{}.enc", d.id)));
    }
    Ok(SyncReport {
        pulled_docs,
        pushed_files,
        pulled_files,
        total_docs: lib.live_docs().count(),
        new_account,
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
            Store { dir, read_only: false }
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
        sync(&acc_a, &mut a, &sa, "PC").unwrap();
        // wrong password is refused
        assert!(unlock(cloud.to_str().unwrap(), "me", "wrong-pass").is_err());
        // Mac connects with the same account and receives the document and file
        let mut b = Library::default();
        let (acc_b, new_b) = unlock(cloud.to_str().unwrap(), "me", "secret123").unwrap();
        assert!(!new_b);
        let r = sync(&acc_b, &mut b, &sb, "Mac").unwrap();
        assert_eq!(r.total_docs, 1);
        assert_eq!(std::fs::read(sb.file_path(&d)).unwrap(), b"original bytes");
        // Mac edits, PC gets the edit
        b.docs[0].title = "계약서 (수정)".into();
        b.docs[0].updated_ms += 10;
        sync(&acc_b, &mut b, &sb, "Mac").unwrap();
        sync(&acc_a, &mut a, &sa, "PC").unwrap();
        assert_eq!(a.docs[0].title, "계약서 (수정)");
        assert_eq!(a.devices.len(), 2);
        // the cloud folder holds no readable text
        let raw = std::fs::read(acc_a.dir.join("library.enc")).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("보증금"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
