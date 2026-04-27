---
name: skillctl-self
version: "0.1.0"
description: How to develop and operate skillctl itself (workspace conventions, protocol invariants, release).
license: MIT
metadata:
  namespace: local
  summary: skillctl 自身的开发约定：workspace 布局、协议不变量、release profile、测试策略
  tags: [skillctl, meta, workspace]
  languages: [rust]
  triggers: [skillctl, AGENTS.md, profile, registry]
  requires:
    binaries:
      - { name: cargo }
      - { name: git }
---

# skillctl Self

## When to use

When working ON skillctl (this repository), not when using skillctl. For consumer use, see `official/rust-review` etc.

## Workspace conventions

- 11 crates: `core` / `id` / `skill` / `profile` / `version` / `store` / `source` / `agent` / `detect` / `protocol` / `tui`，外加 `apps/skillctl-cli`。
- 内部依赖通过 `[workspace.dependencies]` path 引用，集中版本管理。
- `unsafe_code = "forbid"`、`clippy::unwrap_used / expect_used / panic = "deny"` 全 workspace 生效；测试模块用 `#[allow(...)]` 局部豁免。
- 每条新功能：先扩 wire schema (`skillctl-protocol`) → 实现存储/解析 → 写命令 → 集成测试。

## Protocol invariants（PROTOCOL.md §4.2 / §4.3）

- `AGENTS.md` 受管块字节稳定：增删技能、版本变化都不改块。`render_block` 的输出仅依赖 `version` 与 `started_from`。
- 幂等：`enable` / `init --force` 重复执行不修改文件字节。
- 块外内容神圣：`upsert` / `remove` 只触碰 marker 之间的字节。

修改 `agent::render_block` 必须同步加 snapshot 风格断言（见 `crates/skillctl-agent/src/lib.rs` 的 `tests::renders_stable_block`）。

## Testing

```bash
cargo test --workspace                    # 单元 + 集成
cargo test -p skillctl-cli --test cli     # 仅集成
cargo clippy --workspace --all-targets   # lint
```

集成测试 (`apps/skillctl-cli/tests/cli.rs`) 每个 case 起独立 `HOME` tempdir，运行真实子进程；不会污染用户 `~/.skillctl/`。

## Release

```bash
cargo build --release -p skillctl-cli
ls -lh target/release/skillctl   # 目标 < 15 MB
```

profile：`lto=fat / codegen-units=1 / strip=symbols / panic=abort / opt-level=z`。

## 来源支持

| 类型 | feature | 实现 |
|---|---|---|
| 本地路径 | 默认 | 直接拷贝 |
| `github:user/repo[/sub][@ref]` | `source-git-cli` (默认) | 调系统 `git clone --depth 1` |
| URL | `source-http`（未实现）| 需加 reqwest |

## 信任门控

`SKILLCTL_TRUST_GATE=1` 启用项目信任检查。`init` / `use` / `enable` / `trust` 自动加入信任清单。只读 Agent 命令（`list --project` / `describe` / `show` / `path` / `restore --project` / `doctor --project`）在门控启用且未信任时返回 `untrusted_project`。
