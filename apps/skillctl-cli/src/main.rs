//! skillctl 二进制入口。
//!
//! 命令路由在此聚合；具体业务逻辑在各 crate 的 lib 中实现。

#![forbid(unsafe_code)]
#![allow(clippy::print_stdout, clippy::print_stderr)] // CLI 入口允许直接打印

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod commands;

#[derive(Debug, Parser)]
#[command(
    name = "skillctl",
    version,
    about = "Profile-driven Agent skill manager and protocol gateway",
    long_about = None,
)]
struct Cli {
    /// 输出格式：human / json。json 用于 Agent 与脚本。
    #[arg(long, global = true, default_value = "human")]
    format: OutputFormat,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    /// 人类视图（彩色/对齐表格）。
    Human,
    /// 完整 JSON wire schema，适合工具与脚本。
    Json,
    /// 紧凑 TSV，适合 Agent 直接消费。仅在适用命令上有效（list / profile list 等）。
    Tsv,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 起手当前项目（人面主入口）。
    Init(commands::init::Args),

    /// 全局 profile 管理。
    #[command(subcommand)]
    Profile(commands::profile::ProfileCmd),

    /// 添加技能到全局仓库。
    Add(commands::add::Args),
    /// 从全局仓库移除技能。
    Remove(commands::remove::Args),

    /// 微调当前项目启用的技能列表（含 TUI）。
    Use(commands::use_cmd::Args),

    /// 写入 AGENTS.md 协议入口块。
    Enable(commands::enable::Args),
    /// 移除 AGENTS.md 协议入口块。
    Disable(commands::disable::Args),

    /// 列出技能（默认按当前目录范围）。
    List(commands::list::Args),
    /// 输出技能结构化能力（Tier 2a）。
    Describe(commands::describe::Args),
    /// 输出完整 SKILL.md（Tier 2b）。
    Show(commands::show::Args),
    /// 输出 SKILL.md 的绝对路径。
    Path(commands::path::Args),

    /// 按 lock 恢复技能。
    Restore(commands::restore::Args),
    /// 更新技能版本。
    Update(commands::update::Args),

    /// 检查技能外部依赖。
    Doctor(commands::doctor::Args),
    /// 校验 SKILL.md 格式。
    Validate(commands::validate::Args),
    /// 校验 checksum。
    Verify(commands::verify::Args),
    /// 审计技能元数据/脚本/依赖/风险。
    Inspect(commands::inspect::Args),

    /// 项目信任清单管理。
    Trust(commands::trust::Args),
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let format = cli.format;
    match commands::dispatch(cli.command, format) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            emit_error(format, &err);
            ExitCode::FAILURE
        }
    }
}

fn emit_error(format: OutputFormat, err: &skillctl_core::Error) {
    use skillctl_core::Error;
    use skillctl_protocol::{ErrorEnvelope, PROTOCOL_VERSION};

    let (code, hint, matches): (&str, Option<String>, Vec<String>) = match err {
        Error::SkillNotFound(_) => ("skill_not_found", None, vec![]),
        Error::ProfileNotFound(_) => ("profile_not_found", None, vec![]),
        Error::AmbiguousSkillName { matches, .. } => (
            "ambiguous_skill_name",
            Some("Use full id, e.g. namespace/name".into()),
            matches.clone(),
        ),
        Error::NotAProject(_) => ("not_a_project", None, vec![]),
        Error::UntrustedProject(_) => ("untrusted_project", None, vec![]),
        Error::DependencyMissing(_) => ("dependency_missing", None, vec![]),
        Error::InvalidSkillId(_) => ("invalid_skill_id", None, vec![]),
        Error::InvalidSkill(_) => ("invalid_skill", None, vec![]),
        Error::InvalidProfile(_) => ("invalid_profile", None, vec![]),
        Error::ChecksumMismatch { .. } => ("checksum_mismatch", None, vec![]),
        Error::Io { .. } => ("io_error", None, vec![]),
        Error::Source(_) => ("source_error", None, vec![]),
        Error::Other(_) => ("error", None, vec![]),
    };

    match format {
        OutputFormat::Json => {
            let envelope = ErrorEnvelope {
                protocol: PROTOCOL_VERSION,
                success: false,
                error: code.to_owned(),
                hint: hint.or_else(|| Some(err.to_string())),
                matches,
            };
            // 即使错误也输出合法 JSON 到 stdout（PROTOCOL §3.5）
            match serde_json::to_string_pretty(&envelope) {
                Ok(s) => println!("{s}"),
                Err(_) => eprintln!("error: {err}"),
            }
        }
        OutputFormat::Tsv => {
            // TSV 模式错误：单行 ERROR\t<code>\t<hint>，便于脚本解析
            let hint_str = hint.unwrap_or_else(|| err.to_string());
            println!("ERROR\t{code}\t{}", hint_str.replace(['\t', '\n', '\r'], " "));
        }
        OutputFormat::Human => {
            eprintln!("error: {err}");
        }
    }
}
