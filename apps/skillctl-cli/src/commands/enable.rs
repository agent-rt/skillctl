//! `skillctl enable` — 写入 AGENTS.md 协议入口块。

use camino::Utf8PathBuf;
use serde::Serialize;
use skillctl_core::Result;

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    #[arg(long, default_value = "AGENTS.md")]
    pub target: String,
    #[arg(long)]
    pub upgrade_block: bool,
}

#[derive(Serialize)]
struct Out<'a> {
    success: bool,
    action: &'static str,
    target: &'a str,
    started_from: Vec<String>,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let cwd = util::cwd()?;
    let target_path: Utf8PathBuf = if std::path::Path::new(&args.target).is_absolute() {
        args.target.clone().into()
    } else {
        cwd.join(&args.target)
    };

    // started_from 来自 manifest（若已存在），否则为空
    let started_from = match skillctl_store::ProjectStore::discover(&cwd)? {
        Some(p) if p.manifest_path().exists() => {
            p.read_manifest().map(|m| m.project.started_from).unwrap_or_default()
        }
        _ => Vec::new(),
    };

    let block = skillctl_agent::default_block(&started_from);
    skillctl_agent::upsert(&target_path, &block)?;

    // 信任门控：用户显式 enable 视为信任
    if let Some(p) = skillctl_store::ProjectStore::discover(&cwd)? {
        let global = skillctl_store::GlobalStore::default_open()?;
        global.trust(&p.root)?;
    }

    let out = Out {
        success: true,
        action: "enable",
        target: &args.target,
        started_from: started_from.clone(),
    };
    util::emit(fmt, &out, |o| {
        println!("enabled skillctl block in {} ({} started_from)", o.target, o.started_from.len());
        Ok(())
    })
}
