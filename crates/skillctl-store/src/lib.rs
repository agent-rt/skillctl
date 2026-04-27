//! 存储层：全局技能仓库 + 项目 manifest/lock/index。
//!
//! 见 REQ.md §6、§9。

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use skillctl_core::{Error, Result, Tier};
use skillctl_id::SkillId;
use skillctl_profile::SkillSpec;

// ───────────────────────── 项目 manifest ─────────────────────────

/// 项目 manifest（`skillctl.toml`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub project: ProjectMeta,
    #[serde(default)]
    pub skills: ManifestSkills,
    #[serde(default)]
    pub agent: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub started_from: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linked_to: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManifestSkills {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub core: BTreeMap<String, SkillSpec>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, SkillSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub enabled: bool,
    pub target: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self { enabled: false, target: "AGENTS.md".into() }
    }
}

impl Manifest {
    /// 返回 (id, spec, tier) 的扁平视图。core 优先。
    pub fn iter_skills(&self) -> impl Iterator<Item = (&String, &SkillSpec, Tier)> {
        self.skills
            .core
            .iter()
            .map(|(k, v)| (k, v, Tier::Core))
            .chain(self.skills.extra.iter().map(|(k, v)| (k, v, Tier::Extra)))
    }
}

// ───────────────────────── 项目 lock ─────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Lock {
    #[serde(default)]
    pub skills: BTreeMap<String, LockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub id: String,
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub tier: Tier,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_ref: Option<String>,
    pub checksum: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

// ───────────────────────── 全局 registry.lock ─────────────────────────

/// 全局已安装技能注册表（`~/.skillctl/registry.lock`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryLock {
    #[serde(default)]
    pub skills: BTreeMap<String, GlobalEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalEntry {
    pub id: String,
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<Utf8PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub checksum: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

// ───────────────────────── 全局仓库 ─────────────────────────

/// 全局仓库根目录（默认 `~/.skillctl/`）。
#[derive(Debug, Clone)]
pub struct GlobalStore {
    pub root: Utf8PathBuf,
}

impl GlobalStore {
    /// 返回默认全局仓库路径（`$HOME/.skillctl`）。
    pub fn default_root() -> Result<Utf8PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| Error::other("cannot resolve home directory"))?;
        Utf8PathBuf::from_path_buf(home.join(".skillctl"))
            .map_err(|p| Error::other(format!("non-utf8 home: {p:?}")))
    }

    /// 用默认根构造。
    pub fn default_open() -> Result<Self> {
        Ok(Self { root: Self::default_root()? })
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for sub in ["skills", "profiles"] {
            let p = self.root.join(sub);
            fs_err::create_dir_all(p.as_std_path())
                .map_err(|e| Error::Io { path: p.clone(), source: e })?;
        }
        Ok(())
    }

    #[must_use]
    pub fn skills_dir(&self) -> Utf8PathBuf {
        self.root.join("skills")
    }
    #[must_use]
    pub fn profiles_dir(&self) -> Utf8PathBuf {
        self.root.join("profiles")
    }
    #[must_use]
    pub fn registry_lock_path(&self) -> Utf8PathBuf {
        self.root.join("registry.lock")
    }

    #[must_use]
    pub fn skill_dir(&self, id: &SkillId, version: &str) -> Utf8PathBuf {
        self.root.join("skills").join(id.namespace.as_str()).join(id.name.as_str()).join(version)
    }

    #[must_use]
    pub fn skill_md(&self, id: &SkillId, version: &str) -> Utf8PathBuf {
        self.skill_dir(id, version).join("SKILL.md")
    }

    #[must_use]
    pub fn profile_path(&self, name: &str) -> Utf8PathBuf {
        self.profiles_dir().join(format!("{name}.toml"))
    }

    #[must_use]
    pub fn trusted_path(&self) -> Utf8PathBuf {
        self.root.join("trusted.json")
    }

    /// 读取信任清单。
    pub fn read_trusted(&self) -> Result<TrustedList> {
        let p = self.trusted_path();
        match fs_err::read_to_string(p.as_std_path()) {
            Ok(s) => serde_json::from_str(&s)
                .map_err(|e| Error::other(format!("trusted.json parse: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(TrustedList::default()),
            Err(e) => Err(Error::Io { path: p, source: e }),
        }
    }

    /// 持久化信任清单。
    pub fn write_trusted(&self, list: &TrustedList) -> Result<()> {
        self.ensure_dirs()?;
        let p = self.trusted_path();
        let s = serde_json::to_string_pretty(list)
            .map_err(|e| Error::other(format!("trusted.json serialize: {e}")))?;
        fs_err::write(p.as_std_path(), s).map_err(|e| Error::Io { path: p, source: e })
    }

    /// 项目根目录是否已被用户信任。
    pub fn is_trusted(&self, project_root: &Utf8Path) -> Result<bool> {
        let canonical = canonicalize_for_trust(project_root);
        let list = self.read_trusted()?;
        Ok(list.projects.iter().any(|p| canonicalize_for_trust(p) == canonical))
    }

    /// 添加项目到信任清单（幂等）。
    pub fn trust(&self, project_root: &Utf8Path) -> Result<()> {
        let canonical = canonicalize_for_trust(project_root);
        let mut list = self.read_trusted()?;
        if !list.projects.iter().any(|p| canonicalize_for_trust(p) == canonical) {
            list.projects.push(canonical);
            list.projects.sort();
            self.write_trusted(&list)?;
        }
        Ok(())
    }

    /// 从信任清单移除项目（幂等）。
    pub fn untrust(&self, project_root: &Utf8Path) -> Result<()> {
        let canonical = canonicalize_for_trust(project_root);
        let mut list = self.read_trusted()?;
        let before = list.projects.len();
        list.projects.retain(|p| canonicalize_for_trust(p) != canonical);
        if list.projects.len() != before {
            self.write_trusted(&list)?;
        }
        Ok(())
    }
}

/// 信任清单。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustedList {
    #[serde(default)]
    pub projects: Vec<Utf8PathBuf>,
}

fn canonicalize_for_trust(p: &Utf8Path) -> Utf8PathBuf {
    // 优先 std 的 canonicalize；失败时退化为原路径
    match std::fs::canonicalize(p.as_std_path()) {
        Ok(c) => Utf8PathBuf::from_path_buf(c).unwrap_or_else(|_| p.to_owned()),
        Err(_) => p.to_owned(),
    }
}

// 占位实现块结束
impl GlobalStore {
    /// 读取全局 registry.lock（不存在时返回空）。
    pub fn read_registry(&self) -> Result<RegistryLock> {
        let path = self.registry_lock_path();
        match fs_err::read_to_string(path.as_std_path()) {
            Ok(s) => {
                toml::from_str(&s).map_err(|e| Error::other(format!("registry.lock parse: {e}")))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RegistryLock::default()),
            Err(e) => Err(Error::Io { path, source: e }),
        }
    }

    pub fn write_registry(&self, reg: &RegistryLock) -> Result<()> {
        self.ensure_dirs()?;
        let path = self.registry_lock_path();
        let s = toml::to_string_pretty(reg)
            .map_err(|e| Error::other(format!("registry.lock serialize: {e}")))?;
        fs_err::write(path.as_std_path(), s).map_err(|e| Error::Io { path, source: e })
    }

    /// 解析全局 ID 短名 → 完整 SkillId。
    pub fn resolve_short(&self, query: &str) -> Result<SkillId> {
        if let Ok(id) = SkillId::parse(query) {
            return Ok(id);
        }
        // 短名匹配 registry
        let reg = self.read_registry()?;
        let matches: Vec<&GlobalEntry> = reg.skills.values().filter(|e| e.name == query).collect();
        match matches.len() {
            0 => Err(Error::SkillNotFound(query.into())),
            1 => SkillId::parse(&matches[0].id),
            _ => Err(Error::AmbiguousSkillName {
                name: query.into(),
                matches: matches.iter().map(|e| e.id.clone()).collect(),
            }),
        }
    }
}

// ───────────────────────── 项目仓库 ─────────────────────────

/// 项目仓库根目录（含 `skillctl.toml`）。
#[derive(Debug, Clone)]
pub struct ProjectStore {
    pub root: Utf8PathBuf,
}

impl ProjectStore {
    /// 沿 cwd 向上查找包含 `skillctl.toml` 的目录。
    pub fn discover(start: &Utf8Path) -> Result<Option<Self>> {
        let mut cur = Some(start);
        while let Some(dir) = cur {
            if dir.join("skillctl.toml").exists() {
                return Ok(Some(Self { root: dir.to_owned() }));
            }
            cur = dir.parent();
        }
        Ok(None)
    }

    /// 强制以指定目录为项目根（不要求 manifest 存在，用于 init）。
    #[must_use]
    pub fn at(root: Utf8PathBuf) -> Self {
        Self { root }
    }

    #[must_use]
    pub fn manifest_path(&self) -> Utf8PathBuf {
        self.root.join("skillctl.toml")
    }
    #[must_use]
    pub fn lock_path(&self) -> Utf8PathBuf {
        self.root.join("skillctl.lock")
    }
    #[must_use]
    pub fn agents_md_path(&self) -> Utf8PathBuf {
        self.root.join("AGENTS.md")
    }
    #[must_use]
    pub fn index_path(&self) -> Utf8PathBuf {
        self.root.join(".skillctl").join("project-index.json")
    }

    pub fn read_manifest(&self) -> Result<Manifest> {
        let path = self.manifest_path();
        let s = fs_err::read_to_string(path.as_std_path())
            .map_err(|e| Error::Io { path: path.clone(), source: e })?;
        toml::from_str(&s).map_err(|e| Error::other(format!("manifest parse: {e}")))
    }

    pub fn write_manifest(&self, m: &Manifest) -> Result<()> {
        let path = self.manifest_path();
        let s = toml::to_string_pretty(m)
            .map_err(|e| Error::other(format!("manifest serialize: {e}")))?;
        fs_err::write(path.as_std_path(), s).map_err(|e| Error::Io { path, source: e })
    }

    pub fn read_lock(&self) -> Result<Lock> {
        let path = self.lock_path();
        match fs_err::read_to_string(path.as_std_path()) {
            Ok(s) => toml::from_str(&s).map_err(|e| Error::other(format!("lock parse: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Lock::default()),
            Err(e) => Err(Error::Io { path, source: e }),
        }
    }

    pub fn write_lock(&self, lock: &Lock) -> Result<()> {
        let path = self.lock_path();
        let s = toml::to_string_pretty(lock)
            .map_err(|e| Error::other(format!("lock serialize: {e}")))?;
        fs_err::write(path.as_std_path(), s).map_err(|e| Error::Io { path, source: e })
    }

    /// 在项目启用的技能中按 ID 或短名解析。
    pub fn resolve_skill(&self, manifest: &Manifest, query: &str) -> Result<(String, Tier)> {
        // 完整 id 直接命中
        if SkillId::parse(query).is_ok() {
            for (id, _, tier) in manifest.iter_skills() {
                if id == query {
                    return Ok((id.clone(), tier));
                }
            }
            return Err(Error::SkillNotFound(query.into()));
        }
        // 短名：在项目 skills 中找 name 段相等的条目
        let mut hits: Vec<(String, Tier)> = Vec::new();
        for (id_str, _, tier) in manifest.iter_skills() {
            if let Ok(id) = SkillId::parse(id_str) {
                if id.name.as_str() == query {
                    hits.push((id_str.clone(), tier));
                }
            }
        }
        match hits.len() {
            0 => Err(Error::SkillNotFound(query.into())),
            1 => Ok(hits.remove(0)),
            _ => Err(Error::AmbiguousSkillName {
                name: query.into(),
                matches: hits.into_iter().map(|(s, _)| s).collect(),
            }),
        }
    }
}

// ───────────────────────── 工具 ─────────────────────────

/// 计算字节的 sha256 hex（带 `sha256:` 前缀）。
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut out = String::with_capacity(7 + 64);
    out.push_str("sha256:");
    for b in digest.as_slice() {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn discover_walks_up() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        fs_err::write(root.join("skillctl.toml").as_std_path(), "[project]\nname = \"x\"\n")
            .unwrap();
        let nested = root.join("a/b/c");
        fs_err::create_dir_all(nested.as_std_path()).unwrap();
        let store = ProjectStore::discover(&nested).unwrap().expect("found");
        assert_eq!(store.root, root);
    }

    #[test]
    fn manifest_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let store = ProjectStore::at(root);
        let mut m = Manifest {
            project: ProjectMeta {
                name: "demo".into(),
                started_from: vec!["rust-cli".into()],
                linked_to: vec![],
            },
            skills: ManifestSkills::default(),
            agent: AgentConfig::default(),
        };
        m.skills.core.insert("deps".into(), SkillSpec::Version("*".into()));
        store.write_manifest(&m).unwrap();
        let back = store.read_manifest().unwrap();
        assert_eq!(back.project.name, "demo");
        assert_eq!(back.skills.core.len(), 1);
    }
}
