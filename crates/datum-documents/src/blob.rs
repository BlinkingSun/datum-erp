//! Content-addressed immutable blob store.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
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

/// Filesystem store: `<root>/<aa>/<bb>/<hash>` (hex), write-once, fsync.
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
        let path = self.path_for(hash);
        let _ = fs::remove_file(&path);
        self.untrack(hash);
    }
}

impl BlobStore for FsBlobStore {
    fn put(&self, bytes: &[u8]) -> Result<BlobHash> {
        let hash = hash_bytes(bytes);
        let path = self.path_for(hash);
        if path.exists() {
            let existing = fs::read(&path)?;
            if existing.as_slice() == bytes {
                return Ok(hash);
            }
            return Err(Error::BlobWriteOnce {
                hash: hash.to_hex(),
            });
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .or_else(|e| {
                    if e.kind() == io::ErrorKind::AlreadyExists {
                        fs::remove_file(&tmp)?;
                        OpenOptions::new().write(true).create_new(true).open(&tmp)
                    } else {
                        Err(e)
                    }
                })?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        match fs::rename(&tmp, &path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists || path.exists() => {
                let _ = fs::remove_file(&tmp);
                let existing = fs::read(&path)?;
                if existing.as_slice() == bytes {
                    return Ok(hash);
                }
                return Err(Error::BlobWriteOnce {
                    hash: hash.to_hex(),
                });
            }
            Err(e) => return Err(e.into()),
        }
        if let Some(parent) = path.parent() {
            fsync_dir(parent)?;
        }
        self.track_new(hash);
        Ok(hash)
    }

    fn get(&self, hash: BlobHash) -> Result<Vec<u8>> {
        let path = self.path_for(hash);
        fs::read(&path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                Error::BlobMissing {
                    hash: hash.to_hex(),
                }
            } else {
                e.into()
            }
        })
    }

    fn verify(&self, hash: BlobHash) -> Result<()> {
        let bytes = self.get(hash)?;
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
        self.path_for(hash).exists()
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
            let path = self.path_for(hash);
            let _ = fs::remove_file(path);
        }
    }

    fn keep_puts(&self) {
        if let Ok(mut created) = self.created.lock() {
            created.clear();
        }
    }
}

fn fsync_dir(dir: &Path) -> io::Result<()> {
    let file = File::open(dir)?;
    file.sync_all()
}

/// Recompute and compare. Detects on-disk corruption.
pub fn verify_blob(store: &dyn BlobStore, hash: BlobHash) -> Result<()> {
    store.verify(hash)
}
