//! `SKILL.md` 解析与宽松校验。
//!
//! 见 PROTOCOL.md §6。

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use skillctl_core::{Error, Result};
use skillctl_id::Namespace;

/// 解析后的 SKILL.md 文档。
#[derive(Debug, Clone)]
pub struct SkillDoc {
    pub frontmatter: Frontmatter,
    pub body: String,
}

/// SKILL.md 顶层 frontmatter。
///
/// 严格必填：`name`、`description`。
/// `version` 缺省为 `"0.0.0"`（与 agentskills.io 最小 frontmatter 兼容）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub compatibility: Option<String>,
}

/// `metadata.*` 扩展字段。所有字段可选。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(default)]
    pub namespace: Option<Namespace>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub requires: Requires,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Requires {
    #[serde(default)]
    pub binaries: Vec<BinaryReq>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub runtimes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryReq {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// 诊断条目（宽松校验产物）。
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

/// 解析 `SKILL.md` 字符串。
///
/// 严格 frontmatter 解析；首次失败时尝试**宽松重试**——对未引号且含冒号的标量值
/// 自动加引号后重试一次（PROTOCOL.md §6.1）。仍失败时返回 `Error::InvalidSkill`。
pub fn parse_str(input: &str) -> Result<SkillDoc> {
    use gray_matter::engine::YAML;
    use gray_matter::Matter;

    let matter = Matter::<YAML>::new();
    let parsed = match matter.parse::<Frontmatter>(input) {
        Ok(p) => p,
        Err(_) => {
            let fixed = fix_unquoted_colons(input);
            matter
                .parse::<Frontmatter>(&fixed)
                .map_err(|e| Error::InvalidSkill(format!("frontmatter parse failed: {e}")))?
        }
    };
    let mut frontmatter =
        parsed.data.ok_or_else(|| Error::InvalidSkill("missing frontmatter".into()))?;

    if frontmatter.name.trim().is_empty() {
        return Err(Error::InvalidSkill("frontmatter `name` is empty".into()));
    }
    if frontmatter.version.trim().is_empty() {
        frontmatter.version = default_version();
    }
    if frontmatter.description.trim().is_empty() {
        return Err(Error::InvalidSkill("frontmatter `description` is empty".into()));
    }

    Ok(SkillDoc { frontmatter, body: parsed.content })
}

/// 派生 summary：缺省时由 description 截断而来。
///
/// 上限：80 个 CJK 字符 / 160 个 ASCII 字符（按字符计）。
#[must_use]
pub fn derive_summary(fm: &Frontmatter) -> String {
    if let Some(s) = fm.metadata.summary.as_ref() {
        if !s.trim().is_empty() {
            return s.trim().to_owned();
        }
    }
    let desc = fm.description.trim();
    let limit = if desc.chars().any(is_cjk) { 80 } else { 160 };
    if desc.chars().count() <= limit {
        return desc.to_owned();
    }
    let mut out: String = desc.chars().take(limit.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn default_version() -> String {
    "0.0.0".to_owned()
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF |   // CJK Unified
        0x3000..=0x30FF |   // CJK punct + Japanese kana
        0xAC00..=0xD7AF     // Hangul
    )
}

/// 把 frontmatter 中未引号的、含冒号的标量值自动加引号。
///
/// 仅处理顶层 frontmatter 与子表中的简单 `key: value` 行；不触碰
/// 块标量 / 序列 / 流式集合。
fn fix_unquoted_colons(input: &str) -> String {
    // 找 frontmatter 边界
    let Some(rest) = input.strip_prefix("---\n").or_else(|| input.strip_prefix("---\r\n")) else {
        return input.to_owned();
    };
    let Some(end_rel) = rest.find("\n---") else { return input.to_owned() };
    let frontmatter = &rest[..end_rel];
    let after_fm = &rest[end_rel..];

    let mut new_fm = String::with_capacity(frontmatter.len() + 32);
    for line in frontmatter.lines() {
        // 找第一个冒号位置（key: value）
        let trimmed_start = line.trim_start();
        let indent_len = line.len() - trimmed_start.len();
        let indent = &line[..indent_len];

        if let Some(colon_idx) = trimmed_start.find(':') {
            let key = &trimmed_start[..colon_idx];
            let after = &trimmed_start[colon_idx + 1..];
            if !key.is_empty()
                && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                && (after.starts_with(' ') || after.starts_with('\t'))
            {
                let value = after.trim_start();
                let needs_quote = !value.is_empty()
                    && value.contains(':')
                    && !value.starts_with('"')
                    && !value.starts_with('\'')
                    && !value.starts_with('|')
                    && !value.starts_with('>')
                    && !value.starts_with('[')
                    && !value.starts_with('{')
                    && !value.starts_with('#');
                if needs_quote {
                    new_fm.push_str(indent);
                    new_fm.push_str(key);
                    new_fm.push_str(": \"");
                    new_fm.push_str(&value.replace('\\', "\\\\").replace('"', "\\\""));
                    new_fm.push_str("\"\n");
                    continue;
                }
            }
        }
        new_fm.push_str(line);
        new_fm.push('\n');
    }

    let mut out = String::with_capacity(input.len() + 16);
    out.push_str("---\n");
    out.push_str(&new_fm);
    out.push_str(after_fm);
    out
}

/// 宽松校验：对装饰性违规返回警告，对致命违规返回错误。
#[must_use]
pub fn validate(doc: &SkillDoc) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    if doc.frontmatter.name.len() > 64 {
        diags.push(Diagnostic {
            severity: Severity::Warning,
            message: format!("name exceeds 64 chars: {}", doc.frontmatter.name),
        });
    }
    if doc.body.trim().is_empty() {
        diags.push(Diagnostic {
            severity: Severity::Warning,
            message: "SKILL.md body is empty".into(),
        });
    }

    diags
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---\n\
name: review\n\
version: \"1.2.0\"\n\
description: Rust 代码审查与最佳实践\n\
metadata:\n  \
  summary: \"Rust 审查\"\n  \
  tags: [rust, review]\n\
---\n\
\n\
# Review\n\
\n\
Body here.\n";

    #[test]
    fn parses_valid_skill() {
        let doc = parse_str(SAMPLE).expect("parse ok");
        assert_eq!(doc.frontmatter.name, "review");
        assert_eq!(doc.frontmatter.version, "1.2.0");
        assert_eq!(doc.frontmatter.metadata.tags, vec!["rust", "review"]);
        assert!(doc.body.contains("Body here."));
    }

    #[test]
    fn rejects_missing_frontmatter() {
        assert!(parse_str("# just markdown").is_err());
    }

    #[test]
    fn lenient_unquoted_colon_in_description() {
        let raw = "---\n\
name: x\n\
version: \"0.1.0\"\n\
description: Use this skill when: the user asks about PDFs\n\
---\n\
\n\
body";
        let doc = parse_str(raw).expect("lenient retry should succeed");
        assert!(doc.frontmatter.description.contains("Use this skill when:"));
    }

    #[test]
    fn derives_summary_from_description_when_missing() {
        let mut fm: Frontmatter = serde_json::from_str(
            r#"{
            "name":"x","version":"0.1.0","description":"短描述",
            "metadata": { "summary": null }
        }"#,
        )
        .expect("json");
        fm.metadata.summary = None;
        assert_eq!(derive_summary(&fm), "短描述");
    }
}
