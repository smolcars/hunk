# Codex Upgrade Workflow

This is the current Hunk process for upgrading Codex.

Hunk no longer consumes Codex by pinning upstream `tag = "rust-v..."` entries directly in `crates/hunk-codex/Cargo.toml` and `crates/hunk-desktop/Cargo.toml`. Hunk now consumes a fork branch through root `workspace.dependencies`, so the upgrade flow is:

1. Move the fork branch to the target upstream release.
2. Reapply Hunk-specific Codex fixes on the fork.
3. Refresh Hunk's lockfile against that fork commit.
4. Refresh bundled runtime assets for the matching release version.
5. Update docs and fix any protocol/API drift in Hunk.

## Current Source Layout

- Upstream baseline: `openai/codex`
- Hunk fork: `niteshbalusu11/codex`
- Hunk branch: `hunk/embedded-apply-patch-fix`
- Hunk consumes the fork from root `Cargo.toml` `workspace.dependencies`
- Bundled runtime scripts download official `openai/codex` release assets by default

## Upgrade Steps

### 1. Pick the target upstream release

Choose the upstream Codex release tag you want, for example `rust-v0.121.0`.

Capture:
- upstream tag
- upstream commit SHA for that tag

### 2. Update the fork branch

In your Codex fork:

1. Fetch upstream `openai/codex`.
2. Check out `hunk/embedded-apply-patch-fix`.
3. Rebase or recreate that branch on top of the target upstream tag.
4. Reapply the Hunk-required fixes.
5. Push the updated branch to your fork.

At a minimum, confirm the branch still contains the Hunk-required Codex fixes that are not yet available upstream.

### 3. Refresh Hunk to the new fork commit

Hunk's `Cargo.toml` already points at the fork branch. After the fork branch moves, refresh the lockfile so Hunk picks up the new commit.

Run:

```bash
cargo update -p codex-app-server \
  -p codex-app-server-protocol \
  -p codex-arg0 \
  -p codex-core \
  -p codex-exec-server \
  -p codex-feedback \
  -p codex-protocol
```

Then verify `Cargo.lock` points at the expected new fork commit.

### 4. Refresh bundled runtime assets

The runtime binaries are still downloaded from official `openai/codex` release assets by default. They are version-matched using the locked `codex-app-server` crate version in `Cargo.lock`.

Refresh them with:

```bash
./scripts/download_codex_runtime_unix.sh macos
./scripts/download_codex_runtime_unix.sh linux
pwsh -File ./scripts/download_codex_runtime_windows.ps1
```

Notes:
- The scripts default to `openai/codex` release assets.
- Override the release source only if needed with `HUNK_CODEX_RUNTIME_REPO`.
- Override the tag only if needed with `HUNK_CODEX_TAG`.

### 5. Update docs

Update:
- `docs/AI_CODEX_SPEC.md`
  - upstream tag
  - upstream commit SHA
  - current fork commit SHA
- any other docs that mention stale Codex versions or old direct-upstream pinning assumptions

### 6. Fix Hunk integration drift

Expect small integration fixes after a Codex bump, especially in:
- `crates/hunk-codex`
- `crates/hunk-desktop`

Typical areas:
- protocol/type changes
- app-server API changes
- login or keyring behavior
- sandbox/runtime packaging behavior
- release/CI scripts

### 7. Validate

Run once at the end:

```bash
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Also verify the AI workspace still works end to end:
- thread loading
- turn start/streaming
- approvals
- user input requests
- apply-patch / file edits
- packaged runtime validation

## Important Difference From The Old Process

Old process:
- bump `tag = "rust-v..."` in crate manifests

Current process:
- move the fork branch to the new upstream release
- reapply Hunk fixes on the fork
- refresh `Cargo.lock`
- refresh runtime assets separately

That distinction matters because the fork commit, the upstream baseline, and the bundled runtime asset source are now separate pieces of state.
