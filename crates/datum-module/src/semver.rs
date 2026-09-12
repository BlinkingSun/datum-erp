//! Semver parse and caret-range matching. No `semver` crate on the allow-list.

use crate::{Error, Result};

/// `major.minor.patch` (trailing components default to 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    /// Major.
    pub major: u64,
    /// Minor.
    pub minor: u64,
    /// Patch.
    pub patch: u64,
}

impl Version {
    /// Parse `"1.2.0"` / `"0.1"` / `"1"`.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.is_empty() {
            return Err(Error::Semver("empty version".into()));
        }
        let mut parts = s.split('.');
        let major = parse_comp(parts.next().unwrap_or("0"), "major")?;
        let minor = match parts.next() {
            Some(p) => parse_comp(p, "minor")?,
            None => 0,
        };
        let patch = match parts.next() {
            Some(p) => parse_comp(p, "patch")?,
            None => 0,
        };
        if parts.next().is_some() {
            return Err(Error::Semver(format!("too many components in {s}")));
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }

    /// Render `major.minor.patch`.
    pub fn to_canonical(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn parse_comp(s: &str, label: &str) -> Result<u64> {
    s.parse::<u64>()
        .map_err(|_| Error::Semver(format!("invalid {label} {s}")))
}

/// Caret range (`^1.2.0`) or an exact version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    inner: String,
}

impl Range {
    /// Parse a range. Bare versions are exact; `^x.y.z` is a caret range.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.is_empty() {
            return Err(Error::Semver("empty range".into()));
        }
        if let Some(rest) = s.strip_prefix('^') {
            let _ = Version::parse(rest)?;
        } else {
            let _ = Version::parse(s)?;
        }
        Ok(Self {
            inner: s.to_string(),
        })
    }

    /// Whether `version` satisfies this range.
    pub fn matches(&self, version: &Version) -> bool {
        if let Some(rest) = self.inner.strip_prefix('^') {
            let Ok(base) = Version::parse(rest) else {
                return false;
            };
            if version < &base {
                return false;
            }
            let upper = if base.major > 0 {
                Version {
                    major: base.major + 1,
                    minor: 0,
                    patch: 0,
                }
            } else if base.minor > 0 {
                Version {
                    major: 0,
                    minor: base.minor + 1,
                    patch: 0,
                }
            } else {
                Version {
                    major: 0,
                    minor: 0,
                    patch: base.patch + 1,
                }
            };
            version < &upper
        } else {
            Version::parse(&self.inner).ok().as_ref() == Some(version)
        }
    }
}
