//! `skillctl remove` — 从全局仓库移除技能。

use serde::Serialize;
use skillctl_core::{Error, Result};
use skillctl_id::SkillId;
use skillctl_store::GlobalStore;

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// 技能 ID（`namespace/name` 或裸名）。可选附 `@version`。
    pub id: String,
    #[arg(long)]
    pub all_versions: bool,
    #[arg(long)]
    pub force: bool,
}

#[derive(Serialize)]
struct Out {
    success: bool,
    action: &'static str,
    removed: Vec<String>,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    let mut reg = store.read_registry()?;

    let (id_part, version_filter) = match args.id.split_once('@') {
        Some((id, v)) => (id.to_owned(), Some(v.to_owned())),
        None => (args.id.clone(), None),
    };

    let resolved = store.resolve_short(&id_part)?;
    let id_str = resolved.to_string();

    let entry = reg.skills.get(&id_str).ok_or_else(|| Error::SkillNotFound(id_str.clone()))?;

    if let Some(ver) = &version_filter {
        if entry.version != *ver {
            return Err(Error::SkillNotFound(format!("{id_str}@{ver}")));
        }
    }

    // 物理移除目录
    let dir = if args.all_versions {
        let parent = store
            .skill_dir(&resolved, "")
            .parent()
            .map(camino::Utf8PathBuf::from)
            .ok_or_else(|| Error::other("invalid skill dir"))?;
        parent
    } else {
        store.skill_dir(&resolved, &entry.version)
    };
    if dir.exists() {
        fs_err::remove_dir_all(dir.as_std_path())
            .map_err(|e| Error::Io { path: dir, source: e })?;
    }

    reg.skills.remove(&id_str);
    store.write_registry(&reg)?;

    let out = Out { success: true, action: "remove", removed: vec![id_str.clone()] };
    util::emit(fmt, &out, |o| {
        for r in &o.removed {
            println!("removed {r}");
        }
        Ok(())
    })
}

#[allow(dead_code)]
fn _ensure_use(_: SkillId) {}
