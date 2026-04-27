//! `skillctl list` — 列出技能。

use serde::Serialize;
use skillctl_core::{Result, Tier};
use skillctl_protocol::{ListResponse, ListedSkill, PROTOCOL_VERSION};
use skillctl_store::{GlobalStore, ProjectStore};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    #[arg(long, conflicts_with = "project")]
    pub global: bool,
    #[arg(long, conflicts_with = "global")]
    pub project: bool,
    #[arg(long)]
    pub lang: Option<String>,
    #[arg(long)]
    pub tag: Option<String>,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let cwd = util::cwd()?;
    let project_first = match (args.project, args.global) {
        (true, _) => true,
        (_, true) => false,
        _ => ProjectStore::discover(&cwd)?.is_some(),
    };

    if project_first {
        list_project(&cwd, args, fmt)
    } else {
        list_global(args, fmt)
    }
}

fn list_project(cwd: &camino::Utf8Path, args: Args, fmt: super::OutputFormat) -> Result<()> {
    let store = ProjectStore::discover(cwd)?
        .ok_or_else(|| skillctl_core::Error::NotAProject(cwd.to_owned()))?;
    let global = GlobalStore::default_open()?;
    util::ensure_trusted(&store.root, &global)?;
    let manifest = store.read_manifest()?;
    let lock = store.read_lock()?;

    let mut skills: Vec<ListedSkill> = Vec::new();
    for (id_str, _spec, tier) in manifest.iter_skills() {
        // 优先用 lock 里的精确事实；缺失时降级用 manifest 信息
        let (version, summary, triggers, languages, tags) =
            if let Some(le) = lock.skills.get(id_str) {
                (
                    le.version.clone(),
                    le.summary.clone(),
                    le.triggers.clone(),
                    le.languages.clone(),
                    le.tags.clone(),
                )
            } else {
                ("*".into(), String::new(), Vec::new(), Vec::new(), Vec::new())
            };
        let id = skillctl_id::SkillId::parse(id_str)?;
        skills.push(ListedSkill {
            id: id_str.clone(),
            name: id.name.as_str().to_owned(),
            namespace: id.namespace.as_str().to_owned(),
            version,
            tier,
            summary,
            languages,
            triggers,
            tags,
        });
    }

    apply_filters(&mut skills, args.lang.as_deref(), args.tag.as_deref());

    let resp = ListResponse {
        protocol: PROTOCOL_VERSION,
        scope: "project".into(),
        count: skills.len(),
        skills,
    };
    if matches!(fmt, super::OutputFormat::Tsv) {
        emit_tsv(&resp.skills);
        return Ok(());
    }
    util::emit(fmt, &resp, |r| {
        for s in &r.skills {
            let tier = match s.tier {
                Tier::Core => "core ",
                Tier::Extra => "extra",
            };
            println!("{tier}  {:<32} {:<10} {}", s.id, s.version, s.summary);
        }
        Ok(())
    })
}

fn list_global(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    let reg = store.read_registry()?;
    let mut skills: Vec<ListedSkill> = reg
        .skills
        .values()
        .map(|e| ListedSkill {
            id: e.id.clone(),
            name: e.name.clone(),
            namespace: e.namespace.clone(),
            version: e.version.clone(),
            tier: Tier::Extra, // 全局视角无 tier 概念，给 extra 占位
            summary: e.summary.clone(),
            languages: e.languages.clone(),
            triggers: e.triggers.clone(),
            tags: e.tags.clone(),
        })
        .collect();
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    apply_filters(&mut skills, args.lang.as_deref(), args.tag.as_deref());

    let resp = ListResponse {
        protocol: PROTOCOL_VERSION,
        scope: "global".into(),
        count: skills.len(),
        skills,
    };
    if matches!(fmt, super::OutputFormat::Tsv) {
        emit_tsv(&resp.skills);
        return Ok(());
    }
    util::emit(fmt, &resp, |r| {
        for s in &r.skills {
            println!("{:<32} {:<10} {}", s.id, s.version, s.summary);
        }
        Ok(())
    })
}

/// Agent 优化的 4 列 TSV：tier / id / summary / triggers
fn emit_tsv(skills: &[skillctl_protocol::ListedSkill]) {
    println!("TIER\tID\tSUMMARY\tTRIGGERS");
    for s in skills {
        let tier = match s.tier {
            Tier::Core => "core",
            Tier::Extra => "extra",
        };
        let triggers = s.triggers.join(",");
        println!(
            "{}\t{}\t{}\t{}",
            tier,
            util::tsv_clean(&s.id),
            util::tsv_clean(&s.summary),
            util::tsv_clean(&triggers),
        );
    }
}

fn apply_filters(skills: &mut Vec<ListedSkill>, lang: Option<&str>, tag: Option<&str>) {
    if let Some(l) = lang {
        skills.retain(|s| s.languages.iter().any(|x| x == l));
    }
    if let Some(t) = tag {
        skills.retain(|s| s.tags.iter().any(|x| x == t));
    }
}

// 让 Serialize 可用于内部派生（避免警告）
#[allow(dead_code)]
#[derive(Serialize)]
struct _Unused;
