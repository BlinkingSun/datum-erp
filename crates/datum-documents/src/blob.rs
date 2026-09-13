//! Content-addressed immutable blob store.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::domain::BlobHash;
use crate::error::{Error, Result};

/// Content-addressed blob store. Bytes live on disk; the database holds the hash.
pub trait BlobStore: Send + Sync {
    /// Write bytes once. Same bytes return the same hash (dedup).
    fn put(&self, bytes: &[u8]) -> Result<BlobHash>;
    /// Read bytes by hash.
    fn get(&self, hash: BlobHash) -> Result<Vec<u8>>;
    /// Recompute the digest of the stored file and compare.
    fn verify(&self, hash: BlobHash) -> Result<()>;
}

/// Filesystem store: `<root>/<aa>/<bb>/<hash>` (hex), write-once, fsync.
#[derive(Debug, Clone)]
pub struct FsBlobStore {
    root: PathBuf,
}

impl FsBlobStore {
    /// Store under `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
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
}

impl BlobStore for FsBlobStore {
    fn put(&self, bytes: &[u8]) -> Result<BlobHash> {
        let hash = BlobHash(datum_audit::sha256::digest(bytes));
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
}

fn fsync_dir(dir: &Path) -> io::Result<()> {
    let file = File::open(dir)?;
    file.sync_all()
}

/// Recompute and compare. Detects on-disk corruption.
pub fn verify_blob(store: &dyn BlobStore, hash: BlobHash) -> Result<()> {
    store.verify(hash)
}
