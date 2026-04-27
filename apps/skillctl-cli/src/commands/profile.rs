//! `skillctl profile *` — profile 全局管理。

use camino::Utf8PathBuf;
use clap::Subcommand;
use serde::Serialize;
use skillctl_core::{Error, Result};
use skillctl_store::GlobalStore;

use super::util;

#[derive(Debug, Subcommand)]
pub enum ProfileCmd {
    /// 添加 profile 到全局。
    Add(AddArgs),
    /// 列出全局已安装 profile。
    List,
    /// 展示 profile 内容。
    Show(ShowArgs),
    /// 移除 profile。
    Remove(RemoveArgs),
    /// 更新 profile（重新拉取来源）。
    Update(UpdateArgs),
}

#[derive(Debug, clap::Args)]
pub struct AddArgs {
    /// 来源：本地 .toml 文件路径。
    pub source: String,
    /// 安装为该 profile 名（覆盖文件中 profile.name）。
    #[arg(long)]
    pub r#as: Option<String>,
}

fn sidecar_path(store: &GlobalStore, name: &str) -> Utf8PathBuf {
    store.profiles_dir().join(format!("{name}.source.json"))
}

fn save_sidecar(
    store: &GlobalStore,
    name: &str,
    original: &str,
    fetched: &skillctl_source::Fetched,
) -> Result<()> {
    let p = sidecar_path(store, name);
    let v = serde_json::json!({
        "original": original,
        "source_kind": fetched.source_kind,
        "repo": fetched.repo,
        "commit": fetched.commit,
    });
    let s = serde_json::to_string_pretty(&v)
        .map_err(|e| Error::other(format!("sidecar serialize: {e}")))?;
    fs_err::write(p.as_std_path(), s).map_err(|e| Error::Io { path: p, source: e })
}

fn read_sidecar(store: &GlobalStore, name: &str) -> Result<Option<String>> {
    let p = sidecar_path(store, name);
    match fs_err::read_to_string(p.as_std_path()) {
        Ok(s) => {
            let v: serde_json::Value = serde_json::from_str(&s)
                .map_err(|e| Error::other(format!("sidecar parse: {e}")))?;
            Ok(v["original"].as_str().map(str::to_owned))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io { path: p, source: e }),
    }
}

#[derive(Debug, clap::Args)]
pub struct ShowArgs {
    pub name: String,
}

#[derive(Debug, clap::Args)]
pub struct RemoveArgs {
    pub name: String,
}

#[derive(Debug, clap::Args)]
pub struct UpdateArgs {
    pub name: Option<String>,
}

pub fn run(cmd: ProfileCmd, fmt: super::OutputFormat) -> Result<()> {
    match cmd {
        ProfileCmd::Add(a) => add(a, fmt),
        ProfileCmd::List => list(fmt),
        ProfileCmd::Show(a) => show(a, fmt),
        ProfileCmd::Remove(a) => remove(a, fmt),
        ProfileCmd::Update(a) => update(a, fmt),
    }
}

#[derive(Serialize)]
struct UpdateOut {
    success: bool,
    action: &'static str,
    updated: Vec<String>,
    skipped: Vec<String>,
}

fn update(args: UpdateArgs, fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    let dir = store.profiles_dir();
    let mut targets: Vec<String> = Vec::new();
    if let Some(name) = args.name {
        targets.push(name);
    } else if dir.exists() {
        for entry in fs_err::read_dir(dir.as_std_path())
            .map_err(|e| Error::Io { path: dir.clone(), source: e })?
        {
            let Ok(entry) = entry else { continue };
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                targets.push(stem.to_owned());
            }
        }
    }

    let mut updated = Vec::new();
    let mut skipped = Vec::new();
    for name in targets {
        let original = match read_sidecar(&store, &name)? {
            Some(s) => s,
            None => {
                skipped.push(name);
                continue;
            }
        };
        let source = skillctl_source::parse_source(&original)?;
        let fetched = skillctl_source::fetch(&source)?;
        let toml_path = if fetched.root.is_file() {
            fetched.root.clone()
        } else {
            return Err(Error::other(format!(
                "profile source must be a .toml file: {}",
                fetched.root
            )));
        };
        let raw = fs_err::read_to_string(toml_path.as_std_path())
            .map_err(|e| Error::Io { path: toml_path.clone(), source: e })?;
        let mut prof = skillctl_profile::parse_str(&raw)?;
        prof.profile.name = name.clone();
        let target = store.profile_path(&name);
        let s = skillctl_profile::to_string(&prof)?;
        fs_err::write(target.as_std_path(), s)
            .map_err(|e| Error::Io { path: target, source: e })?;
        save_sidecar(&store, &name, &original, &fetched)?;
        updated.push(name);
    }

    let out = UpdateOut { success: true, action: "profile.update", updated, skipped };
    util::emit(fmt, &out, |o| {
        for u in &o.updated {
            println!("updated {u}");
        }
        for s in &o.skipped {
            println!("skipped {s} (no source recorded)");
        }
        Ok(())
    })
}

#[derive(Serialize)]
struct AddOut<'a> {
    success: bool,
    action: &'static str,
    name: &'a str,
    path: Utf8PathBuf,
}

fn add(args: AddArgs, fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    store.ensure_dirs()?;

    // 通过 source 抽象解析（local / github）
    let source = skillctl_source::parse_source(&args.source)?;
    let fetched = skillctl_source::fetch(&source)?;

    // profile 是单个 .toml 文件
    let toml_path = if fetched.root.is_file() {
        fetched.root.clone()
    } else {
        return Err(Error::other(format!(
            "profile source must be a .toml file path: {}",
            fetched.root
        )));
    };

    let raw = fs_err::read_to_string(toml_path.as_std_path())
        .map_err(|e| Error::Io { path: toml_path.clone(), source: e })?;
    let mut prof = skillctl_profile::parse_str(&raw)?;
    if let Some(forced) = args.r#as.as_deref() {
        prof.profile.name = forced.to_owned();
    }
    let name = prof.profile.name.clone();
    if name.trim().is_empty() {
        return Err(Error::other("profile name is empty; use --as <name>"));
    }
    let target = store.profile_path(&name);
    let s = skillctl_profile::to_string(&prof)?;
    fs_err::write(target.as_std_path(), s)
        .map_err(|e| Error::Io { path: target.clone(), source: e })?;
    save_sidecar(&store, &name, &args.source, &fetched)?;
    let out = AddOut { success: true, action: "profile.add", name: &name, path: target };
    util::emit(fmt, &out, |o| {
        println!("added profile {} → {}", o.name, o.path);
        Ok(())
    })
}

#[derive(Serialize)]
struct ListOut {
    success: bool,
    count: usize,
    profiles: Vec<ProfileSummary>,
}

#[derive(Serialize)]
struct ProfileSummary {
    name: String,
    description: Option<String>,
    core_count: usize,
    extra_count: usize,
}

fn list(fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    let dir = store.profiles_dir();
    let mut profiles = Vec::new();
    if dir.exists() {
        for entry in fs_err::read_dir(dir.as_std_path())
            .map_err(|e| Error::Io { path: dir.clone(), source: e })?
        {
            let entry = entry.map_err(|e| Error::other(format!("readdir: {e}")))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            let raw = fs_err::read_to_string(&path)
                .map_err(|e| Error::other(format!("read {path:?}: {e}")))?;
            if let Ok(p) = skillctl_profile::parse_str(&raw) {
                profiles.push(ProfileSummary {
                    name: p.profile.name,
                    description: p.profile.description,
                    core_count: p.skills.core.len(),
                    extra_count: p.skills.extra.len(),
                });
            }
        }
    }
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    let out = ListOut { success: true, count: profiles.len(), profiles };
    util::emit(fmt, &out, |o| {
        for p in &o.profiles {
            println!(
                "{:<24} core={:<3} extra={:<3} {}",
                p.name,
                p.core_count,
                p.extra_count,
                p.description.as_deref().unwrap_or("")
            );
        }
        Ok(())
    })
}

fn show(args: ShowArgs, fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    let path = store.profile_path(&args.name);
    if !path.exists() {
        return Err(Error::ProfileNotFound(args.name.clone()));
    }
    let raw = fs_err::read_to_string(path.as_std_path())
        .map_err(|e| Error::Io { path: path.clone(), source: e })?;
    match fmt {
        super::OutputFormat::Json => {
            let prof = skillctl_profile::parse_str(&raw)?;
            let s = serde_json::to_string_pretty(&prof)
                .map_err(|e| Error::other(format!("json: {e}")))?;
            println!("{s}");
        }
        super::OutputFormat::Human | super::OutputFormat::Tsv => print!("{raw}"),
    }
    Ok(())
}

#[derive(Serialize)]
struct RemoveOut<'a> {
    success: bool,
    action: &'static str,
    name: &'a str,
}

fn remove(args: RemoveArgs, fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    let path = store.profile_path(&args.name);
    if !path.exists() {
        return Err(Error::ProfileNotFound(args.name.clone()));
    }
    fs_err::remove_file(path.as_std_path()).map_err(|e| Error::Io { path, source: e })?;
    // 移除 sidecar 如果存在
    let sc = sidecar_path(&store, &args.name);
    if sc.exists() {
        let _ = fs_err::remove_file(sc.as_std_path());
    }
    let out = RemoveOut { success: true, action: "profile.remove", name: &args.name };
    util::emit(fmt, &out, |o| {
        println!("removed profile {}", o.name);
        Ok(())
    })
}
