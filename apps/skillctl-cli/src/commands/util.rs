//! 子命令共享辅助。

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;
use skillctl_core::{Error, Result};
use skillctl_store::{GlobalStore, ProjectStore};

use super::OutputFormat;

/// 取当前目录为 UTF-8 路径。
pub fn cwd() -> Result<Utf8PathBuf> {
    let std = std::env::current_dir().map_err(|e| Error::other(format!("cwd: {e}")))?;
    Utf8PathBuf::from_path_buf(std).map_err(|p| Error::other(format!("non-utf8 cwd: {p:?}")))
}

/// 在 cwd 之上发现项目根；不存在时报错 `not_a_project`。
pub fn must_find_project() -> Result<ProjectStore> {
    let here = cwd()?;
    ProjectStore::discover(&here)?.ok_or_else(|| Error::NotAProject(here))
}

/// 信任门控：仅当环境变量 `SKILLCTL_TRUST_GATE=1` 时启用检查。
///
/// 启用时，未在信任清单中的项目对只读 Agent 命令返回 `untrusted_project`。
/// 默认关闭以避免现有用户升级时的回归；Agent harness / 安全敏感场景可显式开启。
pub fn ensure_trusted(project_root: &Utf8Path, global: &GlobalStore) -> Result<()> {
    if !trust_gate_enabled() {
        return Ok(());
    }
    if global.is_trusted(project_root)? {
        return Ok(());
    }
    Err(Error::UntrustedProject(project_root.to_owned()))
}

#[must_use]
pub fn trust_gate_enabled() -> bool {
    matches!(std::env::var("SKILLCTL_TRUST_GATE").as_deref(), Ok("1") | Ok("true"))
}

/// 输出 JSON 或 human 文本。
///
/// `Tsv` 仅在显式提供 TSV 渲染的命令上有效；对未实现 TSV 的命令，回退到 Human。
pub fn emit<T: Serialize>(
    fmt: OutputFormat,
    value: &T,
    human: impl FnOnce(&T) -> Result<()>,
) -> Result<()> {
    match fmt {
        OutputFormat::Json => {
            let s = serde_json::to_string_pretty(value)
                .map_err(|e| Error::other(format!("json serialize: {e}")))?;
            println!("{s}");
            Ok(())
        }
        OutputFormat::Human | OutputFormat::Tsv => human(value),
    }
}

/// TSV 列值清洗：替换 tab 与换行为单空格。
#[must_use]
pub fn tsv_clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}
