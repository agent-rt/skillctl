# skillctl Agent Skill 协议

> 版本：1  
> 状态：草案  
> 日期：2026-04-27  
> 范围：定义 AI Agent 通过网关 CLI 独占式地发现、描述与加载本地受管技能的协议规范。

---

## 0. 为什么要新建一套协议

其他 Agent skill 方案（如 `.agents/skills/` 文件系统扫描约定）假设每个 Agent 客户端各自实现发现、解析、冲突消解和信任校验。结果是一个黑盒：客户端之间行为不一致，命名冲突由不可见的目录优先级隐式裁决，没有共享的版本与可复现性概念。

`skillctl` 采取相反立场。所有技能解析集中在一个网关进程内。Agent 不读取用户文件系统，只执行四个命令并信任其输出。这放弃了生态互操作性，换取：

- **可观测性** —— 用户可运行与 Agent 相同的命令，看到 Agent 看到的全部内容。
- **确定性** —— 命名空间冲突、版本锁定、lock file 恢复均显式表达，不是隐式行为。
- **稳定上下文** —— 项目 `AGENTS.md` 中的入口块在技能增删期间字节稳定，保持 prompt cache 命中。
- **单一信任边界** —— 由网关而非每个 Agent 决定什么是合法、当前、项目启用的技能。

本文档定义该协议。`skillctl` 是参考实现；任何字节兼容 §3 与 §4 的工具都是合规网关。

## 1. 术语

- **Skill** —— 由带 YAML frontmatter 的 `SKILL.md` 文件描述的能力单元。
- **Skill ID** —— `namespace/name`。强制必填，全局唯一。协议层面不存在"裸名"形式。
- **Global store（全局仓库）** —— 私有的已安装技能仓库，对 Agent 不可见。
- **Project selection（项目选择）** —— 当前项目显式启用的全局技能子集。
- **Gateway CLI（网关 CLI）** —— Agent 调用的可执行程序。参考实现为 `skillctl`。
- **Entry block（入口块）** —— 位于 `AGENTS.md` 中的稳定受管区域，声明 Agent 如何与网关通信。**唯一被认可的发现路径。**

## 2. 分层

```
┌─────────────────────────────────────────────────┐
│ Agent (Claude Code, Codex, Cursor, …)           │
│   读取 AGENTS.md → 识别入口块                    │
│   通过 shell 调用执行网关命令                    │
└──────────────────┬──────────────────────────────┘
                   │  协议表面 (§3)
┌──────────────────▼──────────────────────────────┐
│ Gateway CLI (skillctl 或任何合规实现)            │
│   解析项目选择 → 读取全局仓库                    │
│   处理命名空间、版本、信任、解析逻辑              │
└──────────────────┬──────────────────────────────┘
                   │  私有 —— Agent 永远不读取
┌──────────────────▼──────────────────────────────┐
│ 全局技能仓库 + 项目 manifest / lock              │
│   路径布局由实现决定                             │
└─────────────────────────────────────────────────┘
```

Agent 只看到协议表面。网关是唯一权威：存储布局、来源解析、版本选择和冲突处理全部隐藏在网关之下。

## 3. 协议表面

合规网关 **必须** 实现以下四个命令。输出格式为规范性约束。

### 3.1 `<gateway> list --project [--format json|tsv]`

返回当前工作目录所属项目启用的技能列表（Tier 1：catalog）。

```json
{
  "protocol": 1,
  "scope": "project",
  "count": 2,
  "skills": [
    {
      "id": "rust/review",
      "name": "review",
      "namespace": "rust",
      "version": "1.2.0",
      "tier": "core",
      "summary": "Rust 代码审查、clippy、unsafe 检查、错误处理建议",
      "languages": ["rust"],
      "triggers": ["rust", "clippy", "unsafe", "review"]
    }
  ]
}
```

每条技能必填字段：`id`、`name`、`namespace`、`version`、`tier`、`summary`。  
可选字段：`languages`、`triggers`、`tags`。

`tier` 取值：

- `"core"` —— 核心技能。用户在项目级 manifest 中显式标记为"始终在场"。Agent **应当** 视其指令为已生效，无需 `show` 即可在判断与执行中参考其能力。Agent 可选择在会话开始时主动 `show` 一次以加载完整正文。
- `"extra"` —— 扩展技能。Agent 在 catalog 中看到 summary，按需 `describe` / `show` 加载完整正文。

`tier` 字段映射项目 manifest 的 `[skills.core]` / `[skills.extra]` 两段（参考 REQ.md §9.2）。

**过滤规则（规范性）**：被禁用、`requires` 检查不通过、或在当前环境下不可用的技能，**必须** 完全从该列表中省略。不允许"列出后再拒绝"——Agent 看到的应当只有可用技能。

token 目标（JSON）：每条技能 ≤ 100 tokens（最小记录 ≤ 30 tokens）。Agent 首先调用此命令以决定加载哪些技能。

#### TSV 格式（Agent 推荐）

JSON 信封对 Agent 是冗余的——`protocol`、`scope`、`count`、`name`、`namespace`、`version` 等字段要么已知，要么可从 `id` 推导，要么 Agent 不关心。TSV 提供同样的语义信息但只保留 4 列：

```text
TIER\tID\tSUMMARY\tTRIGGERS
core\trust/review\tRust 代码审查...\trust,clippy,unsafe
extra\tdocker/ci\tDockerfile 优化...\tdocker,dockerfile,ci
```

字段：

- `TIER` —— `core` 或 `extra`。
- `ID` —— 完整 `namespace/name`。
- `SUMMARY` —— 单行摘要，已清理 tab/换行。
- `TRIGGERS` —— 逗号分隔的触发关键词（可空）。

第一行始终是表头 `TIER\tID\tSUMMARY\tTRIGGERS`。其余每行一个技能。值不加引号；列内 tab/换行预先替换为单空格。

token 节省：典型项目（5-20 个技能）TSV 比 JSON 紧凑 **3-5 倍**，这是 Agent 每会话开头都会读的内容，节省直接传导到上下文。

错误时 TSV 输出单行：

```text
ERROR\t<error_code>\t<hint>
```

退出码非零。`error_code` 与 §3.5 错误信封中的 `error` 字段同集合。

### 3.2 `<gateway> describe <id> --format json`

返回单个技能的结构化能力描述——足以判断是否需加载完整正文，但不为正文付出 token 代价（Tier 2a：结构化元数据）。

```json
{
  "protocol": 1,
  "id": "rust/review",
  "name": "review",
  "namespace": "rust",
  "version": "1.2.0",
  "summary": "...",
  "description": "...",
  "commands": ["cargo check", "cargo clippy", "cargo test"],
  "tags": ["rust", "review"],
  "languages": ["rust"],
  "triggers": ["rust", "clippy", "unsafe"],
  "requires": {
    "binaries": [{ "name": "cargo", "version": ">=1.75" }],
    "env": [],
    "runtimes": []
  },
  "resources": [
    "scripts/check.sh",
    "references/clippy-rules.md"
  ]
}
```

token 目标：≤ 200 tokens。**不包含** `SKILL.md` 正文。

`resources` 字段枚举技能所附带的文件，但 **不返回其内容**。Agent 在技能正文引用某资源时，再用自己的 file-read 工具按需加载。网关 **应当** 对过长列表截断并给出提示。

### 3.3 `<gateway> show <id>`

将完整 `SKILL.md`（frontmatter + 正文）写入 stdout（Tier 2b：instructions）。Agent 将其作为当前任务的指令摄入。

解析规则：
1. 完整 `namespace/name` ID **始终** 被接受且不会歧义。
2. 仅当项目启用技能中恰好有一个匹配时，才接受裸 `name`；否则返回 §3.5 `ambiguous_skill_name`。
3. 全局（未在项目启用）的技能不会被 `show` 解析，除非显式传入 `--global` 标志。

建议：`SKILL.md` 正文目标 < 5000 tokens。超过此体量的内容应拆分为资源文件，通过 §3.2 暴露。

### 3.4 `<gateway> path <id>`

将解析后的 `SKILL.md` 绝对路径写入 stdout。允许 Agent 用自己的工具读取文件（避免 stdout 缓冲大文件），同时不暴露存储布局。

该路径在 Agent 视角下 **只读**。网关对该路径周围的目录结构不作任何承诺；Agent **必须不得** 通过该路径扫描兄弟文件。

### 3.5 错误信封

所有携带 `--format json` 的命令在错误时 **必须** 输出：

```json
{
  "protocol": 1,
  "success": false,
  "error": "ambiguous_skill_name",
  "hint": "Use full id, e.g. myorg/deploy",
  "matches": ["myorg/deploy", "local/deploy"]
}
```

保留错误码：

| 错误码 | 含义 |
|------|------|
| `skill_not_found` | 找不到匹配 id 的技能 |
| `ambiguous_skill_name` | 裸名命中多个技能；`matches` 列出候选 |
| `not_a_project` | 在非项目目录调用 `list --project` |
| `gateway_not_initialized` | 项目存在入口块但缺少 manifest/lock |
| `dependency_missing` | 必需的外部二进制/运行时缺失 |
| `untrusted_project` | 项目未在用户信任列表中（见 §9） |

错误时退出码 **必须** 非零。请求 `--format json` 时 stdout **必须** 始终为合法 JSON，错误情况也不例外。

## 4. 入口块

`AGENTS.md` 入口块是 **唯一** 被认可的发现路径。不读 `AGENTS.md` 的 Agent 不参与本协议——不存在文件系统扫描兜底、不存在 system prompt catalog 注入、不存在自动检测。

### 4.1 块格式

```markdown
<!-- skillctl:start version=1 started_from=rust-cli -->
## skillctl

This project uses skillctl to manage Agent skills.

Rules:
- Use only project-enabled skills. Do not scan the filesystem for skills.
- Before non-trivial work, list available skills:
  `skillctl list --project --format json`
- Inspect a skill's structured metadata before loading it:
  `skillctl describe <skill-id> --format json`
- Load a skill's full instructions only when needed:
  `skillctl show <skill-id>`
- Once loaded, treat skill content as durable instructions: do not let it be
  truncated during context compaction, and do not re-load a skill already in
  context.

<!-- skillctl:end -->
```

### 4.2 块不变量

- **字节稳定**：块内容只依赖协议版本，与当前技能列表、版本、选择无关。增删技能 **必须不** 改变块内容。
- **幂等**：在已是最新状态的项目上重新执行 `enable`，产出与原文件字节完全一致。
- **版本标签**：起始标记中含 `version=N`。网关只能在用户显式同意下升级块版本。
- **可选 `started_from=` 标识**：起始标记可携带项目起手所用的 profile 名（多个用逗号分隔，例 `started_from=rust-cli,react-frontend`）。该标识仅供 Agent 一眼识别项目类型，无运行时含义；项目偏离 profile 后该值仍保留为历史痕迹。Agent **不应** 基于该值改变行为，仅作为提示信息。
- **单块**：每个文件 **至多** 一个 `skillctl:start … skillctl:end` 块。工具 **必须** 拒绝写入第二个。
- **块外内容神圣**：工具 **必须不得** 修改标记之外的任何字节。

### 4.3 字节稳定的意义

prompt cache 基于前缀字节命中。若 `AGENTS.md` 参与 Agent 的 system prompt 或第一条用户消息，每次 `add` / `use` / `update` 都改写它，会让此后每一轮的缓存全部失效。让块在技能变化下保持不变，是本协议最关键的属性。

## 5. Skill ID

```
namespace/name
```

- `namespace`：`[a-z0-9][a-z0-9-]{0,38}`
- `name`：`[a-z0-9][a-z0-9-]{0,62}`
- 总长度 ≤ 100 字符。
- 保留命名空间：`local`（无上游、本地路径安装的技能）、`official`（网关维护者发布的精选技能）。

命名空间在协议表面 **强制必填**。其存在意义是把冲突消解从位置式变为显式式。当两个来源各自发布名为 `deploy` 的技能时，它们以 `myorg/deploy` 与 `acme/deploy` 共存，互不遮蔽，Agent 始终知道收到的是哪一个。

裸名解析（§3.3 规则 2）只是无歧义场景下的便捷写法，不是兜底路径——歧义裸名永远是错误，绝不静默裁决。

## 6. SKILL.md frontmatter

必填：

```yaml
---
name: review
version: "1.2.0"
description: 简短的可读描述。
---
```

网关感知的可选字段：

```yaml
metadata:
  namespace: rust              # 仓库层身份必填；可由 `add --as ns/name` 覆盖
  summary: "≤ 80 中文字符 / 160 ASCII 字符；由 `list --project` 展示"
  triggers: [rust, clippy]
  languages: ["rust"]          # "*" 或缺省 = 任意
  tags: [review, quality]
  requires:
    binaries: [{ name: cargo, version: ">=1.75" }]
    env: []
    runtimes: []
```

默认值：`summary` 缺省时由 `description` 派生；`languages` 缺省时为 `["*"]`。

### 6.1 宽松解析

网关 **必须** 容忍宽松书写的 frontmatter：

- **值中含未引号冒号**（`description: Use when: the user asks…`）—— 失败时尝试加引号或转 block scalar 后重试。
- **`name` 与父目录不一致** —— 警告但仍加载。网关用 `namespace/name` 作为正式身份，目录命名只是参考信息。
- **`name` 超过 64 字符** —— 警告但仍加载。
- **`description` 缺失或为空** —— 跳过该技能（catalog 披露必需）并记录诊断。
- **YAML 完全无法解析** —— 跳过该技能并记录诊断。

诊断由 `skillctl validate` 与 `skillctl doctor` 暴露，不在发现路径上抛错。

## 7. token 预算目标

非强制但建议：

| 表面 | token 目标 |
|------|-----------|
| 入口块（§4） | ≤ 200，恒定 |
| `list --project` 每条技能 | ≤ 100（最小 ≤ 30） |
| `describe` 每条技能 | ≤ 200 |
| `show` 正文 | ≤ 5000（超出部分应拆为资源文件） |

启用 20 个技能的项目，初始发现成本应 ≤ 2000 tokens，完整技能按需加载。

## 8. Agent 工作流（规范性）

```
1. Agent 读取 AGENTS.md → 识别入口块 → 获悉四个命令
2. 会话开始或首次非平凡任务时，Agent 执行：
     skillctl list --project --format json
3. 对所有 tier="core" 的技能：可在会话开始时一次性 `show`，
   将正文作为持久指令注入上下文（豁免压缩）
4. 对 tier="extra" 的技能：将 summary + triggers 作为候选池
   匹配当前任务；对命中候选可选执行：
     skillctl describe <id> --format json
5. 对最终选定的 extra 技能，执行：
     skillctl show <id>           # 或 `path <id>` 后用 file-read 工具读取
6. 把 SKILL.md 正文作为任务指令使用
```

Agent 端规则（实现本协议的 Agent **应当** 遵守）：

- **去重**：当前会话已加载的技能不应再次 `show`。
- **保护**：技能内容是持久行为指令。**应当** 豁免上下文压缩或截断。会话中途丢失技能指令会静默降级，且无可见错误。
- **不扫描文件系统**：不得在网关返回结果之外查找 `SKILL.md` 文件。网关是权威。
- **信任过滤**：未出现在 `list --project` 输出中的技能即视为不可用，不要尝试加载。
- **尊重 tier**：`core` 技能视为用户已显式策划的核心能力，加载阈值低；`extra` 技能按相关性判断后再加载。不要把 `extra` 当成"不重要"忽略，也不要把 `core` 当成"必须永远调用"——`core` 是"知识在场"，不是"必须使用"。

## 9. 信任与安全模型

`SKILL.md` 内容会被 Agent 当作指令对待，因此任何已安装技能本质上都是 prompt injection 入口。本协议以网关受控的显式信任机制处理：

- **项目信任门控**：用户首次在项目中执行 `skillctl use` 时，网关将该项目记录为已信任。在未信任时，`list --project` 返回 `untrusted_project`，Agent 收不到任何技能。这防止刚 clone 的仓库通过 `skillctl.toml` 静默激活技能。
- **校验和记录**：每个安装的技能版本都有记录的 `sha256`。`skillctl verify` 可检测篡改。
- **不自动执行脚本**：网关绝不运行技能附带的脚本。附带资源通过 §3.2 `resources` 暴露，但仅在 Agent 自有工具按需加载时才被读取。
- **审计接口**：`skillctl inspect <id>` 报告声明的依赖、脚本、网络/密钥使用提示与风险等级。

命名空间所有权 **不由本协议鉴权**。命名空间是冲突消解 token，不是身份声明。用户自行决定从哪些命名空间安装。

## 10. 合规性

本协议描述单一的 网关-Agent 契约。合规与否是二元的：要么字节兼容 §3 与 §4，要么不是。

合规网关 **必须**：

- 实现 §3 的全部四个命令，并使用规定的 JSON 信封。
- 维持 §4.2 的入口块不变量。
- 强制 `namespace/name` 必填（§5）。
- 强制项目信任门控（§9）。

合规网关 **可以**：

- 提供 §3 之外的命令（如 `add` / `use` / `enable` / `update` / `restore` / `inspect` / `doctor`）。
- 在自己的私有根目录下采用任意存储布局。
- 提供 JSON 之外的输出格式。

参与本协议的 Agent **必须**：

- 仅通过 `AGENTS.md` 入口块发现技能。
- 把网关输出视为权威。
- 遵循 §8 的 Agent 端规则。

## 11. 版本管理

- 协议版本为整数。本文档为版本 `1`。
- 不兼容变更递增版本号，并要求新的入口块 `version=N`。支持多个版本的网关 **必须** 选择项目现有块声明的版本。
- 兼容性新增（输出新增可选字段、新增错误码、新增可选元数据字段）不递增版本。

## 12. 非目标

- 中心化 registry 或包索引。
- 加密签名或供应链证明。
- 与 `.agents/skills/`、`.claude/skills/` 或任何文件系统扫描约定的跨工具互操作。**skillctl 管理的技能对文件系统扫描类 Agent 刻意不可见。** 选其一，不要混用。
- 把技能同步到 Agent 私有目录。
- 技能执行的运行时沙箱。
- 在入口块中嵌入动态技能列表。
- 鉴权式的命名空间所有权。

## 13. 参考实现

`skillctl` 是参考实现。CLI 规范（包含协议表面之外的人面命令 `add` / `use` / `enable` / `update` 等）见 `REQ.md`（v0.2.0）。本协议不约束 `skillctl` 的存储布局、lock 文件格式或来源解析逻辑——只约束 §3 与 §4 的契约。

## 14. 与其他方案的关系

业界存在其他 Agent skill 方案，最具代表性的是 agentskills.io 文档化的文件系统扫描约定（`.agents/skills/<name>/SKILL.md`，project / user / org 多 scope，由 Agent 原生发现）。`skillctl` **不与该约定兼容**，也不追求兼容。

### 14.1 借鉴的实践

以下设计选择借鉴自更广的 Agent skills 生态中已经成熟的模式，刻意纳入本协议：

- **三层渐进披露**作为概念模型（catalog → instructions → resources）。`skillctl` 在 catalog 与正文之间扩展出可选的 Tier 2a `describe` 用于结构化元数据。
- **宽松 YAML 解析**（§6.1）—— 引号兜底、对装饰性违规仅警告不失败。
- **项目信任门控**（§9）—— 加载项目声明的技能前需用户信任。
- **上下文保护建议**（§8）—— 技能内容豁免压缩。
- **激活去重**（§8）—— 已在上下文中的技能不再加载。
- **隐藏式过滤**（§3.1）—— 不可用技能从 catalog 中缺席，而非列出后拒绝。
- **资源枚举不主动读取**（§3.2 `resources`）。
- **token 预算建议**（§7）—— 各 Tier 显式数字目标。

### 14.2 刻意分歧的设计选择

- **网关唯一访问 vs. 文件系统扫描**。其他方案允许任何能读目录的 Agent 自行发现技能。`skillctl` 要求所有访问经过网关进程。存储路径私有，可不通知地变更。
- **强制 `namespace/name` vs. 裸 `name` + 优先级规则**。其他方案靠目录优先级（project > user）裁决冲突。`skillctl` 显式化命名空间，ID 层面不可能产生冲突。
- **`AGENTS.md` 入口块作为唯一入口 vs. 多发现路径**。其他方案并行支持 system prompt catalog、专用 activation 工具、文件系统扫描。`skillctl` 只定义一条路径。
- **版本与 lock file 一等公民**。其他方案没有版本语义，目录就是事实。`skillctl` 有显式 semver / git-ref 解析与可复现 lock。
- **私有仓库布局 vs. 公开 `.agents/skills/`**。`skillctl` 技能对文件系统扫描类 Agent 不可见。这是特性而非缺陷：网关是唯一知道当前已安装、当前版本、当前可信的实体。

### 14.3 选择指引

- 选文件系统扫描约定，当跨多 Agent 客户端的生态互操作是首要诉求，且各客户端的发现行为可接受现状。
- 选 `skillctl`，当可观测性（"我跑同样的命令，看到的就是 Agent 看到的"）、确定性（命名空间、版本、lock）、单一信任边界比互操作性更重要。

两者不为单项目内共存而设计。**二选一。**

---

## 附录 A —— Agent 最小伪代码

```python
def handle_user_task(task):
    if not has_skillctl_entry_block("AGENTS.md"):
        return proceed_without_skills(task)

    skills = json.loads(sh("skillctl list --project --format json"))["skills"]

    # core 技能：会话开始一次性加载
    for s in skills:
        if s["tier"] == "core" and s["id"] not in already_loaded:
            body = sh(f"skillctl show {s['id']}")
            inject_into_context(body, protect_from_compaction=True)
            already_loaded.add(s["id"])

    # extra 技能：按任务相关性按需加载
    extras = [s for s in skills if s["tier"] == "extra"]
    candidates = [s for s in extras if matches(task, s["summary"], s.get("triggers", []))]
    for s in candidates:
        if s["id"] in already_loaded:
            continue
        body = sh(f"skillctl show {s['id']}")
        inject_into_context(body, protect_from_compaction=True)
        already_loaded.add(s["id"])

    proceed_with_task(task)
```

## 附录 B —— 本协议刻意不规定的内容

为保持契约最小化，本文档刻意不约束以下事项：

- 网关如何在磁盘上存储技能。
- 网关如何解析版本、拉取来源、计算校验和。
- 网关如何实现 `add` / `use` / `enable` 或任何 §3 之外的命令。
- 网关是否提供 TUI、daemon、远程 API，或除 stdin/stdout JSON 之外的任何东西。
- Agent 客户端如何根据用户任务挑选"候选"技能——这是 Agent 自身推理的属性，不是协议的属性。

以上都是各合规网关与 Agent 的实现选择。
