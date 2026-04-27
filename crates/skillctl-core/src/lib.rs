//! skillctl 共享类型层。
//!
//! 不做 IO，不依赖业务 crate。其他所有 crate 都只通过本 crate 引用基础类型。

#![forbid(unsafe_code)]

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

/// skillctl 全局错误类型。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid skill id: {0}")]
    InvalidSkillId(String),

    #[error("invalid skill document: {0}")]
    InvalidSkill(String),

    #[error("invalid profile: {0}")]
    InvalidProfile(String),

    #[error("skill not found: {0}")]
    SkillNotFound(String),

    #[error("profile not found: {0}")]
    ProfileNotFound(String),

    #[error("ambiguous skill name: {name} matches {matches:?}")]
    AmbiguousSkillName { name: String, matches: Vec<String> },

    #[error("project not initialized: {0}")]
    NotAProject(Utf8PathBuf),

    #[error("project not trusted: {0}")]
    UntrustedProject(Utf8PathBuf),

    #[error("dependency missing: {0}")]
    DependencyMissing(String),

    #[error("source error: {0}")]
    Source(String),

    #[error("checksum mismatch for {id}: expected {expected}, found {found}")]
    ChecksumMismatch { id: String, expected: String, found: String },

    #[error("{0}")]
    Other(String),
}

impl Error {
    /// 便利构造，用于把第三方错误包成 skillctl 错误。
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }

    /// 把任意 Display 实现转成 Other（典型用法：`.map_err(Error::wrap("ctx"))`）。
    pub fn wrap<E: std::fmt::Display>(ctx: &'static str) -> impl Fn(E) -> Self + 'static {
        move |e| Error::Other(format!("{ctx}: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// 技能加载分层（对应协议 `tier` 字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// 核心：进入项目即在场。Agent 视为已加载。
    Core,
    /// 扩展：catalog 中可见，按需加载。
    Extra,
}

/// 查询作用域。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Project,
}
