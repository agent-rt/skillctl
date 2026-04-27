//! `skillctl add` — 添加技能到全局仓库（local / github）。

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;
use skillctl_core::{Error, Result};
use skillctl_id::SkillId;
use skillctl_skill::SkillDoc;
use skillctl_source::{fetch, parse_source, Source};
use skillctl_store::{sha256_hex, GlobalEntry, GlobalStore};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// 来源：本地路径、`github:user/repo[/path][@ref]`、URL。
    pub source: String,
    /// 安装为该 ID（覆盖元数据推断）。
    #[arg(long)]
    pub r#as: Option<String>,
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

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    store.ensure_dirs()?;

    let source = parse_source(&args.source)?;
    let fetched = fetch(&source)?;

    let skill_md = locate_skill_md(&fetched.root)?;
    let raw = fs_err::read_to_string(skill_md.as_std_path())
        .map_err(|e| Error::Io { path: skill_md.clone(), source: e })?;
    let doc: SkillDoc = skillctl_skill::parse_str(&raw)?;

    let id = if let Some(forced) = args.r#as.as_deref() {
        SkillId::parse(forced)?
    } else {
        let ns = doc.frontmatter.metadata.namespace.clone().ok_or_else(|| {
            Error::other("metadata.namespace missing; pass --as <namespace/name>")
        })?;
        SkillId { namespace: ns, name: skillctl_id::SkillName::parse(&doc.frontmatter.name)? }
    };

    let version = doc.frontmatter.version.clone();
    let checksum = sha256_hex(raw.as_bytes());
    let summary = skillctl_skill::derive_summary(&doc.frontmatter);

    // 写入全局仓库
    let target_dir = store.skill_dir(&id, &version);
    fs_err::create_dir_all(target_dir.as_std_path())
        .map_err(|e| Error::Io { path: target_dir.clone(), source: e })?;
    let target_md = store.skill_md(&id, &version);

    // 复制 SKILL.md 与所有同目录 / 子目录资源
    let src_dir = if skill_md.parent() == Some(&fetched.root) {
        fetched.root.clone()
    } else {
        skill_md.parent().map(Utf8Path::to_owned).unwrap_or_else(|| fetched.root.clone())
    };
    copy_tree(&src_dir, &target_dir)?;
    fs_err::write(target_md.as_std_path(), &raw)
        .map_err(|e| Error::Io { path: target_md.clone(), source: e })?;

    // 更新 registry
    let mut reg = store.read_registry()?;
    let local_path = match &source {
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
            summary: summary.clone(),
            triggers: doc.frontmatter.metadata.triggers.clone(),
            languages: doc.frontmatter.metadata.languages.clone(),
            tags: doc.frontmatter.metadata.tags.clone(),
        },
    );
    store.write_registry(&reg)?;

    let out = AddOutput {
        success: true,
        action: "add",
        skill: SkillSummary {
            id: id.to_string(),
            namespace: id.namespace.as_str().to_owned(),
            name: id.name.as_str().to_owned(),
            version,
            source: fetched.source_kind.to_owned(),
            repo: fetched.repo.clone(),
            commit: fetched.commit.clone(),
            checksum,
        },
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
        return Ok(()); // 单文件来源由调用方写入
    }
    for entry in walkdir::WalkDir::new(src.as_std_path()).min_depth(1) {
        let entry = entry.map_err(|e| Error::other(format!("walk: {e}")))?;
        let rel = entry
            .path()
            .strip_prefix(src.as_std_path())
            .map_err(|e| Error::other(format!("strip: {e}")))?;
        let rel_str = rel.to_string_lossy();
        // 跳过 .git/ 与 SKILL.md 自身
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
