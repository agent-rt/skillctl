//! `skillctl inspect` — 审计技能元数据/脚本/依赖/风险。

use serde::Serialize;
use skillctl_core::{Error, Result};
use skillctl_id::SkillId;
use skillctl_store::GlobalStore;

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    pub id: String,
}

#[derive(Serialize)]
struct InspectOut {
    success: bool,
    id: String,
    version: String,
    risk_class: &'static str,
    findings: Vec<Finding>,
    resources: Vec<String>,
}

#[derive(Serialize)]
struct Finding {
    severity: &'static str,
    kind: &'static str,
    detail: String,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    let resolved = store.resolve_short(&args.id)?;
    let reg = store.read_registry()?;
    let entry = reg
        .skills
        .get(&resolved.to_string())
        .ok_or_else(|| Error::SkillNotFound(args.id.clone()))?;

    let id = SkillId::parse(&entry.id)?;
    let md = store.skill_md(&id, &entry.version);
    let raw = fs_err::read_to_string(md.as_std_path())
        .map_err(|e| Error::Io { path: md.clone(), source: e })?;
    let doc = skillctl_skill::parse_str(&raw)?;

    let mut findings = Vec::new();
    let dir = store.skill_dir(&id, &entry.version);

    // 资源枚举 + 风险扫描
    let mut resources = Vec::new();
    let mut has_scripts = false;
    let mut has_binary_dep = false;
    let mut has_env = false;
    if dir.exists() {
        for w in walkdir::WalkDir::new(dir.as_std_path()).min_depth(1) {
            let Ok(w) = w else { continue };
            if !w.file_type().is_file() {
                continue;
            }
            let Ok(rel) = w.path().strip_prefix(dir.as_std_path()) else { continue };
            let s = rel.to_string_lossy().into_owned();
            if s == "SKILL.md" || s == "meta.json" {
                continue;
            }
            // shell / python / js 脚本视为脚本
            let lower = s.to_ascii_lowercase();
            if lower.ends_with(".sh")
                || lower.ends_with(".py")
                || lower.ends_with(".js")
                || lower.ends_with(".ts")
            {
                has_scripts = true;
            }
            resources.push(s);
        }
    }
    resources.sort();

    if !doc.frontmatter.metadata.requires.binaries.is_empty() {
        has_binary_dep = true;
        for b in &doc.frontmatter.metadata.requires.binaries {
            findings.push(Finding {
                severity: "info",
                kind: "binary_dep",
                detail: format!(
                    "{}{}",
                    b.name,
                    b.version.as_deref().map(|v| format!(" {v}")).unwrap_or_default()
                ),
            });
        }
    }
    if !doc.frontmatter.metadata.requires.env.is_empty() {
        has_env = true;
        for e in &doc.frontmatter.metadata.requires.env {
            findings.push(Finding {
                severity: "warn",
                kind: "env_dep",
                detail: format!("requires env var: {e}"),
            });
        }
    }
    if has_scripts {
        findings.push(Finding {
            severity: "warn",
            kind: "scripts_present",
            detail: "skill bundle includes executable scripts; review before invoking".into(),
        });
    }

    let risk_class = match (has_scripts, has_env, has_binary_dep) {
        (true, _, _) => "script",
        (_, true, _) => "secret-dependent",
        (_, _, true) => "binary-dependent",
        _ => "instruction-only",
    };

    let out = InspectOut {
        success: true,
        id: entry.id.clone(),
        version: entry.version.clone(),
        risk_class,
        findings,
        resources,
    };
    util::emit(fmt, &out, |o| {
        println!("{} v{}  risk={}", o.id, o.version, o.risk_class);
        for f in &o.findings {
            println!("  [{}] {}: {}", f.severity, f.kind, f.detail);
        }
        if !o.resources.is_empty() {
            println!("  resources:");
            for r in &o.resources {
                println!("    {r}");
            }
        }
        Ok(())
    })
}
