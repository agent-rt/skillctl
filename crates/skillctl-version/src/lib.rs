//! 版本与 git ref 解析。
//!
//! 见 REQ.md §15。

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

pub use semver::{Version, VersionReq};

/// 版本约束的统一表达。
///
/// 优先级：commit > tag > branch > semver range > latest。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Constraint {
    Latest,
    Semver(VersionReq),
    Tag(String),
    Branch(String),
    Commit(String),
}

/// 解析后的精确版本事实，写入 lock file。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolved {
    pub version: String,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
}
