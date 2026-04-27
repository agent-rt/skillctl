//! 协议表面 wire schema。
//!
//! 见 PROTOCOL.md §3。

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use skillctl_core::Tier;
use skillctl_skill::Requires;

pub const PROTOCOL_VERSION: u32 = 1;

/// `list --project --format json` 顶层返回。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse {
    pub protocol: u32,
    pub scope: String,
    pub count: usize,
    pub skills: Vec<ListedSkill>,
}

/// `list` 中每条技能的 catalog 记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedSkill {
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub tier: Tier,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// `describe --format json` 返回。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribeResponse {
    pub protocol: u32,
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub tier: Tier,
    pub summary: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    pub requires: Requires,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,
}

/// 错误信封（所有 `--format json` 命令的错误形式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub protocol: u32,
    pub success: bool, // 始终为 false
    pub error: String, // 错误码，如 "ambiguous_skill_name"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<String>,
}

impl ErrorEnvelope {
    #[must_use]
    pub fn new(error_code: impl Into<String>) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            success: false,
            error: error_code.into(),
            hint: None,
            matches: Vec::new(),
        }
    }
}
