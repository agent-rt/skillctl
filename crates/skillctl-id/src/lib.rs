//! Skill ID：`namespace/name` 强制必填，协议表面唯一身份。
//!
//! 见 PROTOCOL.md §5。

#![forbid(unsafe_code)]

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use skillctl_core::{Error, Result};

const NAMESPACE_MAX: usize = 39;
const NAME_MAX: usize = 63;
const TOTAL_MAX: usize = 100;

/// 命名空间 Newtype。
///
/// 字符集 `[a-z0-9][a-z0-9-]{0,38}`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Namespace(String);

/// 技能短名 Newtype。
///
/// 字符集 `[a-z0-9][a-z0-9-]{0,62}`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SkillName(String);

/// 完整 Skill ID。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkillId {
    pub namespace: Namespace,
    pub name: SkillName,
}

impl Namespace {
    pub fn parse(s: &str) -> Result<Self> {
        if !is_valid_segment(s, NAMESPACE_MAX) {
            return Err(Error::InvalidSkillId(format!("invalid namespace: {s}")));
        }
        Ok(Self(s.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl SkillName {
    pub fn parse(s: &str) -> Result<Self> {
        if !is_valid_segment(s, NAME_MAX) {
            return Err(Error::InvalidSkillId(format!("invalid name: {s}")));
        }
        Ok(Self(s.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl SkillId {
    pub fn parse(s: &str) -> Result<Self> {
        if s.len() > TOTAL_MAX {
            return Err(Error::InvalidSkillId(format!("id too long: {s}")));
        }
        let (ns, name) = s
            .split_once('/')
            .ok_or_else(|| Error::InvalidSkillId(format!("missing namespace: {s}")))?;
        Ok(Self { namespace: Namespace::parse(ns)?, name: SkillName::parse(name)? })
    }
}

impl FromStr for SkillId {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl fmt::Display for SkillId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.namespace.0, self.name.0)
    }
}

fn is_valid_segment(s: &str, max: usize) -> bool {
    if s.is_empty() || s.len() > max {
        return false;
    }
    let bytes = s.as_bytes();
    let first = bytes[0];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    bytes.iter().skip(1).all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_id() {
        let id = SkillId::parse("rust/review").expect("valid");
        assert_eq!(id.namespace.as_str(), "rust");
        assert_eq!(id.name.as_str(), "review");
        assert_eq!(id.to_string(), "rust/review");
    }

    #[test]
    fn rejects_missing_namespace() {
        assert!(SkillId::parse("review").is_err());
    }

    #[test]
    fn rejects_uppercase() {
        assert!(SkillId::parse("Rust/Review").is_err());
    }
}
