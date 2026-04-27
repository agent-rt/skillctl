//! `skillctl trust` — 项目信任清单管理。

use camino::Utf8PathBuf;
use serde::Serialize;
use skillctl_core::{Error, Result};
use skillctl_store::{GlobalStore, ProjectStore};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// 项目根（缺省时用 cwd 起向上查找）。
    pub path: Option<String>,
    /// 列出当前信任清单。
    #[arg(long, conflicts_with_all = ["remove"])]
    pub list: bool,
    /// 从信任清单移除而非添加。
    #[arg(long, conflicts_with = "list")]
    pub remove: bool,
}

#[derive(Serialize)]
struct TrustOut {
    success: bool,
    action: &'static str,
    path: Utf8PathBuf,
}

#[derive(Serialize)]
struct ListOut {
    success: bool,
    count: usize,
    projects: Vec<Utf8PathBuf>,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let global = GlobalStore::default_open()?;

    if args.list {
        let list = global.read_trusted()?;
        let out = ListOut { success: true, count: list.projects.len(), projects: list.projects };
        return util::emit(fmt, &out, |o| {
            for p in &o.projects {
                println!("{p}");
            }
            Ok(())
        });
    }

    let project_root = resolve_target(args.path.as_deref())?;
    if args.remove {
        global.untrust(&project_root)?;
        let out = TrustOut { success: true, action: "trust.remove", path: project_root };
        return util::emit(fmt, &out, |o| {
            println!("untrusted {}", o.path);
            Ok(())
        });
    }

    global.trust(&project_root)?;
    let out = TrustOut { success: true, action: "trust.add", path: project_root };
    util::emit(fmt, &out, |o| {
        println!("trusted {}", o.path);
        Ok(())
    })
}

fn resolve_target(path: Option<&str>) -> Result<Utf8PathBuf> {
    let cwd = util::cwd()?;
    let candidate = match path {
        Some(p) => {
            let pp = Utf8PathBuf::from(p);
            if pp.is_absolute() {
                pp
            } else {
                cwd.join(pp)
            }
        }
        None => cwd.clone(),
    };
    // 规范化路径（消除 . 与 ..）
    let canonical = std::fs::canonicalize(candidate.as_std_path())
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .unwrap_or(candidate);
    if let Some(store) = ProjectStore::discover(&canonical)? {
        Ok(store.root)
    } else if canonical.exists() {
        Ok(canonical)
    } else {
        Err(Error::other(format!("path does not exist: {canonical}")))
    }
}
