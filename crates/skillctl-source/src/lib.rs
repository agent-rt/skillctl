//! 来源解析：github / 本地路径 / URL。
//!
//! 网络/Git 实现按 feature 分层，本地高频命令（list / describe / show）
//! 不应触发任何 IO 之外的依赖。
//!
//! 见 REQ.md §16、§18。

#![forbid(unsafe_code)]

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use skillctl_core::{Error, Result};

/// 技能/profile 来源。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    Github {
        repo: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(rename = "ref")]
        reference: Option<String>,
    },
    Local {
        path: Utf8PathBuf,
    },
    Url {
        url: String,
    },
}

/// 已下载到本地的临时拷贝。
///
/// 持有 `TempDir` 直到调用方使用完毕；drop 时自动清理。
#[derive(Debug)]
pub struct Fetched {
    /// 拷贝根目录（指向 SKILL.md 所在路径）。
    pub root: Utf8PathBuf,
    /// 已解析的 git commit hash（github 来源）。
    pub commit: Option<String>,
    /// 来源类型字符串，用于写入 lock。
    pub source_kind: &'static str,
    /// 仓库引用（github 来源），可写入 registry.lock。
    pub repo: Option<String>,
    /// 持有以保证 `root` 在 drop 前有效。
    _tempdir: Option<tempfile::TempDir>,
}

/// 解析来源字符串。
///
/// 支持：
/// - `./local/path` 或 `/abs/path` → Local
/// - `github:user/repo[/sub/path][@ref]` → Github
/// - `https://...` 或 `http://...` → Url
pub fn parse_source(s: &str) -> Result<Source> {
    if let Some(rest) = s.strip_prefix("github:") {
        return parse_github(rest);
    }
    if s.starts_with("https://") || s.starts_with("http://") {
        return Ok(Source::Url { url: s.to_owned() });
    }
    Ok(Source::Local { path: Utf8PathBuf::from(s) })
}

fn parse_github(rest: &str) -> Result<Source> {
    let (path_part, reference) = match rest.rsplit_once('@') {
        Some((p, r)) if !r.is_empty() => (p, Some(r.to_owned())),
        _ => (rest, None),
    };
    let mut segments = path_part.splitn(3, '/');
    let owner = segments
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::other(format!("invalid github source: {rest}")))?;
    let name = segments
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::other(format!("invalid github source: {rest}")))?;
    let path = segments.next().map(str::to_owned).filter(|s| !s.is_empty());
    Ok(Source::Github { repo: format!("{owner}/{name}"), path, reference })
}

/// 解析并拉取来源。
pub fn fetch(source: &Source) -> Result<Fetched> {
    match source {
        Source::Local { path } => fetch_local(path),
        Source::Github { repo, path, reference } => {
            #[cfg(feature = "source-git-cli")]
            {
                return git_cli::fetch_github(repo, path.as_deref(), reference.as_deref());
            }
            #[cfg(not(feature = "source-git-cli"))]
            {
                let _ = (repo, path, reference);
                return Err(Error::Source(
                    "github source requires `source-git-cli` feature".into(),
                ));
            }
        }
        Source::Url { .. } => Err(Error::Source(
            "URL source not yet implemented (requires `source-http` feature)".into(),
        )),
    }
}

fn fetch_local(path: &camino::Utf8Path) -> Result<Fetched> {
    let abs = if path.is_absolute() {
        path.to_owned()
    } else {
        let cwd = std::env::current_dir().map_err(|e| Error::other(format!("cwd: {e}")))?;
        Utf8PathBuf::from_path_buf(cwd)
            .map_err(|p| Error::other(format!("non-utf8 cwd: {p:?}")))?
            .join(path)
    };
    if !abs.exists() {
        return Err(Error::Source(format!("local source not found: {abs}")));
    }
    Ok(Fetched { root: abs, commit: None, source_kind: "local", repo: None, _tempdir: None })
}

#[cfg(feature = "source-git-cli")]
mod git_cli {
    use super::*;
    use std::process::{Command, Stdio};

    pub fn fetch_github(
        repo: &str,
        sub_path: Option<&str>,
        reference: Option<&str>,
    ) -> Result<Fetched> {
        let temp = tempfile::tempdir().map_err(|e| Error::Source(format!("tempdir: {e}")))?;
        let temp_path = Utf8PathBuf::from_path_buf(temp.path().to_path_buf())
            .map_err(|p| Error::Source(format!("non-utf8 tempdir: {p:?}")))?;

        let url = format!("https://github.com/{repo}.git");
        let mut args: Vec<&str> = vec!["clone", "--depth", "1"];
        if let Some(r) = reference {
            args.push("--branch");
            args.push(r);
        }
        args.push(&url);
        let temp_str = temp_path.as_str();
        args.push(temp_str);

        let output = Command::new("git")
            .args(&args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Error::Source(format!("`git` not available: {e}")))?;
        if !output.status.success() {
            return Err(Error::Source(format!(
                "git clone {url} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        let rev = Command::new("git")
            .args(["-C", temp_str, "rev-parse", "HEAD"])
            .output()
            .map_err(|e| Error::Source(format!("git rev-parse failed: {e}")))?;
        let commit = if rev.status.success() {
            Some(String::from_utf8_lossy(&rev.stdout).trim().to_owned())
        } else {
            None
        };

        let root = match sub_path {
            Some(p) => temp_path.join(p),
            None => temp_path.clone(),
        };
        if !root.exists() {
            return Err(Error::Source(format!("path not found in repo: {root}")));
        }

        Ok(Fetched {
            root,
            commit,
            source_kind: "github",
            repo: Some(repo.to_owned()),
            _tempdir: Some(temp),
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::missing_panics_doc, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_simple() {
        let s = parse_source("github:foo/bar").expect("ok");
        assert!(
            matches!(&s, Source::Github { repo, path: None, reference: None } if repo == "foo/bar")
        );
    }

    #[test]
    fn parses_github_with_path_and_ref() {
        let s = parse_source("github:foo/bar/skills/imgctl@v1.0").expect("ok");
        if let Source::Github { repo, path, reference } = s {
            assert_eq!(repo, "foo/bar");
            assert_eq!(path.as_deref(), Some("skills/imgctl"));
            assert_eq!(reference.as_deref(), Some("v1.0"));
        } else {
            unreachable!("wrong variant");
        }
    }

    #[test]
    fn parses_local_path() {
        let s = parse_source("./relative/path").expect("ok");
        assert!(matches!(s, Source::Local { .. }));
    }

    #[test]
    fn parses_url() {
        let s = parse_source("https://example.com/SKILL.md").expect("ok");
        assert!(matches!(s, Source::Url { .. }));
    }
}
