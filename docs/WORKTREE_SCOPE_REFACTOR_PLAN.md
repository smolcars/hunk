# Worktree Scope Refactor Plan

## Goal

Keep Hunk anchored to the primary checkout for the imported repository while still allowing:

- AI threads to run in isolated linked worktrees
- the AI sidebar to show all repo threads
- the Git tab to inspect and act on arbitrary worktrees
- the Review tab to compare arbitrary branches and worktrees
- the Files tab to remain rooted in the primary checkout

This refactor is repo-agnostic. It must work the same way for any imported Git repository.

## Current Problem

The desktop app currently overloads one concept across multiple tabs:

- `project_path` is the repo identity / primary checkout
- `repo_root` is treated as the current operational root
- `active_workspace_target_id` is treated as the active worktree

Today, selecting or creating a worktree mutates `repo_root`, which causes:

- Files/editor/tree to move into that worktree
- Git panel state to move into that worktree
- toolbar/footer labels to move into that worktree
- comments to change scope
- AI workspace state to change because it follows the same root mutation

## Target Model

The app needs three separate scopes.

### 1. App Anchor

The app anchor is the primary checkout for the imported repo.

- Used by Files
- Used by editor/file tree operations
- Used by repo watch
- Used by repo identity / persistence keys
- Used by toolbar/footer default project context

### 2. Git/Review Inspection Target

The inspection target is a selected workspace target inside the repo.

- Used by the Git tab branch/worktree selector
- Used by Git actions inside the Git tab
- Used by Git working tree, branch, and recent-commit panels
- May inform Review defaults, but must not re-root the app

### 3. AI Execution Target

The AI execution target is the cwd/worktree for a thread or new-thread draft.

- Used to start/resume/select threads
- Used to route worker commands
- Used for workspace-scoped AI preferences
- Used for worktree creation
- Must not change the app anchor

## Required Invariants

- One imported repo maps to one stable app anchor.
- The primary checkout stays loaded in Files unless the user opens a different repo.
- Selecting a worktree in Git or Review does not change Files or editor roots.
- Creating a new AI worktree thread does not call a global app-root switch.
- A thread belongs to exactly one cwd, and commands for that thread go to that cwd runtime.
- The AI thread list is repo-scoped by default and shows threads from all known workspaces.
- Review compare sources remain explicit and selector-driven.
- Branch occupancy still respects Git rules: if a branch is already checked out in a worktree, branch selection should route inspection to that worktree rather than attempt an invalid checkout.

## High-Risk Areas

- `repo_root` is currently overloaded across Files, Git, comments, caching, and AI.
- `active_workspace_target_id` is currently both persisted selection and global app context.
- `ai_workspace_cwd()` and `ai_workspace_key()` currently drive both execution routing and visible UI scope.
- `selected_path` is shared across Files and Review, which is unsafe once Review can inspect worktrees without re-rooting Files.
- Comments are currently keyed by global `repo_root` and `branch_name`.

## Phase Plan

### Phase 1: Separate Scope State

Introduce explicit state for each scope.

Files:

- `crates/hunk-desktop/src/app.rs`
- `crates/hunk-domain/src/state.rs`
- `crates/hunk-desktop/src/app/controller/core.rs`

Changes:

- Keep `project_path` as the stable repo identity.
- Make the primary checkout the persistent app anchor.
- Stop restoring the whole app into a linked worktree on startup/cache hydration.
- Add a Git-tab selection state separate from the app anchor.
- Add helper accessors:
  - primary/app root
  - primary workspace target id
  - selected Git inspection target id/root
  - AI draft execution target id/root
  - selected thread execution workspace key/root

Expected outcome:

- The app no longer treats “selected worktree” as “global root”.

### Phase 2: Split Git Tab Data From Files Data

Files:

- `crates/hunk-desktop/src/app.rs`
- `crates/hunk-desktop/src/app/controller/core.rs`
- `crates/hunk-desktop/src/app/controller/git_ops.rs`
- `crates/hunk-desktop/src/app/controller/recent_commits.rs`
- `crates/hunk-desktop/src/app/render/git_workspace.rs`
- `crates/hunk-desktop/src/app/render/git_workspace_panel.rs`
- `crates/hunk-desktop/src/app/render/git_recent_commits.rs`
- `crates/hunk-desktop/src/app/render/commit.rs`
- `crates/hunk-desktop/src/app/render/workspace_change_row.rs`

Changes:

- Introduce Git-tab-specific workflow state:
  - selected target root/id
  - branch metadata
  - branch list
  - working-tree file list
  - line stats used by Git rows
  - staged-file selection
  - last commit subject
  - recent commits
  - loading/error state
- Keep existing Files workflow state pinned to the primary checkout.
- Change the workspace target picker in the Git tab to change Git inspection state only.
- Change branch activation/occupancy routing in Git to retarget Git inspection, not the whole app.
- Run Git actions against the selected Git inspection root.

Expected outcome:

- Git can inspect and act on arbitrary worktrees without moving Files.

### Phase 3: Keep Files Strictly Primary-Checkout-Scoped

Files:

- `crates/hunk-desktop/src/app/controller/file_tree.rs`
- `crates/hunk-desktop/src/app/controller/editor.rs`
- `crates/hunk-desktop/src/app/render/root.rs`
- `crates/hunk-desktop/src/app/render/toolbar.rs`

Changes:

- Files tree/editor always resolve against the app anchor root.
- Guard cross-tab path reuse:
  - do not blindly carry Review-selected paths into Files if they do not exist in the primary checkout
- Keep toolbar/footer primary-checkout-centric outside Git inspection details.

Expected outcome:

- Files never silently moves into a linked worktree.

### Phase 4: Make Review Fully Source-Scoped

Files:

- `crates/hunk-desktop/src/app/controller/review_compare.rs`
- `crates/hunk-desktop/src/app/controller/comments.rs`
- `crates/hunk-desktop/src/app/review_compare_picker.rs`

Changes:

- Remove dependence on global active workspace target for Review defaults.
- Keep Review compare source selection explicit.
- Scope comment identity to the Review compare context instead of global `repo_root` / `branch_name`.

Expected outcome:

- Review remains seamless for arbitrary worktrees and branches while the app anchor stays unchanged.

### Phase 5: Make AI Repo-Scoped In The UI And Target-Scoped In Execution

Files:

- `crates/hunk-desktop/src/app/controller/ai/core.rs`
- `crates/hunk-desktop/src/app/controller/ai/runtime.rs`
- `crates/hunk-desktop/src/app/controller/ai/workspace_runtime.rs`
- `crates/hunk-desktop/src/app/render/ai.rs`
- `crates/hunk-desktop/src/app/render/ai_helpers/core.rs`

Changes:

- Aggregate visible AI thread list across all known workspace states.
- Keep the visible thread list repo-scoped.
- Keep visible timeline/runtime state tied to the selected thread or draft execution target.
- Stop calling global workspace activation from worktree-thread creation.
- Route thread actions explicitly by target workspace key.
- Update empty states and labels from “this workspace” to repo-wide wording.

Expected outcome:

- The AI sidebar shows all repo threads.
- Selecting a thread switches AI execution context, not the whole app.

### Phase 6: Refresh, Persistence, And Cleanup

Files:

- `crates/hunk-domain/src/state.rs`
- `crates/hunk-domain/tests/app_state.rs`
- `crates/hunk-desktop/src/app/controller/core_runtime.rs`
- `crates/hunk-desktop/src/app/controller/ai/tests.rs`
- new crate-level tests as needed

Changes:

- Persist the Git-tab selected worktree independently from the app anchor.
- Decide which caches are primary-checkout-only and which are inspection-target-specific.
- Refresh workspace catalog from the primary repo root.
- Add tests for:
  - startup staying on primary checkout
  - Git selector not re-rooting Files
  - Review compare staying source-scoped
  - AI worktree thread creation not switching the app root
  - repo-wide AI thread listing and per-thread execution routing

Expected outcome:

- The new behavior survives app restarts and is covered by tests.

## Implementation Checklist

### Foundation

- Add explicit helpers for primary root, primary target, Git-selected target, and AI-selected target.
- Audit every `repo_root` read and classify it as:
  - primary anchor
  - Git inspection
  - Review source
  - AI execution

### Git

- Create a Git-tab-specific state container.
- Port Git render/controller code to that state container.
- Remove global `activate_workspace_target` from the Git picker flow.

### Files

- Ensure editor/tree/load/save/delete/rename always use primary root.
- Stop reusing invalid Review-selected paths in Files.

### Review

- Decouple Review defaults from app-global active target.
- Re-scope comments.

### AI

- Aggregate threads across workspace states.
- Route thread actions explicitly by thread cwd.
- Start/promote worktree runtimes without changing app root.

### Verification

- Manual code review after each phase.
- Run `cargo fmt`.
- Run `cargo build --workspace`.
- Run `cargo clippy --workspace --all-targets -- -D warnings`.
- Run `cargo test --workspace`.
