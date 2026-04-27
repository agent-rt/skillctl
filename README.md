# skillctl

> Profile-driven Agent skill manager and protocol gateway.

`skillctl` is a CLI for users who **already know which skills they want their AI agents to use**. It manages a global skill library, applies profiles to projects in one shot, and exposes a stable shell-level protocol so any Agent (Claude Code, Codex, Cursor, …) can list / describe / load skills via four commands documented in `AGENTS.md`.

```bash
# Curate a profile once
skillctl profile add github:agent-rt/skills/profiles/rust-cli.toml --as rust-cli

# Apply it to a new project
cd my-rust-cli
skillctl init rust-cli

# Agent in this project (per AGENTS.md):
skillctl list --project --format tsv          # 4-col catalog
skillctl describe rust/review --format json   # structured metadata
skillctl show rust/review                     # full SKILL.md
```

## Install

Pre-built binaries (macOS Apple Silicon / Intel, Linux x86_64): see [Releases](https://github.com/agent-rt/skillctl/releases).

Homebrew:

```bash
brew install agent-rt/tap/skillctl
```

From source (Rust 1.83+):

```bash
git clone https://github.com/agent-rt/skillctl
cd skillctl
cargo install --path apps/skillctl-cli --locked
```

## Position in the Agent-RT family

| Tool | Layer |
|---|---|
| [`skillctl`](https://github.com/agent-rt/skillctl) | Curated capability bundles (skills + profiles) |
| [`memoryctl`](https://github.com/agent-rt/memoryctl) | Persistent agent memory (lessons / decisions / facts) |
| [`acpctl`](https://github.com/agent-rt/acpctl) | Agent Client Protocol (ACP) agent invocation |

`skillctl` and `memoryctl` are **independent** — same `AGENTS.md` can host both managed blocks; either works without the other.

## Why not the existing `.agents/skills/` filesystem convention?

`skillctl` deliberately diverges from filesystem-scanning conventions to gain:

- **Observability** — run the same commands the Agent runs and see exactly the same output
- **Determinism** — `namespace/name` IDs make collisions explicit, never positional
- **Stable bytes** — `AGENTS.md` doesn't churn on every skill add/remove (prompt cache stays warm)
- **Reproducibility** — `skillctl.lock` pins versions for the team

See [`PROTOCOL.md`](https://github.com/agent-rt/skillctl/blob/main/PROTOCOL.md) for the full wire contract and conformance rules.

## Documentation

- [`REQ.md`](https://github.com/agent-rt/skillctl/blob/main/REQ.md) — product spec
- [`PROTOCOL.md`](https://github.com/agent-rt/skillctl/blob/main/PROTOCOL.md) — wire schema and Agent-side rules

## License

Apache-2.0
