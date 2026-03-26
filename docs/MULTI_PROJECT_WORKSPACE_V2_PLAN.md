# Multi-Project Workspace V2 Plan

## Status

- Proposed
- Owner: Hunk
- Last Updated: 2026-03-24

## Summary

This document defines the second-phase multi-project workspace refactor for Hunk.

V1 added basic multi-project membership, but it still keeps too much of the app on a single active-project execution path. The current implementation still hard-rebinds project state during switches, AI still inherits active-project assumptions in several places, and both terminal and refresh behavior are not yet shaped for seamless multi-project usage.

The goal of V2 is:

- make AI a true workspace-wide surface with no passive active-project concept
- keep Files, Git, and Review on one shared non-AI active project
- make project switches in non-AI views effectively instant
- keep all workspace projects warm in memory and watched concurrently
- make AI threads, AI runtimes, and AI terminals work across multiple projects and worktrees without teardown
- fix the project picker sizing and AI header behavior so the UI matches the new model

## Reference Findings

These references were inspected locally before writing this plan:

- `t3code` at `2a435ae` in `/private/tmp/t3code-ref`
- `zed` at `aabc967` in `/tmp/zed`

### T3 Code patterns worth copying

- T3 keeps a normalized read model with separate `projects[]` and `threads[]`, where each thread links back to a `projectId` and also carries branch/worktree metadata.
- The sidebar does not treat one project as globally active. It derives project sections from normalized state and groups threads per project with pure helpers.
- Project-local UI state such as expanded/collapsed state and manual order is persisted separately from the thread read model.
- Project headers own project-local "new thread" actions, instead of routing everything through one global active project.

Relevant files:

- `/private/tmp/t3code-ref/apps/web/src/store.ts`
- `/private/tmp/t3code-ref/apps/web/src/components/Sidebar.logic.ts`
- `/private/tmp/t3code-ref/apps/web/src/hooks/useHandleNewThread.ts`
- `/private/tmp/t3code-ref/packages/contracts/src/orchestration.ts`

### Zed patterns worth copying

- Zed uses a top-level `MultiWorkspace` shell that owns multiple live `Workspace` instances at once.
- The visible workspace is just an active index change. Other workspaces stay resident instead of being torn down and rebuilt.
- Workspace membership, active workspace selection, and workspace-sidebar state are top-level concerns, while each workspace owns its own stores and runtime state.
- The sidebar subscribes to `WorkspaceAdded`, `WorkspaceRemoved`, and `ActiveWorkspaceChanged` instead of rebuilding the full world from one singleton project state.

Relevant files:

- `/tmp/zed/crates/workspace/src/multi_workspace.rs`
- `/tmp/zed/crates/workspace/src/persistence/model.rs`
- `/tmp/zed/crates/sidebar/src/sidebar.rs`

## Locked Product Decisions

1. AI has no passive active-project concept.
2. Files, Git, and Review share one global non-AI active project.
3. Selecting an AI thread does not change the non-AI active project.
4. Actions launched from AI into Files, Git, or Review activate the thread's project and worktree on demand.
5. All workspace projects stay warm. Active-only live refresh is not acceptable in V2.
6. AI thread creation is project-local from the sidebar section header, not from one global `New` button.
7. AI terminals are scoped per workspace target root, not per thread and not per project.
8. Files terminals are scoped per project and rooted at that project's selected workspace target.
9. The AI toolbar must not show the project picker.
10. Breaking state/model changes are acceptable if they simplify the implementation.

## Architecture Direction

The core change is to stop treating `DiffViewer` as one mutable project context and instead treat it as:

- one workspace shell
- many fully-owned project contexts
- one non-AI active project pointer
- one AI-visible workspace model derived from all project contexts

### 1. Replace singleton project fields with project-scoped state

Introduce a `WorkspaceProjectState` keyed by canonical primary repo root.

Each project state owns:

- canonical project root
- repo root / selected workspace target root
- worktree catalog and selected workspace target id
- workflow snapshot state
- recent commits state
- repo watch tasks and refresh bookkeeping
- file tree state and quick-open/search indexes
- Files editor selection and any open per-project editor state that must survive switching
- Review compare state and restore state
- Git panel state
- Files terminal state/runtime

`DiffViewer` keeps:

- ordered workspace project roots
- `active_non_ai_project_root`
- `projects_by_root: BTreeMap<PathBuf, WorkspaceProjectState>`
- AI-global state that spans all projects

The old direct fields like `project_path`, `repo_root`, `workspace_targets`, `files`, `branches`, `review_compare_sources`, `files_terminal_session`, and related refresh/watch task state should move behind the project container or become active-project views into that container.

### 2. Split non-AI project switching from project hydration

Replace the current "activate project" path with two explicit concepts:

- `ensure_project_warm(project_root, cx)` for background hydration and watcher startup
- `select_non_ai_project(project_root, cx)` for visible project switching

`select_non_ai_project(...)` must not:

- clear AI state
- clear the whole file tree/editor/review state
- clear recent commits state globally
- restart watchers for unrelated projects
- force a full snapshot refresh when the target project is already warm

Instead it should:

- switch the active non-AI project pointer
- rebind the visible view state to the target project
- restore focus, selection, tree/editor/review state, and terminal state for that project
- trigger a refresh only if the target project is cold, invalidated, or explicitly stale

### 3. Keep all projects warm

V2 should maintain one live refresh/watch pipeline per workspace project.

That means:

- one repo watcher per project
- project-local workflow caches updated from watch events
- project-local recent commit refresh
- project-local worktree target refresh
- project-local repo tree freshness

This does not mean everything must be fully materialized all the time.

Allowed lazy behavior:

- file contents and editor buffers load on first open
- Files terminal shells only spawn after first open
- AI runtimes only spawn when a project/worktree is actually used in AI

Not allowed:

- inactive projects becoming stale because they are not watched
- non-AI project switching showing a cold empty state for a previously opened project

### 4. Make AI workspace-wide and derived

AI should behave more like T3's normalized sidebar and less like the current active-project-bound controller flow.

Keep a normalized AI model:

- workspace membership from the Hunk workspace
- thread summaries grouped by project root
- thread workspace key / worktree root per thread
- AI runtime registry keyed by workspace key
- AI terminal registry keyed by workspace key

Remove current-project pruning behavior such as `clear_ai_state_outside_current_project(...)`.

The AI sidebar should be a pure derivation over:

- workspace projects
- thread summaries across all projects/worktrees
- per-project expanded/collapsed state
- per-project preview limit state

Section order should follow workspace membership order so the UI remains stable.

Inside a project section:

- threads are sorted by existing recency rules
- collapsed sections show a capped preview list
- if the selected thread would otherwise be hidden, keep it visible
- section header includes project-local actions for `New` and `New Worktree`

### 5. Re-scope terminal ownership

Current Hunk behavior is split:

- AI terminal state is effectively thread-owned
- Files terminal state is singleton-owned

V2 should change that to:

- AI terminal state/runtime keyed by workspace key
- Files terminal state/runtime keyed by canonical project root

Consequences:

- switching between two AI threads in the same worktree reuses the same terminal
- switching between two AI threads in different worktrees restores the correct terminal for that worktree
- switching Files between projects restores that project's terminal instantly
- switching the selected workspace target inside a project reuses or restarts the terminal only if the cwd actually changes

### 6. Keep top-level shell state minimal

Follow Zed's `MultiWorkspace` persistence pattern:

- top-level workspace shell only persists membership, ordering, non-AI active project, and global shell UI state
- each project persists its own view state, target selection, and caches
- AI sidebar expansion/order state is independent from project data itself

This prevents one giant monolithic persisted app state from becoming the source of every runtime decision.

## Required UI Changes

### AI view

- Remove the project picker from the top-left toolbar area.
- Remove any `Active` project badge from AI sections.
- Replace the global `New` menu with per-project actions in each section header.
- Keep AI header chips informational only: workspace/worktree label, branch, status, approvals, inputs.
- Do not show a full AI loading pass when entering the tab if cached catalogs are already available.

### Files / Git / Review

- Keep the project picker in the toolbar.
- Make the picker wide enough for long names and paths.
- The picker trigger should have a meaningful minimum width.
- The picker popup should have a larger width than the trigger and should truncate detail text cleanly instead of clipping.
- Switching the picker should feel like changing tabs, not reopening the app.

## Phased TODOs

### Phase 1: State model split

- [ ] Introduce `WorkspaceProjectState` and move singleton non-AI project state under it.
- [ ] Add active-project accessor helpers so existing render/controller code can migrate incrementally.
- [ ] Rename persisted active project to `active_non_ai_project_path`.
- [ ] Keep workspace membership/order separate from per-project state.

### Phase 2: Warm project runtime layer

- [ ] Create one live watcher/refresh controller per project.
- [ ] Maintain workflow, recent-commit, and worktree-target state for all projects concurrently.
- [ ] Preserve file tree, selected file, editor state, review state, and git panel state per project.
- [ ] Change project selection to rebind visible state instead of clearing global state.

### Phase 3: AI decoupling

- [ ] Remove active-project coupling from AI thread selection and AI workspace pruning.
- [ ] Keep AI catalogs loaded for all projects/worktrees at once.
- [ ] Maintain AI runtime registry keyed by workspace key and allow concurrent live runtimes.
- [ ] Rebuild AI section rendering as pure grouping over normalized project/thread data.

### Phase 4: Per-project AI actions

- [ ] Add per-section `New` and `New Worktree` actions.
- [ ] Route new AI thread creation directly to the section's project/worktree context.
- [ ] Make AI-driven navigation activate non-AI project/worktree only when that navigation is invoked.
- [ ] Keep section-local expansion and preview state stable across refreshes.

### Phase 5: Terminal refactor

- [ ] Replace AI thread-keyed terminal maps with workspace-keyed terminal maps.
- [ ] Replace Files singleton terminal state with project-keyed terminal state.
- [ ] Restore matching terminal sessions on thread/project/worktree switches without respawning when possible.
- [ ] Stop pruning terminals or runtimes just because another project becomes visible.

### Phase 6: UI polish and cleanup

- [ ] Remove AI toolbar project picker and active-project affordances.
- [ ] Widen the non-AI project picker trigger and popup.
- [ ] Make non-AI switches preserve focus, scroll, and open terminal/editor state.
- [ ] Clean up stale terminology so "workspace-wide" and "non-AI active project" are used consistently.

## Acceptance Criteria

1. AI shows all workspace projects at once and never requires a passive active-project switch.
2. Selecting an AI thread from another project does not change the non-AI active project.
3. Opening Review, Files, or Git from an AI thread activates the correct target project/worktree.
4. Files, Git, and Review project switching is effectively instant for previously opened projects.
5. Inactive projects continue receiving watcher-driven updates.
6. AI terminals are shared across threads on the same worktree and isolated across different worktrees.
7. Files terminals restore per project.
8. The AI toolbar has no project picker.
9. The non-AI picker no longer clips long labels and paths.

## Test Plan

- Add project-runtime tests proving inactive projects stay warm while another project is selected.
- Add switching tests proving `select_non_ai_project(...)` does not clear unrelated app state.
- Add AI tests proving cross-project thread selection does not mutate `active_non_ai_project_path`.
- Add AI navigation tests proving Review/Git/Files jumps activate the correct project/worktree.
- Add AI terminal tests for same-worktree reuse and cross-worktree isolation.
- Add Files terminal tests for per-project session restore.
- Add UI tests for per-project AI section actions, pinned-selected-thread behavior in collapsed sections, and widened project picker rendering.
- End with one workspace `cargo build`, one workspace `cargo clippy --all-targets -- -D warnings`, and one workspace `cargo test`.

## Implementation Notes

- Keep the Git behavior in `crates/hunk-git`.
- Keep new controller code split into small modules rather than expanding existing files past the repo's size expectations.
- Prefer normalized derived state for AI sidebar rendering instead of embedding grouping logic into render-time side effects.
- Prefer project-local state ownership over more global caches whenever the ownership boundary is obvious.
