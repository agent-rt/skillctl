//! `skillctl use` — 微调当前项目启用的技能列表。

use serde::Serialize;
use skillctl_core::{Error, Result, Tier};
use skillctl_id::SkillId;
use skillctl_profile::SkillSpec;
use skillctl_store::{GlobalStore, LockEntry};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    pub ids: Vec<String>,
    #[arg(long, conflicts_with_all = ["remove", "clear"])]
    pub add: bool,
    #[arg(long, conflicts_with_all = ["add", "clear"])]
    pub remove: bool,
    #[arg(long, conflicts_with_all = ["add", "remove"])]
    pub clear: bool,
    /// 标记新加入的技能为 core 而非 extra。
    #[arg(long)]
    pub core: bool,
}

#[derive(Serialize)]
struct Out {
    success: bool,
    action: &'static str,
    enabled: Vec<String>,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    // 无 ids 且非 clear 模式时，进入 TUI 多选
    if args.ids.is_empty() && !args.clear && !args.remove {
        return run_tui(args.core, fmt);
    }
    run_cli(args, fmt)
}

fn run_cli(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let store = util::must_find_project()?;
    let mut manifest = store.read_manifest()?;
    let mut lock = store.read_lock()?;
    let global = GlobalStore::default_open()?;
    let registry = global.read_registry()?;

    if args.clear {
        manifest.skills.core.clear();
        manifest.skills.extra.clear();
        lock.skills.clear();
    } else if args.remove {
        for raw in &args.ids {
            let id = resolve_arg_id(raw, &registry)?;
            manifest.skills.core.remove(&id);
            manifest.skills.extra.remove(&id);
            lock.skills.remove(&id);
        }
    } else {
        let target_tier = if args.core { Tier::Core } else { Tier::Extra };
        for raw in &args.ids {
            let id = resolve_arg_id(raw, &registry)?;
            apply_add(&mut manifest, &mut lock, &registry, &id, target_tier)?;
        }
    }

    store.write_manifest(&manifest)?;
    store.write_lock(&lock)?;
    global.trust(&store.root)?;

    let enabled: Vec<String> =
        manifest.skills.core.keys().chain(manifest.skills.extra.keys()).cloned().collect();
    let out = Out { success: true, action: "use", enabled };
    util::emit(fmt, &out, |o| {
        println!("{} skills enabled", o.enabled.len());
        Ok(())
    })
}

#[cfg(feature = "tui")]
fn run_tui(_default_core: bool, fmt: super::OutputFormat) -> Result<()> {
    use skillctl_tui::Candidate;

    let store = util::must_find_project()?;
    let manifest = store.read_manifest()?;
    let global = GlobalStore::default_open()?;
    let registry = global.read_registry()?;

    if registry.skills.is_empty() {
        return Err(Error::other("no skills in global store; run `skillctl add` first"));
    }

    // 构造候选项：当前已启用的标记为 enabled
    let mut candidates: Vec<Candidate> = registry
        .skills
        .values()
        .filter_map(|e| {
            let id_str = e.id.clone();
            let id = SkillId::parse(&id_str).ok()?;
            let tier = if manifest.skills.core.contains_key(&id_str) {
                Some(Tier::Core)
            } else if manifest.skills.extra.contains_key(&id_str) {
                Some(Tier::Extra)
            } else {
                None
            };
            Some(Candidate {
                id,
                summary: e.summary.clone(),
                languages: vec![],
                tags: vec![],
                enabled: tier.is_some(),
                tier,
            })
        })
        .collect();
    candidates.sort_by(|a, b| a.id.to_string().cmp(&b.id.to_string()));

    let (core_ids, extra_ids) = skillctl_tui::select(candidates)?;

    // 信任门控
    global.trust(&store.root)?;

    // 写回 manifest + lock
    let mut manifest = store.read_manifest()?;
    let mut lock = store.read_lock()?;
    manifest.skills.core.clear();
    manifest.skills.extra.clear();
    lock.skills.clear();

    for id in &core_ids {
        apply_add(&mut manifest, &mut lock, &registry, &id.to_string(), Tier::Core)?;
    }
    for id in &extra_ids {
        apply_add(&mut manifest, &mut lock, &registry, &id.to_string(), Tier::Extra)?;
    }
    store.write_manifest(&manifest)?;
    store.write_lock(&lock)?;

    let enabled: Vec<String> =
        core_ids.iter().chain(extra_ids.iter()).map(|id| id.to_string()).collect();
    let out = Out { success: true, action: "use", enabled };
    util::emit(fmt, &out, |o| {
        println!("{} skills enabled", o.enabled.len());
        Ok(())
    })
}

#[cfg(not(feature = "tui"))]
fn run_tui(_default_core: bool, _fmt: super::OutputFormat) -> Result<()> {
    Err(Error::other("TUI not available in this build; pass skill ids explicitly"))
}

fn apply_add(
    manifest: &mut skillctl_store::Manifest,
    lock: &mut skillctl_store::Lock,
    registry: &skillctl_store::RegistryLock,
    id: &str,
    target_tier: Tier,
) -> Result<()> {
    let entry = registry
        .skills
        .get(id)
        .ok_or_else(|| Error::SkillNotFound(format!("{id} (run `skillctl add` first)")))?;
    let spec = SkillSpec::Version(entry.version.clone());
    manifest.skills.core.remove(id);
    manifest.skills.extra.remove(id);
    match target_tier {
        Tier::Core => manifest.skills.core.insert(id.to_owned(), spec),
        Tier::Extra => manifest.skills.extra.insert(id.to_owned(), spec),
    };
    lock.skills.insert(
        id.to_owned(),
        LockEntry {
            id: entry.id.clone(),
            namespace: entry.namespace.clone(),
            name: entry.name.clone(),
            version: entry.version.clone(),
            tier: target_tier,
            source: "global".into(),
            global_ref: Some(format!("{}@{}", entry.id, entry.version)),
            checksum: entry.checksum.clone(),
            summary: entry.summary.clone(),
            triggers: entry.triggers.clone(),
            languages: entry.languages.clone(),
            tags: entry.tags.clone(),
        },
    );
    Ok(())
}

fn resolve_arg_id(raw: &str, registry: &skillctl_store::RegistryLock) -> Result<String> {
    if SkillId::parse(raw).is_ok() {
        return Ok(raw.to_owned());
    }
    let candidates: Vec<&skillctl_store::GlobalEntry> =
        registry.skills.values().filter(|e| e.name == raw).collect();
    match candidates.len() {
        0 => Err(Error::SkillNotFound(raw.into())),
        1 => Ok(candidates[0].id.clone()),
        _ => Err(Error::AmbiguousSkillName {
            name: raw.into(),
            matches: candidates.iter().map(|e| e.id.clone()).collect(),
        }),
    }
}
