//! 项目技术栈检测。**仅辅助提示**，永不自动启用任何技能。
//!
//! 见 REQ.md §13。

#![forbid(unsafe_code)]

use camino::Utf8Path;

/// 检测到的栈痕迹。每个字段都是弱信号。
#[derive(Debug, Clone, Default)]
pub struct StackHint {
    pub rust: bool,
    pub tauri: bool,
    pub nodejs: bool,
    pub typescript: bool,
    pub python: bool,
    pub go: bool,
    pub docker: bool,
    pub github_actions: bool,
    pub android: bool,
    pub ios: bool,
}

impl StackHint {
    /// 返回所有命中的栈名（按字母序）。
    #[must_use]
    pub fn tags(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.rust {
            out.push("rust");
        }
        if self.tauri {
            out.push("tauri");
        }
        if self.nodejs {
            out.push("nodejs");
        }
        if self.typescript {
            out.push("typescript");
        }
        if self.python {
            out.push("python");
        }
        if self.go {
            out.push("go");
        }
        if self.docker {
            out.push("docker");
        }
        if self.github_actions {
            out.push("github-actions");
        }
        if self.android {
            out.push("android");
        }
        if self.ios {
            out.push("ios");
        }
        out
    }
}

/// 扫描项目根目录，返回栈痕迹。仅根据顶层文件，不递归。
#[must_use]
pub fn detect(root: &Utf8Path) -> StackHint {
    let exists = |p: &str| root.join(p).exists();
    let is_dir = |p: &str| root.join(p).is_dir();
    let mut h = StackHint::default();
    h.rust = exists("Cargo.toml");
    h.tauri = is_dir("src-tauri") || exists("tauri.conf.json");
    h.nodejs = exists("package.json");
    h.typescript = h.nodejs && exists("tsconfig.json");
    h.python = exists("pyproject.toml") || exists("requirements.txt") || exists("setup.py");
    h.go = exists("go.mod");
    h.docker = exists("Dockerfile") || exists("docker-compose.yml") || exists("compose.yaml");
    h.github_actions = is_dir(".github/workflows");
    h.android = is_dir("android") || exists("build.gradle") || exists("build.gradle.kts");
    h.ios = is_dir("ios") || exists("Package.swift") || exists("Podfile");
    h
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn detects_rust() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        std::fs::write(root.join("Cargo.toml").as_std_path(), "[package]\nname=\"x\"").unwrap();
        let h = detect(&root);
        assert!(h.rust);
        assert!(!h.go);
    }
}
