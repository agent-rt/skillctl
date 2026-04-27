//! `skillctl validate` — 校验 SKILL.md 文件格式（宽松，仅产生诊断）。

use serde::Serialize;
use skillctl_core::{Error, Result};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// SKILL.md 文件路径（缺省时尝试当前目录的 SKILL.md）。
    pub path: Option<String>,
}

#[derive(Serialize)]
struct ValidateOut {
    success: bool,
    path: String,
    diagnostics: Vec<DiagItem>,
}

#[derive(Serialize)]
struct DiagItem {
    severity: String,
    message: String,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let path_str = args.path.unwrap_or_else(|| "SKILL.md".into());
    let path = camino::Utf8PathBuf::from(&path_str);
    let abs = if path.is_absolute() { path } else { util::cwd()?.join(path) };
    let raw = fs_err::read_to_string(abs.as_std_path())
        .map_err(|e| Error::Io { path: abs.clone(), source: e })?;
    let doc = skillctl_skill::parse_str(&raw)?;
    let diags = skillctl_skill::validate(&doc);
    let success = diags.iter().all(|d| !matches!(d.severity, skillctl_skill::Severity::Error));
    let out = ValidateOut {
        success,
        path: abs.to_string(),
        diagnostics: diags
            .into_iter()
            .map(|d| DiagItem {
                severity: match d.severity {
                    skillctl_skill::Severity::Warning => "warning".into(),
                    skillctl_skill::Severity::Error => "error".into(),
                },
                message: d.message,
            })
            .collect(),
    };
    util::emit(fmt, &out, |o| {
        println!("{}", o.path);
        for d in &o.diagnostics {
            println!("  [{}] {}", d.severity, d.message);
        }
        if o.diagnostics.is_empty() {
            println!("  ok");
        }
        Ok(())
    })
}
