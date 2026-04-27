//! `skillctl add` — 添加技能到全局仓库（local / github）。
//!
//! 三种模式：
//! - **single**：source 直接指向 SKILL.md（含 fallback 到 `skills/<sub>/`）→ 安装一个
//! - **discover**：source 是 multi-skill 仓库 + 无 sub-path → 列候选（TUI / TSV / JSON）
//! - **all**：`--all` 标志 + 多 skill 仓库 → 批量安装

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;
use skillctl_core::{Error, Result};
use skillctl_id::SkillId;
use skillctl_skill::SkillDoc;
use skillctl_source::{discover_skills, fetch, parse_source, DiscoveredSkill, Fetched, Source};
use skillctl_store::{sha256_hex, GlobalEntry, GlobalStore};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// 来源：本地路径、`github:user/repo[/path][@ref]`、URL。
    pub source: String,
    /// 安装为该 ID（覆盖元数据推断）；仅单 skill 模式生效。
    #[arg(long)]
    pub r#as: Option<String>,
    /// 多 skill 仓库时批量安装所有发现的 skill。
    #[arg(long)]
    pub all: bool,
}

#[derive(Serialize)]
struct AddOutput {
    success: bool,
    action: &'static str,
    skill: SkillSummary,
    updated: Vec<Utf8PathBuf>,
}

#[derive(Serialize)]
struct SkillSummary {
    id: String,
    namespace: String,
    name: String,
    version: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
    checksum: String,
}

#[derive(Serialize)]
struct BulkOutput {
    success: bool,
    action: &'static str,
    installed: Vec<SkillSummary>,
}

#[derive(Serialize)]
struct DiscoverOutput {
    protocol: u32,
    action: &'static str,
    source: String,
    count: usize,
    candidates: Vec<DiscoveredView>,
}

#[derive(Serialize, Clone)]
struct DiscoveredView {
    name: String,
    namespace: Option<String>,
    path: String,
    version: String,
    description: String,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    store.ensure_dirs()?;

    let source = parse_source(&args.source)?;
    let fetched = fetch(&source)?;

    // 单 skill 命中：root 直接含 SKILL.md
    if locate_skill_md(&fetched.root).is_ok() {
        return install_one(&store, &source, &fetched, args.r#as.as_deref(), fmt);
    }

    // 多 skill 仓库：扫描候选
    let candidates = discover_skills(&fetched.root)?;
    if candidates.is_empty() {
        return Err(Error::other(format!("no SKILL.md found at {} or in skills/*/", fetched.root)));
    }

    // --all：批量安装
    if args.all {
        return install_bulk(&store, &source, &fetched, &candidates, fmt);
    }

    // 否则进入 discovery 模式
    discover_mode(&store, &source, &fetched, candidates, &args.source, fmt)
}

// ─────────── single skill install ───────────

fn install_one(
    store: &GlobalStore,
    source: &Source,
    fetched: &Fetched,
    forced_id: Option<&str>,
    fmt: super::OutputFormat,
) -> Result<()> {
    let summary = install_at(store, source, fetched, &fetched.root, forced_id)?;
    let target_md = store.skill_md(&SkillId::parse(&summary.id)?, &summary.version);
    let out = AddOutput {
        success: true,
        action: "add",
        skill: summary,
        updated: vec![store.registry_lock_path(), target_md],
    };
    util::emit(fmt, &out, |o| {
        println!(
            "added {} v{} ({}) [{}]",
            o.skill.id,
            o.skill.version,
            o.skill.source,
            o.skill.commit.as_deref().unwrap_or("-")
        );
        Ok(())
    })
}

// ─────────── bulk install ───────────

fn install_bulk(
    store: &GlobalStore,
    source: &Source,
    fetched: &Fetched,
    candidates: &[DiscoveredSkill],
    fmt: super::OutputFormat,
) -> Result<()> {
    let mut installed = Vec::with_capacity(candidates.len());
    for c in candidates {
        let skill_root = c
            .skill_md
            .parent()
            .ok_or_else(|| Error::other(format!("invalid skill_md: {}", c.skill_md)))?;
        match install_at(store, source, fetched, skill_root, None) {
            Ok(s) => installed.push(s),
            Err(e) => eprintln!("skip {}: {e}", c.rel_path),
        }
    }
    let out = BulkOutput { success: true, action: "add.bulk", installed };
    util::emit(fmt, &out, |o| {
        for s in &o.installed {
            println!("added {} v{}", s.id, s.version);
        }
        println!("({} installed)", o.installed.len());
        Ok(())
    })
}

// ─────────── discovery ───────────

fn discover_mode(
    store: &GlobalStore,
    source: &Source,
    fetched: &Fetched,
    candidates: Vec<DiscoveredSkill>,
    source_str: &str,
    fmt: super::OutputFormat,
) -> Result<()> {
    // 解析每个候选的 frontmatter，构造 view
    let views: Vec<DiscoveredView> = candidates
        .iter()
        .filter_map(|c| {
            let raw = fs_err::read_to_string(c.skill_md.as_std_path()).ok()?;
            let doc = skillctl_skill::parse_str(&raw).ok()?;
            let summary = skillctl_skill::derive_summary(&doc.frontmatter);
            Some(DiscoveredView {
                name: doc.frontmatter.name,
                namespace: doc.frontmatter.metadata.namespace.map(|n| n.as_str().to_owned()),
                path: c.rel_path.to_string(),
                version: doc.frontmatter.version,
                description: summary,
            })
        })
        .collect();

    use std::io::IsTerminal;
    let interactive = std::io::stdin().is_terminal();

    // TTY 模式：进入 TUI 多选 + 安装
    #[cfg(feature = "tui")]
    if interactive && matches!(fmt, super::OutputFormat::Human) {
        return tui_select_and_install(store, source, fetched, &candidates, &views);
    }

    // 非 TTY：emit catalog
    if matches!(fmt, super::OutputFormat::Tsv)
        || (matches!(fmt, super::OutputFormat::Human) && !interactive)
    {
        emit_catalog_tsv(&views);
        return Ok(());
    }
    // JSON
    let out = DiscoverOutput {
        protocol: 1,
        action: "discover",
        source: source_str.to_owned(),
        count: views.len(),
        candidates: views,
    };
    util::emit(fmt, &out, |o| {
        for c in &o.candidates {
            let ns = c.namespace.as_deref().unwrap_or("(none)");
            println!("{ns:<12} {:<20} {} — {}", c.name, c.path, c.description);
        }
        println!("\n({} candidates) re-run with a sub-path or --all", o.count);
        Ok(())
    })
}

fn emit_catalog_tsv(views: &[DiscoveredView]) {
    println!("NAME\tNAMESPACE\tPATH\tDESCRIPTION");
    for v in views {
        let ns = v.namespace.as_deref().unwrap_or("");
        let desc = util::tsv_clean(&v.description);
        let desc = if desc.chars().count() > 80 {
            let truncated: String = desc.chars().take(79).collect();
            format!("{truncated}…")
        } else {
            desc
        };
        println!("{}\t{}\t{}\t{}", util::tsv_clean(&v.name), ns, util::tsv_clean(&v.path), desc);
    }
}

#[cfg(feature = "tui")]
fn tui_select_and_install(
    store: &GlobalStore,
    source: &Source,
    fetched: &Fetched,
    candidates: &[DiscoveredSkill],
    views: &[DiscoveredView],
) -> Result<()> {
    let tui_items: Vec<skillctl_tui::DiscoverItem> = views
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let proposed_id = match (&v.namespace, v.name.as_str()) {
                (Some(ns), n) => format!("{ns}/{n}"),
                (None, n) => n.to_owned(),
            };
            skillctl_tui::DiscoverItem {
                index: i,
                proposed_id,
                description: v.description.clone(),
                path: v.path.clone(),
            }
        })
        .collect();

    let selected = skillctl_tui::select_for_install(tui_items)?;
    if selected.is_empty() {
        eprintln!("nothing selected");
        return Ok(());
    }

    for idx in selected {
        let c = &candidates[idx];
        let v = &views[idx];
        let proposed_id = match (&v.namespace, v.name.as_str()) {
            (Some(ns), n) => Some(format!("{ns}/{n}")),
            _ => None,
        };
        let skill_root = c
            .skill_md
            .parent()
            .ok_or_else(|| Error::other(format!("invalid skill_md: {}", c.skill_md)))?;
        match install_at(store, source, fetched, skill_root, proposed_id.as_deref()) {
            Ok(s) => println!("added {} v{}", s.id, s.version),
            Err(e) => eprintln!("skip {}: {e}", v.path),
        }
    }
    Ok(())
}

#[cfg(not(feature = "tui"))]
fn tui_select_and_install(
    _store: &GlobalStore,
    _source: &Source,
    _fetched: &Fetched,
    _candidates: &[DiscoveredSkill],
    _views: &[DiscoveredView],
) -> Result<()> {
    Err(Error::other("TUI not built; pass `--all` or a sub-path"))
}

// ─────────── shared install routine ───────────

fn install_at(
    store: &GlobalStore,
    source: &Source,
    fetched: &Fetched,
    skill_root: &Utf8Path,
    forced_id: Option<&str>,
) -> Result<SkillSummary> {
    let skill_md = locate_skill_md(skill_root)?;
    let raw = fs_err::read_to_string(skill_md.as_std_path())
        .map_err(|e| Error::Io { path: skill_md.clone(), source: e })?;
    let doc: SkillDoc = skillctl_skill::parse_str(&raw)?;

    let id = if let Some(forced) = forced_id {
        SkillId::parse(forced)?
    } else {
        let ns = doc.frontmatter.metadata.namespace.clone().ok_or_else(|| {
            Error::other(format!(
                "metadata.namespace missing for `{}`; pass --as <namespace/name>",
                doc.frontmatter.name
            ))
        })?;
        SkillId { namespace: ns, name: skillctl_id::SkillName::parse(&doc.frontmatter.name)? }
    };

    let version = doc.frontmatter.version.clone();
    let checksum = sha256_hex(raw.as_bytes());
    let summary_text = skillctl_skill::derive_summary(&doc.frontmatter);

    let target_dir = store.skill_dir(&id, &version);
    fs_err::create_dir_all(target_dir.as_std_path())
        .map_err(|e| Error::Io { path: target_dir.clone(), source: e })?;
    let target_md = store.skill_md(&id, &version);

    let src_dir = if skill_md.parent() == Some(skill_root) {
        skill_root.to_owned()
    } else {
        skill_md.parent().map(Utf8Path::to_owned).unwrap_or_else(|| skill_root.to_owned())
    };
    copy_tree(&src_dir, &target_dir)?;
    fs_err::write(target_md.as_std_path(), &raw)
        .map_err(|e| Error::Io { path: target_md.clone(), source: e })?;

    let mut reg = store.read_registry()?;
    let local_path = match source {
        Source::Local { path } => Some(path.clone()),
        _ => None,
    };
    reg.skills.insert(
        id.to_string(),
        GlobalEntry {
            id: id.to_string(),
            namespace: id.namespace.as_str().to_owned(),
            name: id.name.as_str().to_owned(),
            version: version.clone(),
            source: fetched.source_kind.to_owned(),
            local_path,
            repo: fetched.repo.clone(),
            commit: fetched.commit.clone(),
            checksum: checksum.clone(),
            summary: summary_text.clone(),
            triggers: doc.frontmatter.metadata.triggers.clone(),
            languages: doc.frontmatter.metadata.languages.clone(),
            tags: doc.frontmatter.metadata.tags.clone(),
        },
    );
    store.write_registry(&reg)?;

    Ok(SkillSummary {
        id: id.to_string(),
        namespace: id.namespace.as_str().to_owned(),
        name: id.name.as_str().to_owned(),
        version,
        source: fetched.source_kind.to_owned(),
        repo: fetched.repo.clone(),
        commit: fetched.commit.clone(),
        checksum,
    })
}

fn locate_skill_md(root: &Utf8Path) -> Result<Utf8PathBuf> {
    if root.is_file() && root.file_name() == Some("SKILL.md") {
        return Ok(root.to_owned());
    }
    let direct = root.join("SKILL.md");
    if direct.exists() {
        return Ok(direct);
    }
    Err(Error::other(format!("SKILL.md not found at {root}")))
}

fn copy_tree(src: &Utf8Path, dst: &Utf8Path) -> Result<()> {
    if src.is_file() {
        return Ok(());
    }
    for entry in walkdir::WalkDir::new(src.as_std_path()).min_depth(1) {
        let entry = entry.map_err(|e| Error::other(format!("walk: {e}")))?;
        let rel = entry
            .path()
            .strip_prefix(src.as_std_path())
            .map_err(|e| Error::other(format!("strip: {e}")))?;
        let rel_str = rel.to_string_lossy();
        if rel_str.starts_with(".git/") || rel_str == ".git" || rel_str == "SKILL.md" {
            continue;
        }
        let dst_p = dst.as_std_path().join(rel);
        if entry.file_type().is_dir() {
            fs_err::create_dir_all(&dst_p)
                .map_err(|e| Error::other(format!("mkdir {dst_p:?}: {e}")))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = dst_p.parent() {
                fs_err::create_dir_all(parent)
                    .map_err(|e| Error::other(format!("mkdir {parent:?}: {e}")))?;
            }
            fs_err::copy(entry.path(), &dst_p)
                .map_err(|e| Error::other(format!("copy {dst_p:?}: {e}")))?;
        }
    }
    Ok(())
}
