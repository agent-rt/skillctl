//! `skillctl doctor` — 检查技能外部依赖（PATH 上的二进制）。

use serde::Serialize;
use skillctl_core::{Error, Result};
use skillctl_id::SkillId;
use skillctl_store::{GlobalStore, ProjectStore};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    pub id: Option<String>,
    #[arg(long)]
    pub project: bool,
}

#[derive(Serialize)]
struct DoctorOut {
    success: bool,
    skills: Vec<SkillCheck>,
}

#[derive(Serialize)]
struct SkillCheck {
    id: String,
    checks: Vec<CheckItem>,
}

#[derive(Serialize)]
struct CheckItem {
    r#type: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    found: Option<String>,
    ok: bool,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let cwd = util::cwd()?;
    let global = GlobalStore::default_open()?;
    let reg = global.read_registry()?;

    let target_ids: Vec<String> = if let Some(q) = &args.id {
        vec![global.resolve_short(q)?.to_string()]
    } else if args.project {
        let proj = ProjectStore::discover(&cwd)?.ok_or_else(|| Error::NotAProject(cwd))?;
        proj.read_manifest()?
            .skills
            .core
            .keys()
            .chain(proj.read_manifest()?.skills.extra.keys())
            .cloned()
            .collect()
    } else if let Some(proj) = ProjectStore::discover(&cwd)? {
        let m = proj.read_manifest()?;
        m.skills.core.keys().chain(m.skills.extra.keys()).cloned().collect()
    } else {
        reg.skills.keys().cloned().collect()
    };

    let mut all_ok = true;
    let mut skills = Vec::new();
    for id_str in target_ids {
        let entry = match reg.skills.get(&id_str) {
            Some(e) => e,
            None => continue,
        };
        let id = SkillId::parse(&id_str)?;
        let md = global.skill_md(&id, &entry.version);
        let raw = fs_err::read_to_string(md.as_std_path())
            .map_err(|e| Error::Io { path: md.clone(), source: e })?;
        let doc = skillctl_skill::parse_str(&raw)?;
        let mut checks = Vec::new();
        for bin in &doc.frontmatter.metadata.requires.binaries {
            let found = which::which(&bin.name).ok();
            let ok = found.is_some();
            if !ok {
                all_ok = false;
            }
            checks.push(CheckItem {
                r#type: "binary".into(),
                name: bin.name.clone(),
                required: bin.version.clone(),
                found: found.map(|p| p.to_string_lossy().into_owned()),
                ok,
            });
        }
        for env_var in &doc.frontmatter.metadata.requires.env {
            let found = std::env::var(env_var).ok();
            let ok = found.is_some();
            if !ok {
                all_ok = false;
            }
            checks.push(CheckItem {
                r#type: "env".into(),
                name: env_var.clone(),
                required: None,
                found: found.map(|_| "set".into()),
                ok,
            });
        }
        skills.push(SkillCheck { id: id_str, checks });
    }

    let out = DoctorOut { success: all_ok, skills };
    util::emit(fmt, &out, |o| {
        for s in &o.skills {
            println!("{}", s.id);
            for c in &s.checks {
                let mark = if c.ok { "✓" } else { "✗" };
                println!(
                    "  {mark} {} {} {}",
                    c.r#type,
                    c.name,
                    c.required.as_deref().unwrap_or("")
                );
            }
        }
        Ok(())
    })
}
