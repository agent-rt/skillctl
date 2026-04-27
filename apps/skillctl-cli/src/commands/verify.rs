//! `skillctl verify` — 校验已安装技能的 checksum。

use serde::Serialize;
use skillctl_core::{Error, Result};
use skillctl_id::SkillId;
use skillctl_store::{sha256_hex, GlobalStore};

use super::util;

#[derive(Debug, clap::Args)]
pub struct Args {
    pub id: Option<String>,
}

#[derive(Serialize)]
struct VerifyOut {
    success: bool,
    items: Vec<VerifyItem>,
}

#[derive(Serialize)]
struct VerifyItem {
    id: String,
    expected: String,
    actual: String,
    ok: bool,
}

pub fn run(args: Args, fmt: super::OutputFormat) -> Result<()> {
    let store = GlobalStore::default_open()?;
    let reg = store.read_registry()?;

    let targets: Vec<String> = match args.id.as_deref() {
        Some(q) => vec![store.resolve_short(q)?.to_string()],
        None => reg.skills.keys().cloned().collect(),
    };

    let mut items = Vec::new();
    let mut all_ok = true;
    for id_str in targets {
        let entry = match reg.skills.get(&id_str) {
            Some(e) => e,
            None => continue,
        };
        let id = SkillId::parse(&id_str)?;
        let md = store.skill_md(&id, &entry.version);
        let raw = fs_err::read_to_string(md.as_std_path())
            .map_err(|e| Error::Io { path: md.clone(), source: e })?;
        let actual = sha256_hex(raw.as_bytes());
        let ok = actual == entry.checksum;
        if !ok {
            all_ok = false;
        }
        items.push(VerifyItem { id: id_str, expected: entry.checksum.clone(), actual, ok });
    }

    let out = VerifyOut { success: all_ok, items };
    util::emit(fmt, &out, |o| {
        for i in &o.items {
            let mark = if i.ok { "✓" } else { "✗ MISMATCH" };
            println!("{mark}  {}  {}", i.id, i.expected);
        }
        Ok(())
    })
}
