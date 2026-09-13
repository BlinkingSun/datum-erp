//! Content-addressed immutable blob store.
//!
//! Windows filesystem rules this store satisfies (unix is a subset):
//!
//! 1. A file cannot be deleted or renamed-over while any handle is open.
//!    Drop/close every `File` (temp writer and any hash/verify reader)
//!    before `rename` or `remove_file`.
//! 2. `FILE_ATTRIBUTE_READONLY` survives rename and also denies delete.
//!    Set it on the exact final path after placement; clear it on that
//!    same path with `std::fs::set_permissions` +
//!    `Permissions::set_readonly(false)` before any `remove_file`.
//! 3. A placed read-only file is never opened with write/append/truncate.
//!    Dedupe is an existence check (`Path::exists` / `fs::metadata`). The
//!    placed blob is only ever opened read-only (`get` / `verify`).
//! 4. Directory deletion requires the directory to be empty and no open
//!    handles inside. This store never deletes directories.
//! 5. `File::sync_all` the temp writer before rename. Rename into place
//!    must not target an existing file (existence check first).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::domain::BlobHash;
use crate::error::{Error, Result};

/// SHA-256 of `bytes` (the blob primary key). Computed before any disk write.
pub fn hash_bytes(bytes: &[u8]) -> BlobHash {
    BlobHash(datum_audit::sha256::digest(bytes))
}

/// Content-addressed blob store. Bytes live on disk; the database holds the hash.
pub trait BlobStore: Send + Sync {
    /// Write bytes once. Same bytes return the same hash (dedup).
    fn put(&self, bytes: &[u8]) -> Result<BlobHash>;
    /// Read bytes by hash.
    fn get(&self, hash: BlobHash) -> Result<Vec<u8>>;
    /// Recompute the digest of the stored file and compare.
    fn verify(&self, hash: BlobHash) -> Result<()>;
    /// Whether the content-addressed path already exists.
    fn exists(&self, hash: BlobHash) -> bool;
    /// Remove a path after a failed write. No-op if the file is absent.
    fn discard_hash(&self, hash: BlobHash);
    /// Delete objects created by [`Self::put`] since [`Self::keep_puts`].
    /// The composition root calls this after a transaction rollback.
    fn discard_uncommitted(&self);
    /// Drop tracking for new objects after a successful commit (files stay).
    fn keep_puts(&self);
}

/// Filesystem store: `<root>/<aa>/<bb>/<hash>` (hex only), write-once, fsync.
#[derive(Debug)]
pub struct FsBlobStore {
    root: PathBuf,
    created: Mutex<Vec<BlobHash>>,
}

impl FsBlobStore {
    /// Store under `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            created: Mutex::new(Vec::new()),
        }
    }

    /// Store under `DATUM_BLOB_ROOT`.
    pub fn from_env() -> Result<Self> {
        let root = std::env::var("DATUM_BLOB_ROOT").map_err(|_| Error::BlobRootMissing)?;
        if root.is_empty() {
            return Err(Error::BlobRootMissing);
        }
        Ok(Self::new(root))
    }

    /// Configured root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, hash: BlobHash) -> PathBuf {
        let hex = hash.to_hex();
        debug_assert_eq!(hex.len(), 64);
        debug_assert!(
            hex.bytes().all(|b| b.is_ascii_hexdigit()),
            "blob names are lowercase hex; Windows reserved characters are forbidden"
        );
        let aa = &hex[0..2];
        let bb = &hex[2..4];
        self.root.join(aa).join(bb).join(hex)
    }

    fn track_new(&self, hash: BlobHash) {
        if let Ok(mut created) = self.created.lock() {
            created.push(hash);
        }
    }

    fn untrack(&self, hash: BlobHash) {
        if let Ok(mut created) = self.created.lock() {
            created.retain(|h| *h != hash);
        }
    }

    fn remove_file(&self, hash: BlobHash) {
        remove_placed(&self.path_for(hash));
        self.untrack(hash);
    }

    /// Rule 3 / 5: dedupe is an existence check. Never opens the placed file.
    fn already_placed(path: &Path) -> bool {
        fs::metadata(path).is_ok()
    }

    /// Rule 3: open the placed blob read-only. Never write/append/truncate.
    fn read_placed(&self, hash: BlobHash) -> Result<Vec<u8>> {
        let path = self.path_for(hash);
        let mut file = File::open(&path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                Error::BlobMissing {
                    hash: hash.to_hex(),
                }
            } else {
                e.into()
            }
        })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        // Rule 1: close the reader before any later rename/remove.
        drop(file);
        Ok(bytes)
    }
}

impl BlobStore for FsBlobStore {
    fn put(&self, bytes: &[u8]) -> Result<BlobHash> {
        let hash = hash_bytes(bytes);
        let path = self.path_for(hash);
        if Self::already_placed(&path) {
            return Ok(hash);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_file_name(format!("{}.tmp", hash.to_hex()));
        if let Err(e) = write_tmp(&tmp, bytes) {
            // write_tmp dropped the File before returning.
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        if Self::already_placed(&path) {
            let _ = fs::remove_file(&tmp);
            return Ok(hash);
        }
        match fs::rename(&tmp, &path) {
            Ok(()) => {}
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                if Self::already_placed(&path) {
                    return Ok(hash);
                }
                return Err(e.into());
            }
        }
        // Rule 2: readonly survives rename — set it on the exact final path.
        set_readonly(&path, true)?;
        if let Some(parent) = path.parent() {
            fsync_dir(parent)?;
        }
        self.track_new(hash);
        Ok(hash)
    }

    fn get(&self, hash: BlobHash) -> Result<Vec<u8>> {
        self.read_placed(hash)
    }

    fn verify(&self, hash: BlobHash) -> Result<()> {
        let bytes = self.read_placed(hash)?;
        let got = datum_audit::sha256::digest(&bytes);
        if got == hash.0 {
            Ok(())
        } else {
            Err(Error::BlobCorrupt {
                hash: hash.to_hex(),
            })
        }
    }

    fn exists(&self, hash: BlobHash) -> bool {
        Self::already_placed(&self.path_for(hash))
    }

    fn discard_hash(&self, hash: BlobHash) {
        self.remove_file(hash);
    }

    fn discard_uncommitted(&self) {
        let hashes = self
            .created
            .lock()
            .map(|mut created| created.drain(..).collect::<Vec<_>>())
            .unwrap_or_default();
        for hash in hashes {
            remove_placed(&self.path_for(hash));
        }
    }

    fn keep_puts(&self) {
        if let Ok(mut created) = self.created.lock() {
            created.clear();
        }
    }
}

/// Write `bytes` to `tmp`, fsync, and close the handle (rule 1 and 5).
fn write_tmp(tmp: &Path, bytes: &[u8]) -> io::Result<()> {
    if tmp.exists() {
        let _ = set_readonly(tmp, false);
        fs::remove_file(tmp)?;
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    // Rule 1: the temp writer must be closed before rename / remove_file.
    drop(file);
    Ok(())
}

/// FILE_ATTRIBUTE_READONLY / unix write-bit. Placed blobs are immutable;
/// Windows also refuses delete, rename-over, and reopen-for-write until cleared.
fn set_readonly(path: &Path, readonly: bool) -> io::Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    {
        perms.set_readonly(readonly);
    }
    fs::set_permissions(path, perms)
}

/// Rollback / orphan cleanup: clear read-only on the exact final path, then unlink.
/// No-op if absent. No handle may be open (rule 1). Directories are left in place (rule 4).
fn remove_placed(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut perms = meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        {
            perms.set_readonly(false);
        }
        let _ = fs::set_permissions(path, perms);
    }
    let _ = fs::remove_file(path);
}

fn fsync_dir(dir: &Path) -> io::Result<()> {
    // Windows: `File::open` on a directory is ERROR_ACCESS_DENIED (5) unless
    // FILE_FLAG_BACKUP_SEMANTICS is set; FlushFileBuffers on a directory
    // handle is the same error. File durability is `sync_all` before rename
    // (rule 5). Close the directory handle before returning (rule 4).
    let file = match File::open(dir) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return Ok(()),
        Err(e) => return Err(e),
    };
    let result = match file.sync_all() {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Ok(()),
        Err(e) => Err(e),
    };
    drop(file);
    result
}

/// Recompute and compare. Detects on-disk corruption.
pub fn verify_blob(store: &dyn BlobStore, hash: BlobHash) -> Result<()> {
    store.verify(hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;

    fn tmp_store() -> FsBlobStore {
        let root = std::env::temp_dir().join(format!(
            "datum-blob-unit-{}",
            datum_core::Identifier::generate()
        ));
        fs::create_dir_all(&root).unwrap();
        FsBlobStore::new(root)
    }

    #[test]
    fn put_dedupes_without_opening_placed_file_for_write() {
        let store = tmp_store();
        let hash = store.put(b"same-bytes-twice").unwrap();
        let again = store.put(b"same-bytes-twice").unwrap();
        assert_eq!(hash, again);
        let path = store.path_for(hash);
        let name = path.file_name().and_then(|n| n.to_str()).unwrap();
        assert_eq!(name, hash.to_hex());
        assert!(name.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(
            !name.contains(':'),
            "Windows reserved character in blob name"
        );
        let write = OpenOptions::new().write(true).open(&path);
        assert!(write.is_err(), "placed blob must not be openable for write");
    }

    #[test]
    fn discard_uncommitted_clears_readonly_then_deletes() {
        let store = tmp_store();
        let hash = store.put(b"rollback-me").unwrap();
        assert!(store.exists(hash));
        store.discard_uncommitted();
        assert!(!store.exists(hash));
    }

    #[test]
    fn put_syncs_tmp_then_renames_onto_absent_readonly_path() {
        let store = tmp_store();
        let hash = store.put(b"place-once").unwrap();
        let path = store.path_for(hash);
        let tmp = path.with_file_name(format!("{}.tmp", hash.to_hex()));
        assert!(path.is_file());
        assert!(
            !tmp.exists(),
            "tmp must be gone after the handle is closed and renamed"
        );
        assert!(
            fs::metadata(&path).unwrap().permissions().readonly(),
            "readonly is set on the exact final path after placement"
        );
        store.verify(hash).unwrap();
        assert_eq!(store.get(hash).unwrap(), b"place-once");
    }

    #[cfg(unix)]
    #[test]
    fn placed_blob_has_unix_write_bits_cleared() {
        use std::os::unix::fs::PermissionsExt;
        let store = tmp_store();
        let hash = store.put(b"unix-mode").unwrap();
        let mode = fs::metadata(store.path_for(hash))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o222, 0, "unix write bits cleared after placement");
    }
}
