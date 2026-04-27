//! 端到端集成测试。
//!
//! 每个测试在隔离的 `HOME` 与项目目录下，从零跑完用户旅程：
//! profile add → add → init → list/describe/show → use 微调 → verify → remove。

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc,
    clippy::pedantic
)]

use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

const SAMPLE_SKILL: &str = r#"---
name: review
version: "1.2.0"
description: Rust 代码审查与最佳实践技能
metadata:
  namespace: rust
  summary: Rust 代码审查、clippy、unsafe 检查
  tags: [rust, review]
  languages: [rust]
  triggers: [rust, clippy, unsafe]
  requires:
    binaries:
      - { name: cargo, version: ">=1.75" }
---
# Rust Review
"#;

const DEPS_SKILL: &str = r#"---
name: deps
version: "0.1.0"
description: Dependency oracle helper.
metadata:
  namespace: tools
  summary: Query live package registries
  tags: [deps]
---
# deps
"#;

const PROFILE_RUST_CLI: &str = r#"
[profile]
name = "rust-cli"
description = "Rust CLI starter"

[skills.core]
"tools/deps" = "*"

[skills.extra]
"rust/review" = "1.2.0"
"#;

struct Harness {
    _home: TempDir,
    home_path: std::path::PathBuf,
    project: std::path::PathBuf,
    fixtures: std::path::PathBuf,
}

impl Harness {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let home_path = home.path().to_path_buf();
        let project = home_path.join("project1");
        let fixtures = home_path.join("fixtures");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(fixtures.join("sample-skill")).unwrap();
        std::fs::create_dir_all(fixtures.join("deps-skill")).unwrap();
        std::fs::write(fixtures.join("sample-skill/SKILL.md"), SAMPLE_SKILL).unwrap();
        std::fs::write(fixtures.join("deps-skill/SKILL.md"), DEPS_SKILL).unwrap();
        std::fs::write(fixtures.join("rust-cli.toml"), PROFILE_RUST_CLI).unwrap();
        Self { _home: home, home_path, project, fixtures }
    }

    fn cmd(&self, cwd: &Path) -> Command {
        let mut c = Command::cargo_bin("skillctl").unwrap();
        c.env("HOME", &self.home_path).env_remove("RUST_LOG").current_dir(cwd);
        c
    }

    fn run_json(&self, cwd: &Path, args: &[&str]) -> Value {
        let out = self.cmd(cwd).arg("--format").arg("json").args(args).output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() {
            // 错误信封也是合法 JSON
            return serde_json::from_str(&stdout).unwrap_or_else(|_| {
                panic!("non-JSON stderr: {}", String::from_utf8_lossy(&out.stderr))
            });
        }
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("bad json: {e}\n{stdout}"))
    }
}

#[test]
fn full_lifecycle() {
    let h = Harness::new();
    let proj = h.project.clone();

    // 1. profile add
    let r =
        h.run_json(&proj, &["profile", "add", h.fixtures.join("rust-cli.toml").to_str().unwrap()]);
    assert_eq!(r["success"], true);
    assert_eq!(r["name"], "rust-cli");

    // 2. add skills
    let r = h.run_json(
        &proj,
        &["add", h.fixtures.join("sample-skill").to_str().unwrap(), "--as", "rust/review"],
    );
    assert_eq!(r["success"], true);
    assert_eq!(r["skill"]["id"], "rust/review");

    let r = h.run_json(
        &proj,
        &["add", h.fixtures.join("deps-skill").to_str().unwrap(), "--as", "tools/deps"],
    );
    assert_eq!(r["skill"]["id"], "tools/deps");

    // 3. init with profile
    let r = h.run_json(&proj, &["init", "rust-cli"]);
    assert_eq!(r["success"], true);
    assert_eq!(r["started_from"][0], "rust-cli");
    let core: Vec<&str> =
        r["skills"]["core"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    let extra: Vec<&str> =
        r["skills"]["extra"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(core, vec!["tools/deps"]);
    assert_eq!(extra, vec!["rust/review"]);

    // 4. AGENTS.md byte-stable across re-init
    let agents_md = proj.join("AGENTS.md");
    let snap1 = std::fs::read(&agents_md).unwrap();
    h.run_json(&proj, &["init", "rust-cli", "--force"]);
    let snap2 = std::fs::read(&agents_md).unwrap();
    assert_eq!(snap1, snap2, "init --force must not change AGENTS.md bytes");

    // 5. list --project --format json with tier
    let r = h.run_json(&proj, &["list", "--project"]);
    assert_eq!(r["protocol"], 1);
    assert_eq!(r["scope"], "project");
    assert_eq!(r["count"], 2);
    let skills = r["skills"].as_array().unwrap();
    assert!(skills.iter().any(|s| s["id"] == "rust/review" && s["tier"] == "extra"));
    assert!(skills.iter().any(|s| s["id"] == "tools/deps" && s["tier"] == "core"));

    // 6. describe
    let r = h.run_json(&proj, &["describe", "rust/review"]);
    assert_eq!(r["id"], "rust/review");
    assert_eq!(r["tier"], "extra");
    assert_eq!(r["requires"]["binaries"][0]["name"], "cargo");

    // 7. show prints SKILL.md
    let out = h.cmd(&proj).args(["show", "rust/review"]).output().unwrap();
    assert!(out.status.success());
    let body = String::from_utf8(out.stdout).unwrap();
    assert!(body.contains("name: review"));
    assert!(body.contains("# Rust Review"));

    // 8. path returns absolute SKILL.md path
    let out = h.cmd(&proj).args(["path", "rust/review"]).output().unwrap();
    let p = String::from_utf8(out.stdout).unwrap();
    assert!(p.trim().ends_with("/SKILL.md"));

    // 9. AGENTS.md unchanged after `use`
    let snap_before = std::fs::read(&agents_md).unwrap();
    let r = h.run_json(&proj, &["use", "--remove", "rust/review"]);
    assert_eq!(r["success"], true);
    let snap_after = std::fs::read(&agents_md).unwrap();
    assert_eq!(snap_before, snap_after, "use must not touch AGENTS.md");

    // 10. verify
    let r = h.run_json(&proj, &["verify"]);
    assert_eq!(r["success"], true);
    for item in r["items"].as_array().unwrap() {
        assert_eq!(item["ok"], true);
    }

    // 11. remove from registry
    let r = h.run_json(&proj, &["remove", "tools/deps"]);
    assert_eq!(r["success"], true);

    // 12. list --global no longer contains tools/deps
    let r = h.run_json(&proj, &["list", "--global"]);
    let ids: Vec<&str> =
        r["skills"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert!(!ids.contains(&"tools/deps"));
}

#[test]
fn ambiguous_short_name_returns_error_envelope() {
    let h = Harness::new();
    let proj = h.project.clone();

    // 安装两个同 short-name 的技能：rust/review + acme/review
    h.run_json(
        &proj,
        &["add", h.fixtures.join("sample-skill").to_str().unwrap(), "--as", "rust/review"],
    );

    // 制造 acme/review
    let acme_dir = h.fixtures.join("acme-skill");
    std::fs::create_dir_all(&acme_dir).unwrap();
    std::fs::write(
        acme_dir.join("SKILL.md"),
        r#"---
name: review
version: "0.1.0"
description: Acme review skill.
metadata:
  namespace: acme
---
# acme
"#,
    )
    .unwrap();
    h.run_json(&proj, &["add", acme_dir.to_str().unwrap(), "--as", "acme/review"]);

    let r = h.run_json(&proj, &["show", "--global", "review"]);
    assert_eq!(r["success"], false);
    assert_eq!(r["error"], "ambiguous_skill_name");
    assert_eq!(r["matches"].as_array().unwrap().len(), 2);
}

#[test]
fn not_a_project_error_envelope() {
    let h = Harness::new();
    let unrelated = h.home_path.join("unrelated");
    std::fs::create_dir(&unrelated).unwrap();
    let r = h.run_json(&unrelated, &["list", "--project"]);
    assert_eq!(r["success"], false);
    assert_eq!(r["error"], "not_a_project");
}

#[test]
fn lenient_yaml_with_unquoted_colon() {
    let h = Harness::new();
    let dir = h.fixtures.join("loose-skill");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        r#"---
name: loose
version: "0.1.0"
description: Use this skill when: the user asks about PDFs
metadata:
  namespace: tools
---
# loose
"#,
    )
    .unwrap();

    let r = h.run_json(&h.project, &["add", dir.to_str().unwrap(), "--as", "tools/loose"]);
    assert_eq!(r["success"], true);
    assert_eq!(r["skill"]["id"], "tools/loose");
}

#[test]
fn enable_disable_idempotent() {
    let h = Harness::new();
    let proj = h.project.clone();

    h.run_json(&proj, &["enable"]);
    let s1 = std::fs::read(proj.join("AGENTS.md")).unwrap();
    h.run_json(&proj, &["enable"]);
    let s2 = std::fs::read(proj.join("AGENTS.md")).unwrap();
    assert_eq!(s1, s2);

    h.run_json(&proj, &["disable"]);
    h.run_json(&proj, &["disable"]); // 再调一次也不报错
                                     // disable 后重新 enable 应该再次产生稳定块
    h.run_json(&proj, &["enable"]);
    let s3 = std::fs::read(proj.join("AGENTS.md")).unwrap();
    // s1 与 s3 应字节相同（同样的 started_from 空集）
    assert_eq!(s1, s3);
}

#[test]
fn trust_gate_blocks_untrusted_then_allows_after_trust() {
    let h = Harness::new();
    let proj = h.project.clone();

    h.run_json(
        &proj,
        &["add", h.fixtures.join("sample-skill").to_str().unwrap(), "--as", "rust/review"],
    );
    h.run_json(&proj, &["init"]);
    h.run_json(&proj, &["use", "--add", "rust/review"]);

    // 强制重置信任清单为空
    let trusted_json = h.home_path.join(".skillctl/trusted.json");
    std::fs::write(&trusted_json, r#"{"projects":[]}"#).unwrap();

    // 关闭门控时仍可工作
    let r = h.run_json(&proj, &["list", "--project"]);
    assert_eq!(r["count"], 1);

    // 启用门控后未信任 → untrusted_project
    let mut c = h.cmd(&proj);
    c.env("SKILLCTL_TRUST_GATE", "1");
    let out = c.args(["--format", "json", "list", "--project"]).output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["success"], false);
    assert_eq!(v["error"], "untrusted_project");

    // trust 后再访问 OK
    let mut c = h.cmd(&proj);
    c.env("SKILLCTL_TRUST_GATE", "1");
    let out = c.args(["trust", "."]).output().unwrap();
    assert!(out.status.success());

    let mut c = h.cmd(&proj);
    c.env("SKILLCTL_TRUST_GATE", "1");
    let out = c.args(["--format", "json", "list", "--project"]).output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 1);
}

#[test]
fn restore_recovers_missing_skill() {
    let h = Harness::new();
    let proj = h.project.clone();

    h.run_json(&proj, &["profile", "add", h.fixtures.join("rust-cli.toml").to_str().unwrap()]);
    h.run_json(
        &proj,
        &["add", h.fixtures.join("sample-skill").to_str().unwrap(), "--as", "rust/review"],
    );
    h.run_json(
        &proj,
        &["add", h.fixtures.join("deps-skill").to_str().unwrap(), "--as", "tools/deps"],
    );
    h.run_json(&proj, &["init", "rust-cli"]);

    // 删除全局仓库中的 SKILL.md 以模拟丢失
    let target_md = h.home_path.join(".skillctl/skills/rust/review/1.2.0/SKILL.md");
    std::fs::remove_file(&target_md).unwrap();
    assert!(!target_md.exists());

    let r = h.run_json(&proj, &["restore", "--project"]);
    assert_eq!(r["success"], true);
    assert!(target_md.exists(), "restore should re-fetch missing skill");

    let restored: Vec<&str> =
        r["restored"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(restored.contains(&"rust/review"));
}
