//! `skillctl path <id>` — 输出 SKILL.md 绝对路径。

use skillctl_core::{Error, Result};
use skillctl_store::{GlobalStore, ProjectStore};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    pub id: String,
}

pub fn run(args: Args, _fmt: super::OutputFormat) -> Result<()> {
    let cwd = util::cwd()?;
    let global = GlobalStore::default_open()?;
    let store = ProjectStore::discover(&cwd)?.ok_or_else(|| Error::NotAProject(cwd.clone()))?;
    util::ensure_trusted(&store.root, &global)?;
    let manifest = store.read_manifest()?;
    let lock = store.read_lock()?;
    let (id_str, _) = store.resolve_skill(&manifest, &args.id)?;
    let entry = lock
        .skills
        .get(&id_str)
        .ok_or_else(|| Error::other(format!("lock entry missing for {id_str}")))?;
    let id = skillctl_id::SkillId::parse(&id_str)?;
    println!("{}", global.skill_md(&id, &entry.version));
    Ok(())
}
