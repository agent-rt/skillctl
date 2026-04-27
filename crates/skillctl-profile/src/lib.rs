//! Profile：用户对"这一类项目我要这些技能"的策划沉淀。
//!
//! 见 REQ.md §4.0、§8.0、§8.0bis。

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use skillctl_core::{Error, Result, Tier};
use skillctl_id::SkillId;

/// 全局已安装的 profile。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub profile: ProfileMeta,
    #[serde(default)]
    pub skills: SkillsTable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMeta {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

/// `[skills.core]` / `[skills.extra]` 两段。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillsTable {
    #[serde(default)]
    pub core: BTreeMap<String, SkillSpec>,
    #[serde(default)]
    pub extra: BTreeMap<String, SkillSpec>,
}

/// profile / manifest 中对单个技能的引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SkillSpec {
    /// 简单形式：`"deps" = "*"`。
    Version(String),
    /// 完整形式：`{ version = "1.2.0", path = "..." }`。
    Detailed {
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        path: Option<camino::Utf8PathBuf>,
        #[serde(default)]
        git: Option<String>,
        #[serde(default)]
        rev: Option<String>,
    },
}

impl SkillSpec {
    #[must_use]
    pub fn version_req(&self) -> &str {
        match self {
            SkillSpec::Version(v) => v.as_str(),
            SkillSpec::Detailed { version: Some(v), .. } => v.as_str(),
            SkillSpec::Detailed { .. } => "*",
        }
    }
}

/// 解析 profile TOML。
pub fn parse_str(input: &str) -> Result<Profile> {
    toml::from_str(input).map_err(|e| Error::InvalidProfile(format!("toml: {e}")))
}

/// 序列化 profile 为 TOML 字符串。
pub fn to_string(profile: &Profile) -> Result<String> {
    toml::to_string_pretty(profile).map_err(|e| Error::InvalidProfile(format!("toml ser: {e}")))
}

/// 多 profile 合并：core 和 extra 取并集，后者不会被前者覆盖（首次出现胜出）。
#[must_use]
pub fn merge(profiles: &[Profile]) -> SkillsTable {
    let mut out = SkillsTable::default();
    for p in profiles {
        for (id, spec) in &p.skills.core {
            out.core.entry(id.clone()).or_insert_with(|| spec.clone());
        }
        for (id, spec) in &p.skills.extra {
            out.extra.entry(id.clone()).or_insert_with(|| spec.clone());
        }
    }
    out
}

/// 通过 ID 查询 spec 的目标 tier。
#[must_use]
pub fn tier_of(table: &SkillsTable, id: &SkillId) -> Option<Tier> {
    let key = id.to_string();
    if table.core.contains_key(&key) {
        Some(Tier::Core)
    } else if table.extra.contains_key(&key) {
        Some(Tier::Extra)
    } else {
        None
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[profile]
name = "rust-cli"
description = "Rust CLI starter"

[skills.core]
"deps" = "*"
"frontend-design" = "*"

[skills.extra]
"rust/review" = "1.2.0"
"#;

    #[test]
    fn parses_profile() {
        let p = parse_str(SAMPLE).expect("ok");
        assert_eq!(p.profile.name, "rust-cli");
        assert_eq!(p.skills.core.len(), 2);
        assert_eq!(p.skills.extra.len(), 1);
        assert_eq!(p.skills.extra["rust/review"].version_req(), "1.2.0");
    }

    #[test]
    fn merges_two_profiles() {
        let a = parse_str(SAMPLE).expect("ok");
        let b = parse_str(
            r#"
[profile]
name = "react"
[skills.core]
"react/best-practices" = "*"
"#,
        )
        .expect("ok");
        let merged = merge(&[a, b]);
        assert_eq!(merged.core.len(), 3);
    }
}
