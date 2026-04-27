//! `skillctl show <id>` — 输出完整 SKILL.md。

use skillctl_core::{Error, Result};
use skillctl_store::{GlobalStore, ProjectStore};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    pub id: String,
    #[arg(long)]
    pub path: bool,
    #[arg(long)]
    pub global: bool,
}

pub fn run(args: Args, _fmt: super::OutputFormat) -> Result<()> {
    let cwd = util::cwd()?;
    let global = GlobalStore::default_open()?;

    // 选择解析作用域
    let (id_str, version) = if args.global {
        let id = global.resolve_short(&args.id)?;
        let reg = global.read_registry()?;
        let entry =
            reg.skills.get(&id.to_string()).ok_or_else(|| Error::SkillNotFound(args.id.clone()))?;
        (id.to_string(), entry.version.clone())
    } else {
        let store = ProjectStore::discover(&cwd)?.ok_or_else(|| Error::NotAProject(cwd.clone()))?;
        util::ensure_trusted(&store.root, &global)?;
        let manifest = store.read_manifest()?;
        let lock = store.read_lock()?;
        let (id_str, _) = store.resolve_skill(&manifest, &args.id)?;
        let entry = lock
            .skills
            .get(&id_str)
            .ok_or_else(|| Error::other(format!("lock entry missing for {id_str}; run restore")))?;
        (id_str, entry.version.clone())
    };

    let id = skillctl_id::SkillId::parse(&id_str)?;
    let path = global.skill_md(&id, &version);
    if args.path {
        println!("{path}");
        return Ok(());
    }
    let raw = fs_err::read_to_string(path.as_std_path())
        .map_err(|e| Error::Io { path: path.clone(), source: e })?;
    print!("{raw}");
    Ok(())
}
