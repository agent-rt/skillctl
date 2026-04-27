---
name: rust-review
version: "1.0.0"
description: Idiomatic Rust review and engineering protocol. Memory-safe, performant, no unwrap/expect/panic.
license: MIT
metadata:
  namespace: official
  summary: Rust 工程化审查：所有权、零拷贝、强类型建模、结构化错误、unsafe 审计
  tags: [rust, review, quality, idiomatic]
  languages: [rust]
  triggers: [rust, cargo, clippy, unsafe, ownership, lifetime, borrow]
  requires:
    binaries:
      - { name: cargo, version: ">=1.75" }
      - { name: rustc }
---

# Rust Review

## When to use

Apply this skill when reviewing or writing production Rust code. The goal is **idiomatic, memory-safe, and performant** code that survives `cargo clippy -- -D clippy::pedantic`.

## Engineering constraints

### A. 内存与所有权

- 禁止滥用 `.clone()`：除非跨线程所有权转移且 `Arc` 不适用，优先生命周期 (`'a`) 或引用。
- 零拷贝优先：`&str` / `&[u8]` 优于 `String` / `Vec<u8>`。
- 栈分配优先：除非数据大小编译期不可知或极大，避免不必要的 `Box`。

### B. 类型系统

- 非法状态不可达：用 `enum` / `struct` 建模，禁止多个 `bool` 或 `Option` 表达互斥状态。
- Newtype：基础类型 (`u64`, `String`) 包成 Newtype 防止误用。
- Trait：优先 `impl Trait` 静态分发；仅在异构集合时用 `&dyn Trait`。

### C. 错误处理

- 严禁 `unwrap()` / `expect()` / `panic!()`（生产代码）。
- 自定义 `Error` 枚举（`thiserror`）。
- `unsafe` 必须附 `// SAFETY:` 注释；非极致性能场景禁用。

## Commands

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo fmt --check
```

## Output expectations

每个核心函数的报告：
1. **复杂度**：时间 + 空间
2. **内存分配**：是否堆分配
3. **并发保障**：`Send` / `Sync`
