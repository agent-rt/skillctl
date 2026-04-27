---
name: release-flow
version: "1.0.0"
description: Release pipeline and family conventions for Agent-RT *ctl tools (skillctl, memctl, imgctl, acpctl, mcpctl). Single Apache-2.0 binary, GitHub repo under agent-rt org, Homebrew tap distribution, AGENTS.md compatibility.
license: Apache-2.0
metadata:
  namespace: agent-rt
  summary: Agent-RT 发布规程：org/license/命名/CI/release.yml/Homebrew tap/site docs，含已知陷阱清单
  tags: [release, agent-rt, ci, homebrew, github-actions]
  languages: [rust, yaml]
  triggers: [agent-rt, ctl, release, homebrew, tap, formula, github-actions, cargo, brew]
  requires:
    binaries:
      - { name: cargo, version: ">=1.83" }
      - { name: gh }
      - { name: git }
      - { name: brew }
      - { name: depx }
---

# Agent-RT Release Flow

## When to use

Apply this skill when:

- Creating a new `*ctl` tool under the `agent-rt` GitHub org
- Setting up CI / release workflows for an existing Agent-RT crate
- Debugging release pipeline failures
- Onboarding a fork of an existing tool

## Family conventions (non-negotiable)

| Property | Value |
|---|---|
| GitHub org | `agent-rt` |
| License | Apache-2.0 (match family; avoid dual MIT/Apache for simplicity) |
| Naming | `<name>ctl`, prefer 6 chars total (`acpctl`/`mcpctl`/`imgctl`/`memctl`); 8 OK if no natural short form (`skillctl`) |
| Distribution | GitHub Releases (3 platforms) + `agent-rt/homebrew-tap` |
| Repository layout | Cargo workspace: `apps/<name>-cli` + `crates/<name>-*` |
| Binary name | Bare `<name>` (no `-cli` suffix); set via `[[bin]] name = "<name>"` in `apps/<name>-cli/Cargo.toml` |
| `AGENTS.md` block | Independent managed marker `<!-- <name>:start version=N -->`, byte-stable, idempotent |

## Cargo.toml workspace shape

```toml
[workspace]
resolver = "2"
members = ["apps/<name>-cli", "crates/<name>-*"]

[workspace.package]
version      = "0.1.0"
edition      = "2021"      # or "2024" if you have rust-version >= 1.85
rust-version = "1.83"
license      = "Apache-2.0"
repository   = "https://github.com/agent-rt/<name>"
homepage     = "https://github.com/agent-rt/<name>"
authors      = ["Agent-RT contributors"]

[workspace.lints.rust]
unsafe_code     = "forbid"
unused_must_use = "deny"

[workspace.lints.clippy]
unwrap_used   = "deny"
expect_used   = "deny"
panic         = "deny"
dbg_macro     = "deny"
pedantic      = { level = "warn", priority = -1 }
module_name_repetitions = "allow"
missing_errors_doc      = "allow"
missing_panics_doc      = "allow"

[profile.release]
lto             = "fat"
codegen-units   = 1
strip           = "symbols"
panic           = "abort"
opt-level       = "z"
```

## CI workflow (`.github/workflows/ci.yml`)

```yaml
name: CI
on:
  push: { branches: [main] }
  pull_request:

env:
  CARGO_TERM_COLOR: always
  # DO NOT add RUSTFLAGS=-D warnings — workspace [lints.clippy] pedantic=warn
  # combined with -D warnings turns ALL pedantic style warnings into CI errors.
  # Workspace deny-list (unsafe/unwrap/expect/panic/dbg) already gates correctness.

jobs:
  check:
    name: check / ${{ matrix.os }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt, clippy }
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets
      - run: cargo test --workspace --all-targets
```

## Release workflow (`.github/workflows/release.yml`)

Parameterize by `PRODUCT` (binary name) and `CRATE` (cargo `-p` target):

```yaml
name: Release
on:
  push: { tags: ['v*'] }
  workflow_dispatch:
    inputs:
      tag: { description: 'Tag to release', required: true }

permissions: { contents: write }

env:
  PRODUCT: <name>             # e.g. memctl
  CRATE: <name>-cli           # e.g. memctl-cli
  TAP_REPO: agent-rt/homebrew-tap

jobs:
  build:
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - { target: x86_64-unknown-linux-gnu, os: ubuntu-latest }
          - { target: x86_64-apple-darwin,      os: macos-latest }
          - { target: aarch64-apple-darwin,     os: macos-14 }
    steps:
      - uses: actions/checkout@v4
      - id: tag
        run: |
          TAG="${GITHUB_REF#refs/tags/}"
          [ -n "${{ github.event.inputs.tag }}" ] && TAG="${{ github.event.inputs.tag }}"
          echo "tag=$TAG" >> "$GITHUB_OUTPUT"
          echo "version=${TAG#v}" >> "$GITHUB_OUTPUT"
          echo "rel_tag=${{ env.PRODUCT }}-${TAG}" >> "$GITHUB_OUTPUT"
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: ${{ matrix.target }} }
      - uses: Swatinem/rust-cache@v2
        with: { shared-key: release-${{ matrix.target }} }
      - name: Build
        run: cargo build --release --locked -p ${{ env.CRATE }} --target ${{ matrix.target }}
      - name: Package
        id: pkg
        run: |
          set -euo pipefail
          NAME="${{ env.PRODUCT }}-${{ steps.tag.outputs.version }}-${{ matrix.target }}"
          mkdir -p "dist/${NAME}"
          cp "target/${{ matrix.target }}/release/${{ env.PRODUCT }}" "dist/${NAME}/"
          cp README.md LICENSE "dist/${NAME}/"
          cd dist
          tar -czf "${NAME}.tar.gz" "${NAME}"
          shasum -a 256 "${NAME}.tar.gz" > "${NAME}.tar.gz.sha256"
          echo "tarball=dist/${NAME}.tar.gz"        >> "$GITHUB_OUTPUT"
          echo "shafile=dist/${NAME}.tar.gz.sha256" >> "$GITHUB_OUTPUT"
      - uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ steps.tag.outputs.tag }}
          files: |
            ${{ steps.pkg.outputs.tarball }}
            ${{ steps.pkg.outputs.shafile }}
          generate_release_notes: true
      - name: Push to tap
        env: { GH_TOKEN: ${{ secrets.GH_DIST_TOKEN }} }
        run: |
          gh release create "${{ steps.tag.outputs.rel_tag }}" \
            --repo "${{ env.TAP_REPO }}" --prerelease \
            --title "${{ env.PRODUCT }} ${{ steps.tag.outputs.tag }}" \
            --notes "${{ env.PRODUCT }} ${{ steps.tag.outputs.tag }}" \
            "${{ steps.pkg.outputs.tarball }}" "${{ steps.pkg.outputs.shafile }}" \
            || gh release upload "${{ steps.tag.outputs.rel_tag }}" \
               --repo "${{ env.TAP_REPO }}" --clobber \
               "${{ steps.pkg.outputs.tarball }}" "${{ steps.pkg.outputs.shafile }}"

  update-formula:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - id: tag
        # ... same as above
      - id: sha
        env: { GH_TOKEN: ${{ secrets.GH_DIST_TOKEN }} }
        run: |
          # download .sha256 files, extract ARM_SHA / X86_SHA
      - name: Push Formula
        env: { GH_TOKEN: ${{ secrets.GH_DIST_TOKEN }} }
        run: |
          # generate Formula/<name>.rb with ARM/Intel URLs+SHAs
          # commit to tap main branch via agent-rt-bot
```

Full reference template: copy from `agent-rt/skillctl/.github/workflows/release.yml` and substitute `PRODUCT` / `CRATE` / Formula class name.

## Homebrew Formula structure

The release workflow generates this file at `agent-rt/homebrew-tap/Formula/<name>.rb`:

```ruby
class <Capitalized> < Formula
  desc "<one-line description>"
  homepage "https://github.com/agent-rt/<name>"
  version "<version>"
  license "Apache-2.0"

  on_macos do
    on_arm   { url "<aarch64 url>"; sha256 "<arm sha>" }
    on_intel { url "<x86_64 url>";  sha256 "<x86 sha>" }
  end

  def install
    bin.install "<name>"
  end

  test do
    assert_match "<name>", shell_output("#{bin}/<name> --version")
  end
end
```

## Site docs (`agent-rt/agent-rt.github.io/<name>/`)

Standard 4-page set per tool:

- `index.md` — product positioning, family relation table, get-started links
- `install.md` — Homebrew + prebuilt + cargo + verify
- `quickstart.md` — 5-7 step end-to-end walkthrough
- `commands.md` — full subcommand reference + wire schemas + env vars

Top-level `index.md` adds a `### [<name>](/<name>/)` section with the same shape as siblings.

## Release ritual

```bash
# 1. cd to repo, ensure working tree clean and CI green on main
git status && gh run list --workflow CI --limit 1

# 2. annotated tag (NOT lightweight — git rejects it without -m)
git -c tag.gpgsign=false tag -a v0.1.0 -m "<name> v0.1.0 — initial release"
git push origin v0.1.0

# 3. watch the release workflow (3 platforms × ~2-5 min each)
RUN=$(gh run list --workflow Release --limit 1 --json databaseId --jq '.[0].databaseId')
gh run watch $RUN --exit-status

# 4. verify artifacts
gh release view v0.1.0 --json assets --jq '[.assets[].name]'
gh api repos/agent-rt/homebrew-tap/contents/Formula --jq '.[].name'

# 5. install via brew tap and smoke test
brew tap agent-rt/tap
brew install <name>
<name> --version
```

## Bootstrapping a new `*ctl` tool

```bash
# 1. depx for live versions, never guess
depx -q crates:clap --latest
depx -q crates:thiserror --latest

# 2. cargo new + workspace shape
mkdir <name> && cd <name>
git init -q
mkdir -p apps/<name>-cli/src crates

# 3. copy templates from a reference repo
cp -r ~/ai-workspace/skillctl/.github .
cp ~/ai-workspace/skillctl/{Cargo.toml,rust-toolchain.toml,rustfmt.toml,LICENSE,.gitignore} .
# substitute names: skillctl → <name> in copied files

# 4. create gh repo + push initial
gh repo create agent-rt/<name> --public \
  --description "<one-liner>" \
  --homepage "https://github.com/agent-rt/<name>"
git remote add origin git@github.com:agent-rt/<name>.git
git add . && git commit -m "Initial release of <name>" && git push -u origin main

# 5. verify CI green, then tag v0.1.0
```

## Pitfalls (hit during real releases)

1. **`RUSTFLAGS: "-D warnings"` + workspace pedantic = CI deadlock**
   Workspace `[lints.clippy] pedantic = "warn"` plus env var `-D warnings` promotes ALL pedantic style nits into errors. Drop the env var; rely on workspace deny list.

2. **`cargo fmt --check 2>&1 | tail -3` masks exit code**
   Pipe to `tail` makes LHS exit 0, so `|| cargo fmt --all` fallback never fires. Either:
   `cargo fmt --check || cargo fmt` (no pipe), or check `${PIPESTATUS[0]}`.

3. **Lightweight tag rejected**
   `git tag v0.1.0` fails with "no tag message" if `tag.gpgsign` or related is set.
   Use annotated: `git -c tag.gpgsign=false tag -a v0.1.0 -m "..."`.

4. **Repo not yet on GitHub when first push**
   Order matters: `gh repo create agent-rt/<name>` BEFORE `git push -u origin main`.
   Or use `gh repo create --source=. --push`.

5. **Homebrew install hits HTTP 502 on tap CDN**
   Transient GitHub CDN issue, not a setup problem. Retry `brew install` once.

6. **Renaming a tool mid-flight**
   Use `git mv` for crate dirs, then `perl -pi -e 's/\boldname\b/newname/g'` (NOT BSD sed
   `\b` — broken on macOS) on `*.rs *.toml *.md *.yml`. Bump version. `gh repo rename`
   keeps redirects for git remotes.

7. **CJK-safe substring slicing**
   `&content[start..end]` panics if `start`/`end` falls inside a multi-byte char.
   Implement `floor_char_boundary` / `ceil_char_boundary` (still unstable in std).

8. **`cargo install --path` from workspace requires explicit member**
   `cargo install --path apps/<name>-cli --locked --force`. Plain
   `cargo install --path .` fails on workspace roots.

## Required org-level secrets

`agent-rt` org secret `GH_DIST_TOKEN` — fine-grained PAT with write access to
`agent-rt/homebrew-tap`. Inherits to every repo automatically.

## Reference repos (canonical templates)

- [`agent-rt/skillctl`](https://github.com/agent-rt/skillctl) — workspace + 11 crates + TUI feature gate
- [`agent-rt/memctl`](https://github.com/agent-rt/memctl) — workspace + 7 crates (no TUI)
- [`agent-rt/acpctl`](https://github.com/agent-rt/acpctl) — single crate (no workspace)
- [`agent-rt/imgctl`](https://github.com/agent-rt/imgctl) — workspace + edition 2024 + Chrome dependency

When in doubt, mimic the closest match.
