use std::fmt;

use serde::{Deserialize, Serialize};

/// Identifier of a question / answer, e.g. `project_name`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AnswerId(pub String);

/// Identifier of a hook, e.g. `uv-sync`. Human slug, unique within a template.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HookId(pub String);

/// Content address of a patch: blake3 of its canonical serialization.
///
/// The hash covers dependency *ids* (not names), so patch ids form a Merkle
/// DAG: changing a patch changes the id of everything downstream of it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PatchId([u8; 32]);

impl PatchId {
    pub fn from_canonical_bytes(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    pub fn to_hex(self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }

    pub fn parse(s: &str) -> Result<Self, IdError> {
        let hash = blake3::Hash::from_hex(s).map_err(|_| IdError::BadPatchId(s.to_owned()))?;
        Ok(Self(*hash.as_bytes()))
    }

    /// Short prefix for human-facing output.
    pub fn short(self) -> String {
        self.to_hex()[..12].to_owned()
    }
}

impl fmt::Display for PatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for PatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PatchId({})", self.short())
    }
}

impl Serialize for PatchId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for PatchId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for AnswerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for HookId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for AnswerId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<&str> for HookId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdError {
    #[error("invalid patch id (expected 64 hex chars): {0:?}")]
    BadPatchId(String),
}
