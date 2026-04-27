//! `skillctl describe <id>` — 输出技能结构化能力（Tier 2a）。

use skillctl_core::{Error, Result};
use skillctl_protocol::{DescribeResponse, PROTOCOL_VERSION};
use skillctl_store::{GlobalStore, ProjectStore};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    pub id: String,
    #[arg(long, conflicts_with = "detail")]
    pub brief: bool,
    #[arg(long)]
    pub detail: bool,
    #[arg(long)]
    pub global: bool,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let cwd = util::cwd()?;
    let global = GlobalStore::default_open()?;

    let (id_str, version, tier) = if args.global {
        let id = global.resolve_short(&args.id)?;
        let reg = global.read_registry()?;
        let entry =
            reg.skills.get(&id.to_string()).ok_or_else(|| Error::SkillNotFound(args.id.clone()))?;
        (id.to_string(), entry.version.clone(), skillctl_core::Tier::Extra)
    } else {
        let store = ProjectStore::discover(&cwd)?.ok_or_else(|| Error::NotAProject(cwd.clone()))?;
        util::ensure_trusted(&store.root, &global)?;
        let manifest = store.read_manifest()?;
        let lock = store.read_lock()?;
        let (id_str, tier) = store.resolve_skill(&manifest, &args.id)?;
        let entry = lock
            .skills
            .get(&id_str)
            .ok_or_else(|| Error::other(format!("lock entry missing for {id_str}")))?;
        (id_str, entry.version.clone(), tier)
    };

    let id = skillctl_id::SkillId::parse(&id_str)?;
    let path = global.skill_md(&id, &version);
    let raw = fs_err::read_to_string(path.as_std_path())
        .map_err(|e| Error::Io { path: path.clone(), source: e })?;
    let doc = skillctl_skill::parse_str(&raw)?;
    let summary = skillctl_skill::derive_summary(&doc.frontmatter);

    // 资源枚举：扫描 skill 目录，排除 SKILL.md 与 meta.json
    let resources = list_resources(&global.skill_dir(&id, &version));

    let resp = DescribeResponse {
        protocol: PROTOCOL_VERSION,
        id: id_str.clone(),
        name: id.name.as_str().to_owned(),
        namespace: id.namespace.as_str().to_owned(),
        version: version.clone(),
        tier,
        summary,
        description: doc.frontmatter.description.clone(),
        commands: Vec::new(),
        tags: doc.frontmatter.metadata.tags.clone(),
        languages: doc.frontmatter.metadata.languages.clone(),
        triggers: doc.frontmatter.metadata.triggers.clone(),
        requires: doc.frontmatter.metadata.requires.clone(),
        resources,
    };

    if args.brief {
        let brief = serde_json::json!({
            "protocol": PROTOCOL_VERSION,
            "id": resp.id,
            "name": resp.name,
            "namespace": resp.namespace,
            "version": resp.version,
            "tier": resp.tier,
            "summary": resp.summary,
        });
        match fmt {
            super::OutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(&brief)
                    .map_err(|e| Error::other(format!("json: {e}")))?
            ),
            super::OutputFormat::Human | super::OutputFormat::Tsv => println!(
                "{} v{} [{}]: {}",
                resp.id,
                resp.version,
                tier_label(resp.tier),
                resp.summary
            ),
        }
        return Ok(());
    }

    util::emit(fmt, &resp, |r| {
        println!("{} v{} [{}]", r.id, r.version, tier_label(r.tier));
        println!("  {}", r.summary);
        if !r.tags.is_empty() {
            println!("  tags: {}", r.tags.join(", "));
        }
        if !r.languages.is_empty() {
            println!("  languages: {}", r.languages.join(", "));
        }
        Ok(())
    })
}

fn list_resources(dir: &camino::Utf8Path) -> Vec<String> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    for entry in walkdir::WalkDir::new(dir.as_std_path()).min_depth(1).max_depth(4) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(dir.as_std_path()) else { continue };
        let s = rel.to_string_lossy();
        if s == "SKILL.md" || s == "meta.json" {
            continue;
        }
        out.push(s.into_owned());
    }
    out.sort();
    out
}

fn tier_label(t: skillctl_core::Tier) -> &'static str {
    match t {
        skillctl_core::Tier::Core => "core",
        skillctl_core::Tier::Extra => "extra",
    }
}
