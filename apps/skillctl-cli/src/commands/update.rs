//! `skillctl update` — 重新拉取技能源，刷新版本与校验和。
//!
//! MVP：对每个目标技能从其 registry 记录的来源重新 fetch，
//! 解析 SKILL.md，更新 registry 中的 version/commit/checksum。
//! 不做 semver 范围解析（Phase 3）。

use serde::Serialize;
use skillctl_core::{Error, Result};
use skillctl_id::{SkillId, SkillName};
use skillctl_source::Source;
use skillctl_store::{sha256_hex, GlobalStore};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    pub id: Option<String>,
    #[arg(long)]
    pub global: bool,
    #[arg(long)]
    pub project: bool,
    #[arg(long)]
    pub breaking: bool,
}

#[derive(Serialize)]
struct UpdateOut {
    success: bool,
    action: &'static str,
    updated: Vec<UpdatedItem>,
}

#[derive(Serialize)]
struct UpdatedItem {
    id: String,
    from_version: String,
    to_version: String,
    from_commit: Option<String>,
    to_commit: Option<String>,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    let mut reg = store.read_registry()?;

    let targets: Vec<String> = match args.id.as_deref() {
        Some(q) => {
            let id = store.resolve_short(q)?;
            vec![id.to_string()]
        }
        None => reg.skills.keys().cloned().collect(),
    };

    let mut updated = Vec::new();
    for id_str in targets {
        let entry = match reg.skills.get(&id_str) {
            Some(e) => e.clone(),
            None => continue,
        };
        let source = match (entry.source.as_str(), entry.local_path.as_ref(), entry.repo.as_ref()) {
            ("local", Some(p), _) => Source::Local { path: p.clone() },
            ("github", _, Some(repo)) => Source::Github {
                repo: repo.clone(),
                path: None,
                // 无 reference 表示拉默认分支最新
                reference: None,
            },
            _ => continue,
        };

        let fetched = skillctl_source::fetch(&source)?;
        let skill_md = if fetched.root.is_file() {
            fetched.root.clone()
        } else {
            fetched.root.join("SKILL.md")
        };
        let raw = fs_err::read_to_string(skill_md.as_std_path())
            .map_err(|e| Error::Io { path: skill_md.clone(), source: e })?;
        let doc = skillctl_skill::parse_str(&raw)?;
        let new_version = doc.frontmatter.version.clone();
        let new_checksum = sha256_hex(raw.as_bytes());

        // 拷贝新版本到全局仓库
        let ns_for_id = match doc.frontmatter.metadata.namespace.clone() {
            Some(n) => n,
            None => skillctl_id::Namespace::parse(&entry.namespace)?,
        };
        let id = SkillId { namespace: ns_for_id, name: SkillName::parse(&entry.name)? };
        let target_dir = store.skill_dir(&id, &new_version);
        fs_err::create_dir_all(target_dir.as_std_path())
            .map_err(|e| Error::Io { path: target_dir.clone(), source: e })?;
        fs_err::write(store.skill_md(&id, &new_version).as_std_path(), &raw)
            .map_err(|e| Error::Io { path: store.skill_md(&id, &new_version), source: e })?;

        if entry.version != new_version || entry.checksum != new_checksum {
            updated.push(UpdatedItem {
                id: id_str.clone(),
                from_version: entry.version.clone(),
                to_version: new_version.clone(),
                from_commit: entry.commit.clone(),
                to_commit: fetched.commit.clone(),
            });
            let mut new_entry = entry.clone();
            new_entry.version = new_version;
            new_entry.checksum = new_checksum;
            new_entry.commit = fetched.commit.clone();
            new_entry.summary = skillctl_skill::derive_summary(&doc.frontmatter);
            reg.skills.insert(id_str, new_entry);
        }
    }

    if !updated.is_empty() {
        store.write_registry(&reg)?;
    }
    let _ = args.breaking; // placeholder for semver-aware updates

    let out = UpdateOut { success: true, action: "update", updated };
    util::emit(fmt, &out, |o| {
        println!("{} updates", o.updated.len());
        for u in &o.updated {
            println!("  {} {} → {}", u.id, u.from_version, u.to_version);
        }
        Ok(())
    })
}
