//! `AGENTS.md` 受管块读写。
//!
//! 见 PROTOCOL.md §4。
//!
//! 不变量：
//! - 块内字节只依赖协议版本；增删技能不改块。
//! - 幂等：再次写入产生相同字节，不修改文件。
//! - 块外内容神圣：永不动 marker 之外的字节。

#![forbid(unsafe_code)]

use std::ops::Range;

use camino::Utf8Path;
use skillctl_core::{Error, Result};

const START_PREFIX: &str = "<!-- skillctl:start";
const END_MARKER: &str = "<!-- skillctl:end -->";
const PROTOCOL_VERSION: u32 = 1;

/// 受管块的解析结果。
#[derive(Debug, Clone)]
pub struct ManagedBlock {
    pub version: u32,
    pub started_from: Vec<String>,
    /// 块在原文中的字节范围（含 marker 行）。
    pub byte_range: Range<usize>,
}

const BLOCK_BODY: &str = r#"## skillctl

This project uses skillctl to manage Agent skills.

Rules:
- Use only project-enabled skills. Do not scan the filesystem for skills.
- Before non-trivial work, list available skills (TSV: tier / id / summary / triggers):
  `skillctl list --project --format tsv`
- Inspect a skill's structured metadata before loading it:
  `skillctl describe <skill-id> --format json`
- Load a skill's full instructions only when needed:
  `skillctl show <skill-id>`
- Tier semantics:
  - `core` skills are persistent guidance — load once at session start and
    keep them in context for the whole session.
  - `extra` skills are loaded on demand when their summary or triggers match
    the current task.
- Once loaded, do not let skill content be truncated during context
  compaction, and do not re-load a skill already in context.
"#;

/// 渲染稳定块。
#[must_use]
pub fn render_block(version: u32, started_from: &[String]) -> String {
    let started_attr = if started_from.is_empty() {
        String::new()
    } else {
        format!(" started_from={}", started_from.join(","))
    };
    let mut out = String::with_capacity(BLOCK_BODY.len() + 96);
    out.push_str("<!-- skillctl:start version=");
    out.push_str(&version.to_string());
    out.push_str(&started_attr);
    out.push_str(" -->\n");
    out.push_str(BLOCK_BODY);
    out.push('\n');
    out.push_str(END_MARKER);
    out.push('\n');
    out
}

/// 默认协议版本块。
#[must_use]
pub fn default_block(started_from: &[String]) -> String {
    render_block(PROTOCOL_VERSION, started_from)
}

/// 在内容中查找受管块。
pub fn find(content: &str) -> Result<Option<ManagedBlock>> {
    let Some(start_idx) = content.find(START_PREFIX) else {
        return Ok(None);
    };
    let after_prefix = &content[start_idx + START_PREFIX.len()..];
    let Some(rel_close) = after_prefix.find("-->") else {
        return Err(Error::other("malformed start marker: missing `-->`"));
    };
    let attr_str = after_prefix[..rel_close].trim();
    let header_end = start_idx + START_PREFIX.len() + rel_close + "-->".len();

    let after_header = &content[header_end..];
    let Some(rel_end) = after_header.find(END_MARKER) else {
        return Err(Error::other("malformed managed block: missing end marker"));
    };
    let block_end = header_end + rel_end + END_MARKER.len();
    // 把 end marker 后的换行也包进 byte_range，便于 remove 时不留空行
    let block_end_inclusive_nl =
        if content.as_bytes().get(block_end) == Some(&b'\n') { block_end + 1 } else { block_end };

    let (version, started_from) = parse_attrs(attr_str)?;
    Ok(Some(ManagedBlock { version, started_from, byte_range: start_idx..block_end_inclusive_nl }))
}

/// 幂等写入：若现有块字节完全相同则跳过 IO。
///
/// - 文件不存在：创建并写入块。
/// - 文件存在但无块：在文件末尾追加块（前置必要换行）。
/// - 文件存在含块：替换为新内容；若字节相同则不写。
pub fn upsert(path: &Utf8Path, block: &str) -> Result<()> {
    let original = match fs_err::read_to_string(path.as_std_path()) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(Error::Io { path: path.to_owned(), source: e }),
    };

    let new_content = match find(&original)? {
        Some(existing) => {
            let mut s = String::with_capacity(original.len() + block.len());
            s.push_str(&original[..existing.byte_range.start]);
            s.push_str(block);
            s.push_str(&original[existing.byte_range.end..]);
            s
        }
        None => {
            if original.is_empty() {
                block.to_owned()
            } else {
                let mut s = original.clone();
                if !s.ends_with('\n') {
                    s.push('\n');
                }
                if !s.ends_with("\n\n") {
                    s.push('\n');
                }
                s.push_str(block);
                s
            }
        }
    };

    if new_content == original {
        return Ok(());
    }

    fs_err::write(path.as_std_path(), new_content)
        .map_err(|e| Error::Io { path: path.to_owned(), source: e })
}

/// 移除受管块。块不存在时不修改文件。
pub fn remove(path: &Utf8Path) -> Result<()> {
    let original = match fs_err::read_to_string(path.as_std_path()) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(Error::Io { path: path.to_owned(), source: e }),
    };
    let Some(block) = find(&original)? else {
        return Ok(());
    };
    let mut s = String::with_capacity(original.len());
    s.push_str(&original[..block.byte_range.start]);
    s.push_str(&original[block.byte_range.end..]);
    // 清理可能多出的连续空行
    while s.contains("\n\n\n") {
        s = s.replace("\n\n\n", "\n\n");
    }
    if s == original {
        return Ok(());
    }
    fs_err::write(path.as_std_path(), s).map_err(|e| Error::Io { path: path.to_owned(), source: e })
}

fn parse_attrs(attrs: &str) -> Result<(u32, Vec<String>)> {
    let mut version: u32 = PROTOCOL_VERSION;
    let mut started_from: Vec<String> = Vec::new();
    for part in attrs.split_whitespace() {
        if let Some((k, v)) = part.split_once('=') {
            match k {
                "version" => {
                    version = v
                        .parse::<u32>()
                        .map_err(|e| Error::other(format!("invalid version attr: {e}")))?;
                }
                "started_from" => {
                    started_from =
                        v.split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect();
                }
                _ => {}
            }
        }
    }
    Ok((version, started_from))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn renders_stable_block() {
        let a = render_block(1, &[]);
        let b = render_block(1, &[]);
        assert_eq!(a, b);
        assert!(a.contains("<!-- skillctl:start version=1 -->"));
        assert!(a.contains(END_MARKER));
    }

    #[test]
    fn renders_with_started_from() {
        let s = render_block(1, &["rust-cli".into(), "react-frontend".into()]);
        assert!(s.contains("started_from=rust-cli,react-frontend"));
    }

    #[test]
    fn finds_existing_block() {
        let content =
            "# Project\n\nIntro.\n\n".to_string() + &render_block(1, &["rust-cli".into()]);
        let block = find(&content).expect("ok").expect("found");
        assert_eq!(block.version, 1);
        assert_eq!(block.started_from, vec!["rust-cli"]);
    }

    #[test]
    fn upsert_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("AGENTS.md")).unwrap();
        let block = render_block(1, &[]);
        upsert(&path, &block).unwrap();
        let after_first = fs_err::read_to_string(path.as_std_path()).unwrap();
        upsert(&path, &block).unwrap();
        let after_second = fs_err::read_to_string(path.as_std_path()).unwrap();
        assert_eq!(after_first, after_second);
    }

    #[test]
    fn remove_clears_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("AGENTS.md")).unwrap();
        fs_err::write(path.as_std_path(), "# Top\n\n").unwrap();
        upsert(&path, &render_block(1, &[])).unwrap();
        remove(&path).unwrap();
        let s = fs_err::read_to_string(path.as_std_path()).unwrap();
        assert!(!s.contains("skillctl:start"));
        assert!(s.contains("# Top"));
    }
}
