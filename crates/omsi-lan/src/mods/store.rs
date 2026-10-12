//! The joining player's store of the host's mods (`~/.openomsi/lan-store`), kept between
//! sessions so a map fetched once is not fetched again: every file only sealed (see
//! `omsi_cfg::vfs::seal`), its blob named by a keyed hash of its SHA-256 - neither its name
//! nor its hash can be read off the disk.
//!
//! The store's key has to outlive the game, so it is on the disk as well: on Windows sealed
//! for the user's account by the system (DPAPI), elsewhere in a file only the user may read
//! (`store.key`, mode 0600). That is obfuscation, not protection from the user: whoever runs
//! as the player can read the key, and openOMSI's source says what to do with it. (The
//! macOS keychain would ask the player to allow every new build of the game.) A key that is
//! lost or unreadable gives a new one, and the blobs of the old one go.

use super::Entry;
use omsi_cfg::vfs::{SealKey, SealedFile};
use ring::hkdf::{KeyType, Salt, HKDF_SHA256};
use ring::hmac;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(windows)]
const KEY_FILE: &str = "store.key.dpapi";
#[cfg(not(windows))]
const KEY_FILE: &str = "store.key";
const BLOB: &str = "blob";

pub(crate) struct Store {
    dir: PathBuf,
    key: Arc<SealKey>,
    names: hmac::Key,
}

struct Len32;

impl KeyType for Len32 {
    fn len(&self) -> usize {
        32
    }
}

impl Store {
    /// The store in `dir` (made when missing), with its key.
    pub(crate) fn open(dir: &Path) -> Result<Store, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        remove_legacy(dir);
        let key_path = dir.join(KEY_FILE);
        let master = match load_key(&key_path) {
            Some(k) => k,
            None => {
                // (blobs of a key that is gone can never be read again)
                let removed = remove_blobs(dir);
                if removed > 0 {
                    log::info!("LAN mods: the store's key was missing; {removed} files of the old one removed");
                }
                let k = omsi_cfg::vfs::random_key().map_err(|e| e.to_string())?;
                save_key(&key_path, &k).map_err(|e| format!("{}: {e}", key_path.display()))?;
                k
            }
        };
        // one key for the blobs, one for their names, both from the store's key
        let prk = Salt::new(HKDF_SHA256, b"openOMSI LAN store").extract(&master);
        let sub = |what: &[u8]| -> Result<[u8; 32], String> {
            let mut k = [0u8; 32];
            prk.expand(&[what], Len32).and_then(|o| o.fill(&mut k)).map_err(|_| "no store key".to_string())?;
            Ok(k)
        };
        Ok(Store { dir: dir.to_path_buf(), key: Arc::new(SealKey::new(&sub(b"blobs")?)), names: hmac::Key::new(hmac::HMAC_SHA256, &sub(b"names")?) })
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn key(&self) -> Arc<SealKey> {
        self.key.clone()
    }

    fn file(&self, e: &Entry) -> SealedFile {
        let sha = e.sha256.to_ascii_lowercase();
        let name: String = hmac::sign(&self.names, sha.as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect();
        SealedFile { rel: e.path.clone(), blob: self.dir.join(format!("{name}.{BLOB}")), aad: sha.into_bytes(), size: e.size }
    }

    /// `e`'s file, if a join before fetched it.
    pub(crate) fn has(&self, e: &Entry) -> Option<SealedFile> {
        let f = self.file(e);
        std::fs::metadata(&f.blob).ok().filter(|m| m.len() == omsi_cfg::vfs::sealed_len(e.size)).map(|_| f)
    }

    /// Keep `data` (checked against `e` already) sealed.
    pub(crate) fn put(&self, e: &Entry, data: &[u8]) -> std::io::Result<SealedFile> {
        let f = self.file(e);
        let sealed = omsi_cfg::vfs::seal(&self.key, &f.aad, data)?;
        let tmp = f.blob.with_extension("part");
        std::fs::write(&tmp, sealed).and_then(|_| std::fs::rename(&tmp, &f.blob))?;
        Ok(f)
    }
}

/// What a game before the encryption kept in the store: the plain files, named by their
/// SHA-256, and half-written ones.
pub(crate) fn remove_legacy(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut n = 0;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let plain = name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit());
        if (plain || name.ends_with(".part")) && std::fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    if n > 0 {
        log::info!("LAN mods: {n} unencrypted files of an older version removed from the store");
    }
}

fn remove_blobs(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == BLOB) && std::fs::remove_file(e.path()).is_ok()).count()
}

fn load_key(path: &Path) -> Option<[u8; 32]> {
    let raw = std::fs::read(path).ok()?;
    #[cfg(windows)]
    let raw = dpapi(&raw, false)?;
    raw.try_into().ok()
}

fn save_key(path: &Path, key: &[u8; 32]) -> std::io::Result<()> {
    #[cfg(windows)]
    let bytes = dpapi(key, true).ok_or_else(|| std::io::Error::other("the system did not protect the key"))?;
    #[cfg(not(windows))]
    let bytes = key.to_vec();
    let tmp = path.with_extension("new");
    let _ = std::fs::remove_file(&tmp);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    std::io::Write::write_all(&mut o.open(&tmp)?, &bytes)?;
    std::fs::rename(&tmp, path)
}

/// Seal (`protect`) or open `data` for this Windows user account.
#[cfg(windows)]
fn dpapi(data: &[u8], protect: bool) -> Option<Vec<u8>> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` points at `data` for the call; `out` is the system's buffer, copied
    // and then freed with LocalFree as the documentation asks
    unsafe {
        let r = if protect {
            CryptProtectData(&input, windows::core::w!("openOMSI LAN store"), None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        } else {
            CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        r.ok()?;
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_store_keeps_nothing_plain() {
        let dir = std::env::temp_dir().join(format!("openomsi-lan-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // an older version's plain file goes
        let legacy = dir.join("ab".repeat(32));
        std::fs::write(&legacy, b"old plain").unwrap();
        let marker = b"[name]\nPLAINTEXT-MARKER-store\n".repeat(50);
        let e = Entry { path: "maps/Secret Map/global.cfg".into(), size: marker.len() as u64, sha256: super::super::sha256_of(&marker) };
        let store = Store::open(&dir).unwrap();
        assert!(!legacy.exists());
        assert!(store.has(&e).is_none());
        let f = store.put(&e, &marker).unwrap();
        assert!(store.has(&e).is_some());
        for p in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = p.file_name().to_string_lossy().to_string();
            let bytes = std::fs::read(p.path()).unwrap();
            assert!(!bytes.windows(16).any(|w| w == b"PLAINTEXT-MARKER"), "plain text in {name}");
            assert!(!name.contains(&e.sha256) && !name.to_lowercase().contains("global"), "{name}");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(dir.join(KEY_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // the key survives: the same store opens the blob again
        let again = Store::open(&dir).unwrap();
        assert_eq!(omsi_cfg::vfs::unseal(&again.key, &f.aad, std::fs::read(&f.blob).unwrap()).unwrap(), marker);
        // mounted, the engine reads it under its own name
        let mount = dir.join("session");
        omsi_cfg::vfs::mount_sealed(&mount, again.key(), vec![again.has(&e).unwrap()]);
        let cfg = omsi_cfg::resolve_path(&mount, "MAPS\\secret map\\Global.cfg");
        assert_eq!(omsi_cfg::vfs::read(&cfg).unwrap(), marker);
        assert_eq!(omsi_cfg::vfs::list_dir(&mount.join("maps")).unwrap()[0].0, "Secret Map");
        omsi_cfg::vfs::unmount_sealed(&mount);
        // a lost key: the blobs go with it
        std::fs::remove_file(dir.join(KEY_FILE)).unwrap();
        let fresh = Store::open(&dir).unwrap();
        assert!(!f.blob.exists());
        assert!(fresh.has(&e).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
