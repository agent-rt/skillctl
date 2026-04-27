//! `skillctl init [profile...] [--link]` — 起手项目（人面主入口）。

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use serde::Serialize;
use skillctl_core::{Error, Result, Tier};
use skillctl_id::SkillId;
use skillctl_profile::{merge, Profile, SkillSpec, SkillsTable};
use skillctl_store::{
    AgentConfig, GlobalStore, Lock, LockEntry, Manifest, ManifestSkills, ProjectMeta, ProjectStore,
};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    pub profiles: Vec<String>,
    #[arg(long)]
    pub link: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(long, conflicts_with_all = ["profiles", "link"])]
    pub suggest: bool,
}

#[derive(Serialize)]
struct InitOut {
    success: bool,
    action: &'static str,
    project: String,
    started_from: Vec<String>,
    linked_to: Vec<String>,
    skills: SkillsBuckets,
    updated: Vec<Utf8PathBuf>,
}

#[derive(Serialize)]
struct SkillsBuckets {
    core: Vec<String>,
    extra: Vec<String>,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    if args.suggest {
        return suggest(fmt);
    }

    let cwd = util::cwd()?;
    let store = ProjectStore::at(cwd.clone());

    if store.manifest_path().exists() && !args.force {
        return Err(Error::other(format!(
            "skillctl.toml already exists at {}; use --force to overwrite",
            store.manifest_path()
        )));
    }

    // 项目名：cwd basename
    let project_name =
        cwd.file_name().map(str::to_owned).unwrap_or_else(|| "skillctl-project".to_owned());

    // 加载 profiles
    let global = GlobalStore::default_open()?;
    global.ensure_dirs()?;
    let loaded = load_profiles(&global, &args.profiles)?;
    let merged: SkillsTable = merge(&loaded);

    // 解析所有 skill spec → 全局 registry 中的 LockEntry
    let registry = global.read_registry()?;
    let mut lock = Lock::default();
    let mut core_ids = Vec::new();
    let mut extra_ids = Vec::new();
    let mut manifest_skills = ManifestSkills::default();

    resolve_bucket(
        &merged.core,
        &mut manifest_skills.core,
        &mut core_ids,
        &mut lock,
        &registry,
        Tier::Core,
    )?;
    resolve_bucket(
        &merged.extra,
        &mut manifest_skills.extra,
        &mut extra_ids,
        &mut lock,
        &registry,
        Tier::Extra,
    )?;

    let manifest = Manifest {
        project: ProjectMeta {
            name: project_name.clone(),
            started_from: args.profiles.clone(),
            linked_to: if args.link { args.profiles.clone() } else { Vec::new() },
        },
        skills: manifest_skills,
        agent: AgentConfig { enabled: true, target: "AGENTS.md".into() },
    };

    store.write_manifest(&manifest)?;
    store.write_lock(&lock)?;

    // 写 AGENTS.md 入口块
    let block = skillctl_agent::default_block(&args.profiles);
    skillctl_agent::upsert(&store.agents_md_path(), &block)?;

    // init 视为用户显式起手，自动加入信任清单
    global.trust(&store.root)?;

    let out = InitOut {
        success: true,
        action: "init",
        project: project_name,
        started_from: args.profiles.clone(),
        linked_to: if args.link { args.profiles.clone() } else { Vec::new() },
        skills: SkillsBuckets { core: core_ids, extra: extra_ids },
        updated: vec![store.manifest_path(), store.lock_path(), store.agents_md_path()],
    };
    util::emit(fmt, &out, |o| {
        println!(
            "initialized {} ({} core, {} extra) from {:?}",
            o.project,
            o.skills.core.len(),
            o.skills.extra.len(),
            o.started_from
        );
        Ok(())
    })
}

fn load_profiles(store: &GlobalStore, names: &[String]) -> Result<Vec<Profile>> {
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let path = store.profile_path(name);
        if !path.exists() {
            return Err(Error::ProfileNotFound(name.clone()));
        }
        let raw = fs_err::read_to_string(path.as_std_path())
            .map_err(|e| Error::Io { path: path.clone(), source: e })?;
        out.push(skillctl_profile::parse_str(&raw)?);
    }
    Ok(out)
}

fn resolve_bucket(
    src: &BTreeMap<String, SkillSpec>,
    dst_manifest: &mut BTreeMap<String, SkillSpec>,
    out_ids: &mut Vec<String>,
    lock: &mut Lock,
    registry: &skillctl_store::RegistryLock,
    tier: Tier,
) -> Result<()> {
    for (id_str, spec) in src {
        // ID 校验：要么是合法 namespace/name，要么是裸 name → 查询 registry 唯一性
        let resolved_id = if SkillId::parse(id_str).is_ok() {
            id_str.clone()
        } else {
            // 裸名解析
            let candidates: Vec<&skillctl_store::GlobalEntry> =
                registry.skills.values().filter(|e| e.name == *id_str).collect();
            match candidates.len() {
                0 => {
                    return Err(Error::SkillNotFound(format!(
                        "{id_str} (not in global store; run `skillctl add` first)"
                    )))
                }
                1 => candidates[0].id.clone(),
                _ => {
                    return Err(Error::AmbiguousSkillName {
                        name: id_str.clone(),
                        matches: candidates.iter().map(|e| e.id.clone()).collect(),
                    })
                }
            }
        };

        let entry = registry.skills.get(&resolved_id).ok_or_else(|| {
            Error::SkillNotFound(format!(
                "{resolved_id} (not in global store; run `skillctl add` first)"
            ))
        })?;

        // 版本约束验证（MVP：仅接受 "*" 或精确匹配）
        let req = spec.version_req();
        if req != "*" && req != entry.version {
            return Err(Error::other(format!(
                "version mismatch for {resolved_id}: requested `{req}`, installed `{}`",
                entry.version
            )));
        }

        dst_manifest.insert(resolved_id.clone(), spec.clone());
        out_ids.push(resolved_id.clone());
        lock.skills.insert(
            resolved_id.clone(),
            LockEntry {
                id: entry.id.clone(),
                namespace: entry.namespace.clone(),
                name: entry.name.clone(),
                version: entry.version.clone(),
                tier,
                source: "global".into(),
                global_ref: Some(format!("{}@{}", entry.id, entry.version)),
                checksum: entry.checksum.clone(),
                summary: entry.summary.clone(),
                triggers: entry.triggers.clone(),
                languages: entry.languages.clone(),
                tags: entry.tags.clone(),
            },
        );
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct SuggestOut {
    success: bool,
    action: &'static str,
    detected: Vec<&'static str>,
    profiles_available: Vec<String>,
    suggested_profiles: Vec<String>,
}

fn suggest(fmt: super::OutputFormat) -> Result<()> {
    let cwd = util::cwd()?;
    let hint = skillctl_detect::detect(&cwd);
    let detected = hint.tags();

    // 列出全局可用 profile
    let global = GlobalStore::default_open()?;
    let mut profiles_available = Vec::new();
    let mut suggested = Vec::new();
    let dir = global.profiles_dir();
    if dir.exists() {
        for entry in fs_err::read_dir(dir.as_std_path())
            .map_err(|e| Error::other(format!("readdir: {e}")))?
        {
            let Ok(entry) = entry else { continue };
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                profiles_available.push(stem.to_owned());
                // 简单匹配：profile 名与栈 tag 有交集即建议
                let lower = stem.to_ascii_lowercase();
                if detected.iter().any(|t| lower.contains(t)) {
                    suggested.push(stem.to_owned());
                }
            }
        }
    }
    profiles_available.sort();
    suggested.sort();

    let out = SuggestOut {
        success: true,
        action: "init.suggest",
        detected,
        profiles_available,
        suggested_profiles: suggested,
    };
    util::emit(fmt, &out, |o| {
        println!("detected stacks: {}", o.detected.join(", "));
        if !o.suggested_profiles.is_empty() {
            println!("suggested profiles: {}", o.suggested_profiles.join(", "));
        } else {
            println!("no matching profile (available: {})", o.profiles_available.join(", "));
        }
        println!("\nrun: skillctl init <profile>  to apply");
        Ok(())
    })
}
