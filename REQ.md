# skillctl — 需求规格说明书

> 版本：v0.2.0  
> 语言：Rust  
> 定位：面向多 Agent 的全局 Skill 管理器、项目级 Skill 选择器与稳定上下文入口生成器

---

## 1. 项目背景

Agent 技能（`SKILL.md`）生态正在形成事实标准。不同 Agent（Claude Code、Codex、Cursor、Copilot、Cline、Windsurf 等）对技能、项目指令和上下文入口的支持方式并不完全一致：有的支持专用 skills 目录，有的主要依赖 `AGENTS.md` / `CLAUDE.md` / project instructions，有的只适合通过 shell 命令按需读取技能内容。

现有工具大多聚焦在“把一个 `SKILL.md` 安装到某个 Agent 目录”或“同步团队 Agent 配置”，但仍存在几个基础问题：

| 问题 | 描述 |
|------|------|
| 同名冲突 | `deploy`、`review`、`frontend`、`rust` 等技能名称极易冲突 |
| 全局与项目混淆 | 用户希望全局管理很多技术，但项目只启用其中一部分 |
| 上下文污染 | 新安装全局技能不应自动进入所有项目的 Agent 上下文 |
| 缓存不稳定 | 频繁改写 `AGENTS.md` / `CLAUDE.md` 会降低 prompt/context cache 命中概率 |
| 规范割裂 | 各 Agent 的 skills 目录规范不统一，直接同步目录会带来兼容成本 |
| 版本不可复现 | 团队和 CI 需要通过 lock file 使用完全一致的技能版本 |
| token 成本高 | Agent 不应启动时读取所有完整 `SKILL.md` |

`skillctl` 的目标不是再做一个 Agent-specific skills installer，而是作为一层稳定的本地能力管理协议：

> 全局安装技能；项目显式选择技能；通过稳定的 `AGENTS.md` 协议入口让 Agent 按需查询、描述和加载技能。

---

## 2. 产品定位

`skillctl` 是一个面向 AI Agent 的本地 Skill 管理器，提供：

1. **全局技能库**：用户可以全局安装和管理 N 个技术/流程技能。
2. **项目级选择**：每个项目只 `use` 与自身相关的技能。
3. **namespace 唯一标识**：通过 `namespace/name` 解决同名技能冲突。
4. **稳定 Agent 入口**：通过 `skillctl enable` 向 `AGENTS.md` 注入稳定协议，不注入动态技能列表。
5. **按需加载**：Agent 先通过 `list --project` 获取摘要，再按需 `describe` / `show` 完整技能。
6. **版本锁定**：全局与项目均支持 lock file，保证可复现。
7. **上下文缓存友好**：全局安装、项目选择、技能更新默认不修改 `AGENTS.md`。

核心一句话：

> `init` 起手项目，`add` 管全局，`use` 微调项目，`enable` 管 Agent 入口。

skillctl 以 **profile** 为人面主路径：用户用 profile 把"我每个 Rust 项目都要这套技能"显式策划成一个起手包，新项目 `init <profile>` 一键就位。Agent 端不感知 profile 概念，只看到协议层最终输出的扁平技能列表。

---

## 3. 设计原则

- **显式策划优先**：skillctl 服务于"明确知道自己要哪些技能"的用户，不做 Agent 端的隐式发现。
- **profile 是人面主路径**：常见流程是 `init <profile>` 一键起手，TUI 多选与逐个 `use` 是后期微调手段，非首选。
- **全局默认**：`skillctl add` 默认安装到全局仓库，不修改当前项目。
- **项目显式启用**：项目通过 `skillctl init` 或 `skillctl use` 显式选择要暴露给 Agent 的技能。
- **稳定上下文**：`skillctl enable` 只注入稳定协议块，不列出动态技能清单。
- **namespace-first**：技能唯一 ID 使用 `namespace/name`，短名只用于展示和无歧义调用。
- **Agent-first**：查询命令默认支持 JSON 输出，分级信息密度，减少 token 消耗。
- **不依赖 Agent skills 目录**：多 Agent 支持通过 `AGENTS.md` 协议入口实现，而非同步到各 Agent 私有 skills 目录。
- **兼容 SKILL.md**：完全兼容现有 `SKILL.md` frontmatter 与正文格式，扩展字段向后兼容。
- **本地优先**：本地查询、展示、启用、索引完全离线可用。
- **可审计而非天然可信**：技能本地可审计、可校验 checksum，但不宣称自动可信。

---

## 4. 核心概念

### 4.0 Profile

**Profile 是一组技能的起手包**，用于把用户对"这一类项目我要这些技能"的策划沉淀下来，新项目一键应用。

```text
~/.skillctl/profiles/
  rust-cli.toml
  tauri-app.toml
  nextjs-saas.toml
  myorg-base.toml
```

profile 文件结构示例：

```toml
[profile]
name = "rust-cli"
description = "Rust CLI 项目起手包"

[skills.core]
"deps" = "*"
"frontend-design" = "*"

[skills.extra]
"rust/review" = "1.2.0"
"github/actions" = "0.3.0"
```

要点：

- `core` 段：进入项目即在场的技能，Agent 默认视为已加载。
- `extra` 段：对 Agent 可见的扩展技能，按需 `show` 加载。
- profile 可来源于本地、GitHub、URL（与 skill 来源解析共用）。
- profile 之间不嵌套，组合在项目层完成（`init A` 后 `use ...` 加东西，或多次 `init` 合并）。
- 没有"默认 profile"——`skillctl init` 不带参数就是空白项目，profile 必须显式指定。

### 4.1 Skill

一个 Skill 是一个包含 `SKILL.md` 的能力包，可以包含说明、命令、脚本、资源文件和外部依赖声明。

### 4.2 Skill ID

技能唯一标识采用：

```text
namespace/name
```

示例：

```text
official/rust-review
anthropic/frontend-design
myorg/deploy
yourname/imgctl
local/custom-deploy
```

字段含义：

| 字段 | 含义 |
|------|------|
| `id` | 稳定唯一 ID，例如 `myorg/deploy` |
| `namespace` | 冲突域，例如 `myorg` |
| `name` | 短名，例如 `deploy` |
| `source` | 来源，例如 GitHub、本地路径、URL、registry |

当短名无冲突时允许使用短名；当存在冲突时必须使用完整 ID。

### 4.3 Global Skill Store

全局技能仓库位于：

```text
~/.skillctl/
```

用于保存用户安装过的所有技能。

### 4.4 Project Skill Selection

项目不复制全部全局技能，只声明当前项目使用哪些技能：

```text
skillctl.toml
skillctl.lock
.skillctl/project-index.json
```

### 4.5 Agent Entry

Agent 不直接读取全局技能库，也不依赖各 Agent 的 skills 目录。项目通过 `AGENTS.md` 中的稳定协议块告诉 Agent：

```bash
skillctl list --project --format json
skillctl describe <skill-id> --format json
skillctl show <skill-id>
```

---

## 5. SKILL.md 标准格式

兼容现有生态标准：

```markdown
---
name: imgctl
version: "1.2.0"
description: 图片处理工具，支持格式转换、编辑标注、Mermaid渲染。处理图片相关任务时使用。
license: MIT
compatibility: requires imgctl binary
metadata:
  author: yourname
  tags: [image, diagram, annotation]
  languages: ["*"]
---

# imgctl

## When to use
...

## Commands
...
```

`skillctl` 扩展字段：

```yaml
metadata:
  namespace: yourname
  author: yourname
  tags: [image, diagram]
  languages: ["rust", "python", "*"]
  summary: "处理图片：格式转换/缩放裁剪/文字箭头/Mermaid渲染"
  triggers: [image, screenshot, diagram, mermaid]
  homepage: https://github.com/yourname/imgctl
  requires:
    binaries:
      - name: imgctl
        version: ">=1.0.0"
    env: []
    runtimes: []
```

规则：

- `summary` 建议不超过 80 个中文字符或 160 个英文字符。
- `languages` 缺省时视为 `["*"]`。
- `summary` 缺省时从 `description` 生成。
- `namespace` 可由安装命令 `--as namespace/name` 覆盖。
- 扩展字段只能增强功能，不影响基础兼容性。

---

## 6. 目录结构

### 6.1 全局目录

```text
~/.skillctl/
  skills/
    yourname/
      imgctl/
        1.2.0/
          SKILL.md
          meta.json
          checksum.sha256
        1.1.0/
          SKILL.md
          meta.json
    myorg/
      deploy/
        b7e3a1c9/
          SKILL.md
          meta.json
  registry.lock
  global-index.json
  config.toml
  sources.toml
```

### 6.2 项目目录

```text
your-repo/
  AGENTS.md
  skillctl.toml
  skillctl.lock
  .skillctl/
    project-index.json
```

说明：

- `skillctl.toml`：项目声明，人工维护。
- `skillctl.lock`：项目锁定，自动生成，应提交到 git。
- `.skillctl/project-index.json`：项目启用技能索引，可重新生成。
- `AGENTS.md`：由 `skillctl enable` 注入稳定协议块。

---

## 7. 命令总览

### 7.1 人类常用命令

| 命令 | 作用 |
|------|------|
| `skillctl init [profile]` | 起手新项目；带 profile 则应用其技能集与配置 |
| `skillctl profile add` | 添加全局 profile（来源同 skill：本地/GitHub/URL） |
| `skillctl profile list` | 列出全局已安装 profile |
| `skillctl profile show <name>` | 展示 profile 内容 |
| `skillctl add` | 添加技能到全局仓库 |
| `skillctl remove` | 从全局仓库移除技能 |
| `skillctl use` | 当前项目微调技能列表，支持 TUI 多选 |
| `skillctl enable` | 向当前项目 `AGENTS.md` 注入稳定协议入口 |
| `skillctl disable` | 从 `AGENTS.md` 移除 skillctl 协议入口 |
| `skillctl list` | 列出全局或项目技能 |
| `skillctl update` | 更新全局或项目锁定版本 |
| `skillctl restore` | 按 lock file 恢复技能 |

### 7.2 Agent 常用命令

| 命令 | 作用 |
|------|------|
| `skillctl list --project --format json` | 获取当前项目启用技能摘要 |
| `skillctl describe <id> --format json` | 获取结构化能力说明 |
| `skillctl show <id>` | 输出完整 `SKILL.md` |
| `skillctl path <id>` | 输出 `SKILL.md` 路径 |
| `skillctl doctor <id>` | 检查该技能外部依赖 |

### 7.3 维护命令

| 命令 | 作用 |
|------|------|
| `skillctl validate` | 验证 `SKILL.md` 格式 |
| `skillctl verify` | 校验 checksum |
| `skillctl inspect` | 审计技能元数据、脚本、外部依赖和风险 |
| `skillctl init` | 初始化项目配置，可引导 `use` / `enable` |
| `skillctl create` | 创建本地技能模板 |
| `skillctl export` | 导出技能目录 |

---

## 8. 命令规格

### 8.0 `skillctl init`

起手项目，是 skillctl 的人面主入口。

```bash
skillctl init                       # 空白项目：写最小 skillctl.toml + AGENTS.md 入口块
skillctl init rust-cli              # 用 rust-cli profile 起手（拷贝、断链）
skillctl init rust-cli --link       # 用 rust-cli 起手并保持联动（profile 升级时同步）
skillctl init rust-cli react-frontend  # 多 profile 合并起手（core/extra 取并集）
```

默认行为：

- 写入 `skillctl.toml`，包含 `[skills.core]` 与 `[skills.extra]` 两段。
- 写入 `skillctl.lock`，锁定每个技能版本到全局仓库当前版本。
- 写入 `.skillctl/project-index.json`。
- 调用 `enable` 注入 `AGENTS.md` 协议入口块。
- 若 profile 引用的技能不在全局仓库，**报错并提示** `skillctl add <id>`，不静默拉取。

`--link` 模式：在 manifest 写入 `linked_to = ["rust-cli"]`。`skillctl restore` 与 `skillctl update` 会根据该字段重新对齐 profile 当前内容。

JSON 输出示例：

```json
{
  "success": true,
  "action": "init",
  "started_from": ["rust-cli"],
  "linked_to": [],
  "skills": {
    "core": ["deps", "frontend-design"],
    "extra": ["rust/review", "github/actions"]
  },
  "updated": ["skillctl.toml", "skillctl.lock", ".skillctl/project-index.json", "AGENTS.md"]
}
```

冲突处理：

- 项目已存在 `skillctl.toml`：默认拒绝，需 `--force`。
- profile 中的技能与已有项目技能冲突：`init` 仅用于起手，冲突场景应使用 `use --add` 增量添加。

### 8.0bis `skillctl profile *`

profile 自身的全局管理，与 `add/remove/list` 对 skill 的关系平行。

```bash
skillctl profile add github:myorg/profiles/rust-cli --as myorg/rust-cli
skillctl profile add ./my-profiles/tauri.toml --as local/tauri
skillctl profile list
skillctl profile show rust-cli
skillctl profile remove rust-cli
```

profile 存储于 `~/.skillctl/profiles/<name>.toml`，与全局技能仓库分离但共享来源解析逻辑。profile 自身不需要锁定版本（其内部技能引用才需要锁定），但会记录 source 与 fetched_at 以便 `update` 重新拉取。

### 8.1 `skillctl add`

将技能添加到全局仓库。

```bash
skillctl add imgctl
skillctl add imgctl@1.2.0
skillctl add github:yourname/imgctl --as yourname/imgctl
skillctl add github:myorg/agent-skills/deploy --as myorg/deploy
skillctl add ./local/my-skill --as local/my-skill
skillctl add https://example.com/SKILL.md --as example/imgctl
```

默认行为：

- 安装到 `~/.skillctl/`。
- 更新 `~/.skillctl/registry.lock`。
- 更新 `~/.skillctl/global-index.json`。
- 不修改当前项目。
- 不修改 `AGENTS.md`。
- 不影响任何项目的上下文缓存。

JSON 输出示例：

```json
{
  "success": true,
  "action": "add",
  "skill": {
    "id": "yourname/imgctl",
    "namespace": "yourname",
    "name": "imgctl",
    "version": "1.2.0",
    "source": "github:yourname/imgctl",
    "commit": "a3f8c2d1b4e9f203b7e3a1c9"
  },
  "updated": [
    "~/.skillctl/registry.lock",
    "~/.skillctl/global-index.json"
  ],
  "project_changed": false,
  "agents_md_changed": false
}
```

### 8.2 `skillctl remove`

从全局仓库移除技能。

```bash
skillctl remove yourname/imgctl
skillctl remove imgctl@1.1.0
skillctl remove yourname/imgctl --all-versions
```

规则：

- 如果某些项目 lock file 引用了该技能，默认只警告，不自动修改项目。
- 可通过 `--force` 强制删除。
- 不自动修改项目 `AGENTS.md`。

### 8.3 `skillctl use`

让当前项目使用一组全局已安装技能。

交互式：

```bash
skillctl use
```

行为：

- 打开 TUI 多选列表。
- 默认展示全局已安装技能。
- 支持搜索、按标签/语言过滤、空格选择、回车保存。

TUI 示例：

```text
Project: ~/work/my-tauri-app

Select skills to use in this project:

Technology
  [x] rust/review          Rust 代码审查、clippy、unsafe 检查
  [x] tauri/app            Tauri 桌面应用开发、权限、打包
  [ ] typescript/frontend  TypeScript/React 前端开发
  [ ] android/sdk          Android、Gradle、ADB
  [ ] docker/ci            Dockerfile、CI、镜像优化

Space select/unselect · Enter save · / search · q cancel
```

非交互式：

```bash
skillctl use rust/review tauri/app
skillctl use --add docker/ci
skillctl use --remove android/sdk
skillctl use --clear
```

默认行为：

- 更新项目 `skillctl.toml`。
- 更新项目 `skillctl.lock`。
- 更新 `.skillctl/project-index.json`。
- 不修改 `AGENTS.md`。

JSON 输出示例：

```json
{
  "success": true,
  "action": "use",
  "project": "/path/to/my-tauri-app",
  "enabled": ["rust/review", "tauri/app"],
  "updated": [
    "skillctl.toml",
    "skillctl.lock",
    ".skillctl/project-index.json"
  ],
  "agents_md_changed": false
}
```

### 8.4 `skillctl enable`

在当前项目启用 skillctl Agent 协议入口。

```bash
skillctl enable
skillctl enable --target AGENTS.md
skillctl enable --target CLAUDE.md
skillctl enable --upgrade-block
```

默认行为：

- 在 `AGENTS.md` 中插入 managed block。
- 若 block 已存在且版本兼容，则不修改文件。
- 注入内容为稳定协议，不包含动态技能列表。
- 不读取全局所有技能。
- 不因为 `add` / `use` / `update` 自动变化。

`AGENTS.md` 注入块示例：

```markdown
<!-- skillctl:start version=1 -->
## skillctl

This project uses skillctl to manage Agent skills.

Rules:
- Use only project-enabled skills.
- Before non-trivial development work, inspect available project skills with:
  `skillctl list --project --format json`
- Load full skill instructions only when needed:
  `skillctl show <skill-id>`
- Inspect structured metadata when deciding whether to use a skill:
  `skillctl describe <skill-id> --format json`
- Do not use global skills unless the user explicitly asks.

<!-- skillctl:end -->
```

### 8.5 `skillctl disable`

移除当前项目中的 skillctl Agent 协议入口。

```bash
skillctl disable
skillctl disable --target AGENTS.md
```

只移除 managed block，不删除 `skillctl.toml`、`skillctl.lock` 或项目索引。

### 8.6 `skillctl list`

列出技能。

```bash
skillctl list
skillctl list --global
skillctl list --project
skillctl list --lang rust
skillctl list --tag image
skillctl list --format json
skillctl list --format table
```

默认：

- 在项目目录内，默认等价于 `skillctl list --project`。
- 非项目目录内，默认等价于 `skillctl list --global`。

JSON 输出示例：

```json
{
  "success": true,
  "scope": "project",
  "count": 2,
  "skills": [
    {
      "id": "rust/review",
      "namespace": "rust",
      "name": "review",
      "version": "1.2.0",
      "summary": "Rust 代码审查、clippy、unsafe 检查、错误处理建议",
      "languages": ["rust"],
      "scope": "project"
    },
    {
      "id": "tauri/app",
      "namespace": "tauri",
      "name": "app",
      "version": "0.4.1",
      "summary": "Tauri 桌面应用开发、权限、打包、插件配置",
      "languages": ["rust", "typescript"],
      "scope": "project"
    }
  ]
}
```

### 8.7 `skillctl describe`

分级信息密度查询。

```bash
skillctl describe rust/review
skillctl describe rust/review --detail
skillctl describe rust/review --format json
```

Level 1：摘要。

```json
{
  "id": "rust/review",
  "name": "review",
  "namespace": "rust",
  "version": "1.2.0",
  "summary": "Rust 代码审查、clippy、unsafe 检查、错误处理建议"
}
```

Level 2：结构化能力。

```json
{
  "id": "rust/review",
  "description": "Rust 代码审查与最佳实践技能",
  "commands": ["cargo check", "cargo clippy", "cargo test"],
  "tags": ["rust", "review", "quality"],
  "languages": ["rust"],
  "triggers": ["rust", "clippy", "unsafe", "review"],
  "requires": {
    "binaries": [
      { "name": "cargo", "version": ">=1.75" }
    ],
    "env": [],
    "runtimes": []
  },
  "summary": "Rust 代码审查、clippy、unsafe 检查、错误处理建议"
}
```

### 8.8 `skillctl show`

输出完整 `SKILL.md`。

```bash
skillctl show rust/review
skillctl show rust/review --path
```

规则：

- 默认优先查找当前项目启用技能。
- 如果短名存在歧义，返回 `ambiguous_skill_name` 错误。
- `--path` 只输出文件路径，由 Agent 自行读取。

歧义错误示例：

```json
{
  "success": false,
  "error": "ambiguous_skill_name",
  "name": "deploy",
  "matches": ["myorg/deploy", "local/deploy"],
  "hint": "Use skillctl show myorg/deploy"
}
```

### 8.9 `skillctl restore`

按 lock file 恢复技能。

```bash
skillctl restore
skillctl restore --global
skillctl restore --project
```

默认行为：

- 在项目目录内恢复项目 `skillctl.lock` 中声明的技能。
- 非项目目录内恢复全局 `registry.lock`。
- 更新本地 index。
- 不修改 `AGENTS.md`。

### 8.10 `skillctl update`

更新技能版本。

```bash
skillctl update
skillctl update rust/review
skillctl update --global
skillctl update --project
skillctl update rust/review --breaking
```

规则：

- 全局更新修改 `~/.skillctl/registry.lock`。
- 项目更新修改 `skillctl.lock`。
- 默认不修改 `AGENTS.md`。
- git ref 类型更新到最新 commit，并记录新的 commit hash。

### 8.11 `skillctl doctor`

检查技能外部依赖。

```bash
skillctl doctor
skillctl doctor rust/review
skillctl doctor --project
```

输出示例：

```json
{
  "success": true,
  "skills": [
    {
      "id": "rust/review",
      "checks": [
        { "type": "binary", "name": "cargo", "required": ">=1.75", "found": "1.86.0", "ok": true }
      ]
    }
  ]
}
```

---

## 9. Manifest 与 Lock File

### 9.1 全局 lock：`~/.skillctl/registry.lock`

记录全局仓库中已安装技能的精确事实。

```toml
[skills."yourname/imgctl"]
id = "yourname/imgctl"
namespace = "yourname"
name = "imgctl"
version = "1.2.0"
source = "github"
repo = "yourname/imgctl"
path = "skills/imgctl"
ref = "v1.2.0"
commit = "a3f8c2d1b4e9f203b7e3a1c9"
checksum = "sha256:..."
summary = "图片处理：格式转换/缩放裁剪/文字箭头/Mermaid渲染"
resolved_at = "2026-04-27T00:00:00Z"
```

### 9.2 项目 manifest：`skillctl.toml`

表达当前项目使用哪些技能。

```toml
[project]
name = "my-tauri-app"
started_from = ["rust-cli"]   # 起手用的 profile，仅信息记录
linked_to = []                 # --link 模式下含 profile 名，会被 restore/update 同步

[skills.core]
"deps" = "*"
"rust/review" = "1.2.0"

[skills.extra]
"docker/ci" = "1.0.0"
"local/our-deploy" = { path = "./skills/deploy" }

[agent]
enabled = true
target = "AGENTS.md"
```

要点：

- `[skills.core]` 与 `[skills.extra]` 对应协议层 `tier` 字段（见 PROTOCOL §3.1）。
- `started_from` 是只读痕迹，不影响运行时行为。
- `linked_to` 非空时，profile 仍是"父级"，更新会流入。
- 复杂项目（rust + react）直接在 core/extra 列出全部技能，不依赖 profile 嵌套。

### 9.3 项目 lock：`skillctl.lock`

记录当前项目启用技能的精确版本。

```toml
[skills."rust/review"]
id = "rust/review"
namespace = "rust"
name = "review"
version = "1.2.0"
tier = "core"
source = "global"
global_ref = "rust/review@1.2.0"
checksum = "sha256:..."
summary = "Rust 代码审查、clippy、unsafe 检查、错误处理建议"

[skills."tauri/app"]
id = "tauri/app"
namespace = "tauri"
name = "app"
version = "0.4.1"
tier = "extra"
source = "global"
global_ref = "tauri/app@0.4.1"
checksum = "sha256:..."
summary = "Tauri 桌面应用开发、权限、打包、插件配置"
```

规则：

- 项目 `skillctl.lock` 应提交到 git。
- `restore` 应能仅凭 lock file 精确恢复项目所需技能。
- lock file 不应依赖动态 latest。

---

## 10. Agent 调用工作流

### 10.1 项目配置流程

人类执行：

```bash
skillctl add rust/review
skillctl add tauri/app

cd my-tauri-app
skillctl enable
skillctl use
```

或非交互式：

```bash
skillctl use rust/review tauri/app
skillctl enable
```

### 10.2 Agent 运行流程

Agent 读取 `AGENTS.md` 后，看到 skillctl 协议入口。

典型流程：

```bash
skillctl list --project --format json
```

根据摘要判断是否需要某个技能：

```bash
skillctl describe rust/review --format json
```

必要时加载全文：

```bash
skillctl show rust/review
```

### 10.3 Token 策略

| 阶段 | 内容 | 预估 token |
|------|------|------------|
| `AGENTS.md` stable block | 固定协议，不含技能列表 | 很低，长期稳定 |
| `list --project` | name + summary | 约 20 token / skill |
| `describe --detail` | 结构化能力 | 约 100 token / skill |
| `show` | 完整 `SKILL.md` | 按需加载 |

---

## 11. 上下文缓存策略

设计目标：避免全局技能变化影响项目 Agent 上下文。

规则：

| 操作 | 是否修改 `AGENTS.md` | 原因 |
|------|----------------------|------|
| `skillctl init` | 是，首次写入入口块（含 `started_from=` 标识） | 项目首次接入 skillctl |
| `skillctl add` | 否 | 全局安装不应影响项目上下文 |
| `skillctl remove` | 否 | 全局删除不应重写项目入口 |
| `skillctl use` | 否 | 项目技能列表变化写入 index，不写入上下文入口 |
| `skillctl update` | 否 | 版本变化写入 lock/index，不写入入口 |
| `skillctl restore` | 否 | 恢复依赖不改变协议 |
| `skillctl profile *` | 否 | profile 全局管理与项目入口无关 |
| `skillctl enable` | 是，仅首次或升级协议块 | 明确启用 Agent 入口（`init` 已含此步） |
| `skillctl disable` | 是 | 明确移除 Agent 入口 |

`AGENTS.md` 中只保留稳定协议，不嵌入动态技能列表。这样新安装全局技术、项目切换技能、更新技能版本，都不会自动改变 Agent 启动上下文，有利于 prompt/context cache 命中。

可选模式：

```bash
skillctl enable --embed-summary
```

该模式会把项目启用技能摘要写入 `AGENTS.md`，适合小项目，但会降低缓存稳定性。默认不启用。

---

## 12. 多 Agent 支持策略

`skillctl` 不再默认同步到各 Agent 的 skills 目录。

废弃默认方案：

```text
~/.claude/skills/
~/.cursor/skills/
~/.codex/skills/
.claude/skills/
.cursor/skills/
```

推荐方案：

```text
AGENTS.md stable protocol
  → skillctl list --project
  → skillctl describe
  → skillctl show
```

适配方式：

| Agent | 推荐方式 |
|------|----------|
| Codex | 直接读取 `AGENTS.md` |
| Claude Code | 在 `CLAUDE.md` 中引用或同步 `AGENTS.md` 协议块 |
| Cursor / Cline / Windsurf | 通过项目 instructions 或 `AGENTS.md` 兼容层读取协议 |
| 其他 Agent | 只需支持读取项目指令和执行 shell 命令 |

可选兼容命令后置：

```bash
skillctl export-agent claude
skillctl export-agent cursor
```

该能力仅作为兼容层，不作为 MVP 默认路径。

---

## 13. 技术栈检测（辅助提示，非主路径）

> **降级说明**：本节内容刻意从主流程中移除。skillctl 不做"根据项目文件自动推荐技能"——这违反"显式策划优先"原则。检测逻辑仅在以下两个**辅助场景**中浮现，且永远不自动启用任何技能：
>
> 1. `skillctl init --suggest`：扫描项目文件，建议适合的 profile，由用户手动确认。
> 2. `skillctl use` 的 TUI 中作为列表排序的**弱信号**之一。

`skillctl use` 的 TUI 可以根据项目文件辅助排序全局已安装技能。

检测逻辑：

| 文件 | 推断 |
|------|------|
| `Cargo.toml` | rust |
| `tauri.conf.json` / `src-tauri/` | tauri |
| `package.json` | javascript/typescript |
| `pyproject.toml` | python |
| `go.mod` | go |
| `Dockerfile` | docker |
| `.github/workflows/` | github-actions |
| `android/` / `build.gradle` | android |
| `ios/` / `Package.swift` | ios/swift |

推荐原则：

- 语言只是弱信号。
- 文件结构、工具链、框架、当前任务关键词是更强信号。
- 推荐只影响 TUI 排序，不自动启用技能。

---

## 14. 安全与信任模型

`skillctl` 认为技能是本地可审计资源，不假设所有技能天然可信。

### 14.1 风险分类

| 类型 | 描述 |
|------|------|
| instruction-only | 只有 `SKILL.md` 文本 |
| script skill | 包含脚本文件 |
| binary-dependent | 依赖外部二进制 |
| network-dependent | 执行流程可能访问网络 |
| secret-dependent | 依赖环境变量或密钥 |

### 14.2 审计命令

```bash
skillctl inspect rust/review
skillctl verify rust/review
skillctl doctor rust/review
```

### 14.3 安装策略

- 默认记录 checksum。
- 默认不执行技能内脚本。
- 执行脚本类技能时，Agent 应先读取 `describe` / `doctor`。
- 后续可加入 trust policy：

```bash
skillctl trust myorg
skillctl trust github:myorg/agent-skills
```

---

## 15. 版本管理

### 15.1 支持的版本引用

```bash
skillctl add imgctl@1.2.0
skillctl add imgctl@^1.0
skillctl add github:yourname/imgctl@main --as yourname/imgctl
skillctl add github:yourname/imgctl@v1.2.0 --as yourname/imgctl
skillctl add github:yourname/imgctl@a3f8c2d --as yourname/imgctl
```

优先级：

1. 精确 commit hash
2. tag
3. branch/ref
4. semver range
5. latest

lock file 始终记录精确事实：commit hash 或 checksum。

### 15.2 `update` 行为

- semver 技能：在约束范围内更新。
- git branch/ref 技能：拉取最新 commit 并更新 lock。
- pinned 技能：默认跳过。
- `--breaking` 才允许跨 major 更新。

---

## 16. 注册源与来源

MVP 支持：

```text
GitHub repo/path
local path
URL to SKILL.md
```

后续支持：

```text
registry
GitHub tap
local registry
private source
```

`sources.toml` 示例：

```toml
[sources.default]
type = "github"
repo = "yourname/skills"

[sources.myorg]
type = "github"
repo = "myorg/agent-skills"

[sources.local]
type = "local"
path = "~/my-private-skills"
```

官方 registry 与 tap/publish 能力后置，不进入 MVP。

---

## 17. Rust 项目结构

```text
skillctl/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── commands/
│   │   ├── add.rs
│   │   ├── remove.rs
│   │   ├── use_cmd.rs
│   │   ├── enable.rs
│   │   ├── disable.rs
│   │   ├── list.rs
│   │   ├── describe.rs
│   │   ├── show.rs
│   │   ├── path.rs
│   │   ├── restore.rs
│   │   ├── update.rs
│   │   ├── doctor.rs
│   │   ├── validate.rs
│   │   ├── verify.rs
│   │   └── inspect.rs
│   ├── store/
│   │   ├── global.rs
│   │   ├── project.rs
│   │   ├── index.rs
│   │   └── lock.rs
│   ├── id/
│   │   └── skill_id.rs
│   ├── skill/
│   │   ├── parser.rs
│   │   ├── validator.rs
│   │   ├── meta.rs
│   │   └── summary.rs
│   ├── source/
│   │   ├── github.rs
│   │   ├── local.rs
│   │   └── url.rs
│   ├── version/
│   │   ├── semver.rs
│   │   ├── gitref.rs
│   │   └── resolver.rs
│   ├── agent/
│   │   ├── agents_md.rs
│   │   └── managed_block.rs
│   ├── detect/
│   │   └── stack.rs
│   ├── tui/
│   │   └── use_selector.rs
│   └── output.rs
└── tests/
    ├── add.rs
    ├── use.rs
    ├── enable.rs
    ├── lock.rs
    ├── id.rs
    ├── list.rs
    └── describe.rs
```

---

## 18. 技术依赖建议

```toml
[dependencies]
clap        = { version = "4", features = ["derive"] }
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
toml        = "0.8"
semver      = "1"
anyhow      = "1"
thiserror   = "1"
gray_matter = "0.2"
walkdir     = "2"
sha2        = "0.10"
dirs        = "5"
which       = "6"
ratatui     = "0.29"
crossterm   = "0.28"
```

网络与 Git 依赖建议分层：

- 本地高频命令：`list` / `describe` / `show` / `enable` 不初始化网络与 Git 逻辑。
- 安装更新命令：`add` / `update` / `restore` 才加载网络与 Git 能力。
- 若使用 `git2` / `reqwest`，需接受二进制体积与冷启动影响。
- 可考虑通过系统 `git` 实现 MVP，降低二进制复杂度。

---

## 19. 非功能需求

| 指标 | 目标 |
|------|------|
| 本地查询冷启动 | < 10ms |
| `list --project` 响应 | < 10ms，纯本地读取 |
| `describe` 响应 | < 10ms，纯本地读取 |
| `show` 响应 | < 20ms，纯本地读取 |
| 二进制大小 | MVP 尽量 < 15MB |
| 平台支持 | Linux x86_64/aarch64，macOS Apple Silicon/Intel，Windows 后续支持 |
| 离线能力 | 本地查询、展示、启用、索引完全离线 |
| AGENTS.md 稳定性 | `add` / `use` / `update` / `restore` 默认不改写 |

---

## 20. 开发优先级

### Phase 1 — 起手包 + 协议网关 MVP

人面主路径：

- `skillctl add`：GitHub、本地路径、URL。
- `skillctl profile add` / `list` / `show`：profile 全局管理。
- `skillctl init [profile...]`：项目起手主入口，包含 enable 的全部行为。
- `skillctl use`：非交互式增删（TUI 推到 Phase 2）。
- `skillctl enable` / `disable`：稳定 `AGENTS.md` block 的独立命令（用于已存在项目接入）。

Agent 协议表面：

- `skillctl list --project --format json`（含 `tier` 字段）。
- `skillctl describe --format json`。
- `skillctl show`。
- `skillctl path`。

基础设施：

- namespace 解析与冲突处理。
- 全局 `registry.lock`。
- 项目 `skillctl.toml`（含 `[skills.core]` / `[skills.extra]`）/ `skillctl.lock` / `.skillctl/project-index.json`。
- `started_from` / `linked_to` manifest 字段。

### Phase 2 — 版本、恢复与微调

- semver range。
- git ref / tag / commit hash。
- `skillctl restore`（含 `linked_to` 模式同步 profile）。
- `skillctl update`。
- `skillctl remove`。
- `skillctl profile remove` / `update`。
- `skillctl use` TUI 多选界面（含弱栈检测排序）。
- checksum verify。
- `doctor`。

### Phase 3 — 推荐与安全

- 项目技术栈检测。
- `use` TUI 推荐排序。
- `inspect` 风险审计。
- trust policy。
- `validate` 增强。

### Phase 4 — 生态扩展

- registry / source。
- GitHub tap。
- `publish`。
- `create`。
- `export`。
- Agent-specific export 兼容层。

---

## 21. MVP 验收场景

### 场景 1：全局添加技能

```bash
skillctl add github:yourname/imgctl/skills/imgctl --as yourname/imgctl
skillctl list --global
```

验收：技能出现在全局列表中，不修改当前项目文件。

### 场景 2：用 profile 起手新项目

```bash
skillctl profile add github:myorg/profiles/rust-cli --as rust-cli
cd my-new-cli
skillctl init rust-cli
skillctl list --project --format json
```

验收：`AGENTS.md` 出现稳定 managed block，`skillctl.toml` 含 `started_from = ["rust-cli"]` 与 core/extra 两段，`list --project` 输出每条技能带 `tier` 字段。

### 场景 3：复杂项目重新编排

```bash
cd my-tauri-react-app
skillctl init rust-cli
skillctl use --add react/best-practices tauri/perms
skillctl list --project --format json
```

验收：项目 manifest 中 core/extra 同时含 rust-cli 起手内容与后加的两个技能，`started_from` 仅记录 `["rust-cli"]`。

### 场景 3bis：手动接入已有项目

```bash
cd legacy-project
skillctl enable
skillctl use rust/review docker/ci
```

验收：`AGENTS.md` 出现 managed block，项目 manifest 仅含手动选择的技能，`started_from` 为空数组。

### 场景 4：Agent 按需加载

```bash
skillctl list --project --format json
skillctl describe yourname/imgctl --format json
skillctl show yourname/imgctl
```

验收：Agent 可先看到摘要，再按需读取完整 `SKILL.md`。

### 场景 5：同名冲突

```bash
skillctl add github:myorg/skills/deploy --as myorg/deploy
skillctl add ./skills/deploy --as local/deploy
skillctl show deploy
```

验收：返回 `ambiguous_skill_name`，提示使用完整 ID。

### 场景 6：缓存稳定

```bash
sha256sum AGENTS.md
skillctl add rust/review
skillctl use rust/review
skillctl update rust/review
sha256sum AGENTS.md
```

验收：除非显式 `enable --upgrade-block`，`AGENTS.md` hash 不变。

---

## 22. 与工具家族的关系

```text
skillctl   → 全局技能仓库、项目技能选择、Agent 稳定入口
imgctl     → 图片处理技能或外部工具
axctl      → 界面感知技能或外部工具
```

`imgctl`、`axctl` 等工具可以作为技能依赖被 `skillctl doctor` 检查，也可以通过 `SKILL.md` 被 Agent 按需调用。

---

## 23. 非目标

MVP 不做：

- 默认同步到各 Agent 私有 skills 目录。
- 官方中心 registry。
- tap/publish 生态。
- 自动把所有全局技能注入项目上下文。
- 自动根据语言启用技能。
- 自动信任远程技能。
- 在 `AGENTS.md` 中默认嵌入动态技能列表。

---

## 24. 总结

`skillctl` 的最终设计从“本地 Agent 技能包管理器”收敛为：

> 面向多 Agent 的全局 Skill 管理器、项目级 Skill 选择器与稳定上下文入口生成器。

核心 UX：

```bash
skillctl profile add github:myorg/profiles/rust-cli --as rust-cli   # 全局安装 profile
skillctl add rust/review                                             # 全局添加技能
skillctl init rust-cli                                               # 当前项目用 profile 起手
skillctl use --add docker/ci                                         # 微调
```

Agent UX：

```bash
skillctl list --project --format json
skillctl describe <skill-id> --format json
skillctl show <skill-id>
```

这个设计同时解决：

- 同名技能冲突；
- 全局技能管理；
- 项目级按需启用；
- 多 Agent 规范不统一；
- `AGENTS.md` 稳定性；
- prompt/context cache 友好；
- 技能版本可复现；
- Agent token 成本控制。
