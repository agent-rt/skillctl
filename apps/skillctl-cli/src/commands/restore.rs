//! `skillctl restore` — 按 lock 校验/恢复技能。
//!
//! 在项目目录内：检查 lock 中每个技能在全局仓库存在且 checksum 一致；缺失则从 registry 记录的来源重拉。
//! 在全局：根据 registry 重新从源拉取所有技能。

use serde::Serialize;
use skillctl_core::{Error, Result};
use skillctl_id::SkillId;
use skillctl_source::Source;
use skillctl_store::{sha256_hex, GlobalStore, ProjectStore};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    #[arg(long)]
    pub global: bool,
    #[arg(long)]
    pub project: bool,
}

#[derive(Serialize)]
struct Out {
    success: bool,
    action: &'static str,
    restored: Vec<String>,
    verified: Vec<String>,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let cwd = util::cwd()?;
    let store = GlobalStore::default_open()?;
    let project_first = match (args.project, args.global) {
        (true, _) => true,
        (_, true) => false,
        _ => ProjectStore::discover(&cwd)?.is_some(),
    };

    let mut restored = Vec::new();
    let mut verified = Vec::new();
    let reg = store.read_registry()?;

    let ids: Vec<String> = if project_first {
        let proj = ProjectStore::discover(&cwd)?.ok_or_else(|| Error::NotAProject(cwd))?;
        proj.read_lock()?.skills.into_keys().collect()
    } else {
        reg.skills.keys().cloned().collect()
    };

    for id_str in ids {
        let entry = match reg.skills.get(&id_str) {
            Some(e) => e.clone(),
            None => return Err(Error::SkillNotFound(format!("{id_str} not in global registry"))),
        };
        let id = SkillId::parse(&id_str)?;
        let md_path = store.skill_md(&id, &entry.version);
        if md_path.exists() {
            let raw = fs_err::read_to_string(md_path.as_std_path())
                .map_err(|e| Error::Io { path: md_path.clone(), source: e })?;
            if sha256_hex(raw.as_bytes()) == entry.checksum {
                verified.push(id_str);
                continue;
            }
        }
        // 重新拉取
        let source = match (entry.source.as_str(), entry.local_path.as_ref(), entry.repo.as_ref()) {
            ("local", Some(p), _) => Source::Local { path: p.clone() },
            ("github", _, Some(repo)) => {
                Source::Github { repo: repo.clone(), path: None, reference: entry.commit.clone() }
            }
            _ => {
                return Err(Error::other(format!(
                    "cannot restore {id_str}: unknown source `{}`",
                    entry.source
                )))
            }
        };
        let fetched = skillctl_source::fetch(&source)?;
        let skill_md = if fetched.root.is_file() {
            fetched.root.clone()
        } else {
            fetched.root.join("SKILL.md")
        };
        let raw = fs_err::read_to_string(skill_md.as_std_path())
            .map_err(|e| Error::Io { path: skill_md.clone(), source: e })?;
        let target_dir = store.skill_dir(&id, &entry.version);
        fs_err::create_dir_all(target_dir.as_std_path())
            .map_err(|e| Error::Io { path: target_dir.clone(), source: e })?;
        fs_err::write(md_path.as_std_path(), &raw)
            .map_err(|e| Error::Io { path: md_path.clone(), source: e })?;
        restored.push(id_str);
    }

    let out = Out { success: true, action: "restore", restored, verified };
    util::emit(fmt, &out, |o| {
        println!("verified={}, restored={}", o.verified.len(), o.restored.len());
        Ok(())
    })
}
