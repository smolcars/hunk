# Multi-Project Workspace Implementation Plan

## Status

- Proposed
- Owner: Hunk
- Last Updated: 2026-03-24

## Summary

This document defines the implementation plan for adding first-class multi-project workspace support to Hunk.

Today Hunk is structurally a single-project app. The active UI state, persistence model, repo watch pipeline, file tree, quick open, Git workflow state, Review defaults, and most toolbar labeling all assume there is exactly one project open at a time. The only major exception is the AI subsystem, which already has partial multi-workspace behavior for linked worktrees inside a single repo.

The goal of this plan is to lift Hunk from a single-project model to a workspace model without destabilizing the app:

- allow the user to add multiple Git projects to one Hunk workspace
- keep one globally active project across Files, Git, and Review
- let AI show threads grouped by project instead of flattening everything into one repo-local list
- preserve existing worktree behavior inside each project
- keep the implementation simple enough to reason about and scalable enough to extend later

The intended user experience is:

- `File > Open Project...` adds a project to the current workspace and activates it
- the toolbar exposes a searchable project picker
- Files, Git, and Review operate on the active project only
- AI shows sectioned thread groups for all projects in the workspace
- selecting an AI thread from another project switches the active project automatically
- projects can be removed from the workspace without touching on-disk repos or Git worktrees
- the full workspace persists across restarts

## Locked Product Decisions

These decisions are treated as fixed for this implementation unless product direction changes.

1. A project is always a canonical Git repo root.
2. Any selected folder is normalized to the primary Git repo root for identity and persistence.
3. `Open Project...` is additive in v1. It does not replace the existing workspace.
4. Files, Git, and Review share one global active project.
5. The AI thread list shows all workspace projects in separate sections.
6. AI sections are capped per project in v1 instead of rendering unbounded full lists.
7. Existing worktree behavior remains project-local. Worktrees are not promoted to top-level projects.
8. Removing a project only removes it from Hunk's workspace state. It does not delete files, repos, branches, or worktrees.
9. Heavy live refresh pipelines remain single-project in v1. Only the active project gets full watch/refresh treatment.

## Design Goals

- Introduce a first-class workspace model without rewriting every subsystem around simultaneous multi-repo execution.
- Reuse the existing single-project controller and rendering flows by rebinding them to an active project context.
- Keep project identity canonical and stable by keying everything off the primary Git repo root.
- Preserve current worktree semantics and per-repo persistence behavior.
- Keep the hot paths fast. Multi-project support must not make Git tab, Review tab, file tree, or diff refreshes slower for the active project.
- Avoid broadening scope to arbitrary non-Git folders in v1.

## Non-Goals For V1

- No support for arbitrary non-Git folders as top-level projects.
- No per-view project selection.
- No multiple live repo watches or snapshot refresh loops for inactive projects.
- No cross-project combined Git diff, file tree, or Review view.
- No deletion of Git worktrees or repositories when removing a project.
- No separate "workspace file" import/export format beyond persisted app state.
- No project tabs or side-by-side multi-project editors.

## Current Codebase Shape

The current implementation already has several useful seams and several hard single-project assumptions.

### Useful Seams

- `crates/hunk-git/src/worktree.rs` already models multiple workspace targets inside one repo.
- `crates/hunk-desktop/src/app/controller/core_workspace_targets.rs` already persists per-repo active workspace target selection.
- `crates/hunk-desktop/src/app/controller/ai/catalog.rs` already loads AI thread catalogs for multiple workspace roots.
- `crates/hunk-desktop/src/app/controller/ai/helpers.rs` already reasons about whether a thread belongs to the current project via primary repo root relationships.
- `crates/hunk-desktop/src/app/workspace_target_picker.rs` already provides a reusable fuzzy-select pattern suitable for a project picker.

### Single-Project Constraints

- `DiffViewer` stores one `project_path` and one `repo_root`.
- `AppState` persists one `last_project_path`.
- open-project flow in `core_snapshot.rs` replaces the current project instead of adding to a workspace.
- repo watch, snapshot refresh, quick open, file tree, recent commits, Git workspace state, Review defaults, and toolbar labels all bind to the one active repo.
- AI thread rendering is still presented as one flat thread list, even though the underlying data model already tracks multiple workspace roots.

## Canonical Workspace Model

The workspace model should be minimal and explicit.

### Project Identity

- Every top-level project is keyed by canonical primary Git repo root.
- If the user picks:
  - a nested folder inside a repo
  - the repo root itself
  - a linked worktree path for that repo
  - the primary checkout path for that repo
  they all normalize to the same top-level project identity.

### Active Project

- Exactly one project is active at a time.
- The active project drives:
  - Files view
  - Git view
  - Review view
  - toolbar labels
  - repo tree
  - quick open
  - recent commits
  - repo watcher
  - the active project context for new AI drafts

### Project-Local Worktree Model

- Each project keeps its own existing workspace-target catalog.
- The Git view's workspace target picker remains scoped to the active project's worktrees.
- AI thread grouping treats all worktrees belonging to the same primary repo as one project section.

## Persistence Model

`crates/hunk-domain/src/state.rs` should be extended from a single-project memory model to a workspace memory model.

### Required State Additions

- ordered workspace project roots
- active workspace project root
- optional project order metadata if the chosen representation needs explicit ordering

### Existing Per-Repo Maps To Keep

These maps already fit the desired model and should stay keyed by canonical primary repo root:

- `last_workspace_target_by_repo`
- `review_compare_selection_by_repo`
- existing workflow/recent-commit caches after they are made project-aware

### Migration

- If legacy state has `last_project_path` and no workspace project list, create a one-project workspace from it.
- Keep reading `last_project_path` temporarily for migration only.
- Once migrated, all new saves should persist the workspace-list fields.

### Cache Scope

The current `git_workflow_cache` and `git_recent_commits_cache` are singleton-shaped. For multi-project support they should become per-project caches, keyed by canonical repo root, or be wrapped in a project-aware cache container.

That change is necessary so switching between projects can hydrate cached UI state instead of forcing cold empty transitions for inactive projects.

## UX Model

### Entry Points

Projects should be addable from more than one place:

- File menu:
  - `Open Project...` adds and activates a project
  - `Remove Project` removes the active project from the workspace
- empty state:
  - opening a project when no workspace exists still uses the same additive flow
- toolbar:
  - searchable project picker for switching projects
  - active-project remove action

### Global Project Picker

Add a new fuzzy-searchable project picker modeled after the current workspace target picker:

- title: project display name
- detail: canonical repo path
- optional status badges:
  - `Active`
  - `Git`
- actions:
  - select project
  - remove non-active project
  - remove active project

The picker should live in the toolbar because project switching is app-global in v1.

### Files View

- Always scoped to the active project.
- Quick open only indexes and searches the active project's visible files.
- Switching projects safely reloads:
  - repo tree
  - open file selection
  - file editor tabs
  - file terminal cwd

### Git View

- Still project-local.
- The existing workspace target picker remains, but only for the active project.
- All Git actions still operate on the selected workspace target inside the active project.

### Review View

- Still project-local.
- Default compare selections remain persisted per project.
- Switching projects restores that project's last compare pair and active workspace target binding.

### AI View

- Thread sidebar becomes sectioned by project.
- Each section contains threads whose cwd belongs to that project's primary repo root.
- Active project section is shown first.
- Each project section is capped to 5 visible threads by default.
- Each section gets:
  - project title
  - optional thread count
  - `Show more` / `Show less`
- Selecting a thread from another section:
  - activates that project
  - preserves the thread's actual workspace root and worktree context

## Architecture Strategy

The correct v1 architecture is not "make every current field multi-project at once." That would create unnecessary complexity and likely destabilize refresh order, focus behavior, and rendering.

The better approach is:

1. add a real workspace/project state model
2. keep one active project bound into the current `DiffViewer` single-project fields
3. store per-project persisted and cached state separately
4. rebind the active project into the existing pipelines when switching

This preserves the current mental model of the controllers while still enabling a first-class workspace.

## Recommended State Shape

### New Top-Level Types

Add a small desktop-local project descriptor and a persisted workspace state model.

Suggested conceptual types:

- `WorkspaceProjectRecord`
  - canonical repo root
  - display name
- `WorkspaceProjectsState`
  - ordered projects
  - active project root
- `ProjectUiCache`
  - cached workflow snapshot data
  - cached recent commits
  - cached workspace targets
  - cached active target id

These names are illustrative; exact names can be adjusted to fit the existing style.

### `DiffViewer` Responsibilities

`DiffViewer` should continue to own the active project's bound runtime fields:

- `project_path`
- `repo_root`
- `workspace_targets`
- `active_workspace_target_id`
- current snapshot/rendering state

But it should also gain workspace-level fields:

- ordered workspace project list
- active project root
- per-project caches
- project picker state
- AI per-project visible thread section expansion state

### Active Project Rebinding Rule

Whenever the active project changes:

- persist draft AI prompt for the old visible workspace
- swap `DiffViewer` active single-project fields to the new project's cached or hydrated state
- restore that project's active workspace target
- restart active-project watch/refresh loops
- request repo tree/file search reload for the new repo
- refresh AI visible state against the new active project

Only one project should own the active hot-path machinery at a time.

## AI Thread Grouping Semantics

This feature is easy to get wrong if thread grouping is based only on exact cwd.

The grouping rule should be:

- derive each thread's primary repo root from its cwd
- group threads by that primary repo root
- map that primary repo root to a top-level workspace project section

That ensures:

- primary-checkout threads and linked-worktree threads appear together under the same project section
- AI threads do not disappear just because the current active workspace target is different
- adding a project exposes all its known thread roots under one project section

### Section Capping

V1 cap:

- 5 visible threads per section

Behavior:

- if total threads `<= 5`, show all
- if total threads `> 5`, show 5 and render `Show more`
- expanded/collapsed state is per project section, not global
- active project section may be auto-expanded if needed, but default rendering should still be deterministic and not reorder threads inside sections

## Removal Semantics

Removing a project must be defined precisely.

### Remove Active Project

- remove the project from persisted workspace membership
- tear down active repo watch/refresh state for that project
- choose the next active project:
  - next project in order if one exists
  - otherwise previous project
  - otherwise empty state
- preserve per-project persisted preferences for that repo so re-adding it later restores state

### Remove Inactive Project

- remove it from persisted workspace membership
- keep the current active project unchanged
- prune inactive AI section state for that project
- do not disturb active watch/refresh pipelines

### Important Safety Rule

- removing a project must never call Git mutation APIs
- removing a project must never delete a worktree or repo
- removing a project is purely a Hunk workspace-membership change

## Performance Constraints

Performance is a hard requirement for this feature.

The following must remain true:

- active project snapshot refresh cost should stay close to current single-project behavior
- inactive projects must not accumulate background refresh churn
- switching projects should feel immediate and should prefer cache hydration before cold reload
- Git tab and Review tab should not pay for other inactive projects
- AI thread catalog refresh across multiple projects must stay bounded and avoid booting unnecessary live runtimes

Recommended v1 performance stance:

- exactly one repo watch
- exactly one active snapshot refresh pipeline
- exactly one active recent-commits pipeline
- exactly one active file-search index
- AI catalogs may scan all workspace roots, but only as lightweight catalog loads, not live runtime boots

## Risks

### Risk 1: Singletons Hidden As Project State

Several caches and controller paths are singleton-shaped today. Converting only some of them to per-project state while leaving others global can create stale-state bugs.

Mitigation:

- explicitly audit every field that depends on `project_path` or `repo_root`
- make rebinding paths deterministic
- keep a checklist of all single-project assumptions to clear before rollout

### Risk 2: Project Identity Drift

If project identity is sometimes the selected folder and sometimes the discovered primary repo root, per-repo persistence will fracture.

Mitigation:

- canonicalize immediately on add
- persist only canonical primary repo root
- key all per-project maps off the canonical root

### Risk 3: AI Thread Misgrouping

If AI grouping stays based on visible workspace targets only, threads from worktrees or previously added project roots may disappear or land in the wrong section.

Mitigation:

- group by thread primary repo root
- treat worktrees as belonging to the parent project section

### Risk 4: Watch/Refresh Ordering Bugs On Project Switch

Project switching touches repo watch, snapshot refresh, tree reload, quick open, AI workspace change, and toolbar labeling.

Mitigation:

- centralize active-project switch flow in one controller path
- avoid partial manual field mutation across multiple call sites

## Execution Plan

## Phase 1: Workspace State Foundation

- [ ] Add persisted workspace project membership and active-project state to `crates/hunk-domain/src/state.rs`.
- [ ] Add migration from legacy `last_project_path` into the new workspace state.
- [ ] Decide and implement the exact serialized representation for ordered project membership.
- [ ] Convert singleton workflow/recent-commit caches into project-aware caches.
- [ ] Add desktop-local project descriptor types for active/inactive project metadata.
- [ ] Add tests for:
  - workspace-state round-tripping
  - legacy migration
  - project-order persistence
  - active-project persistence
  - per-project map key stability
- [ ] Deep review of the state model before landing later phases.

## Phase 2: Project Identity and Add/Remove Flows

- [ ] Add a canonical project-resolution helper that normalizes any selected path to primary Git repo root.
- [ ] Update `open_project_picker` so `Open Project...` adds and activates a project instead of replacing all state.
- [ ] Add a single controller path for:
  - add project
  - activate project
  - remove project
- [ ] Add safe duplicate handling when the same repo is selected twice through different paths.
- [ ] Define active-project fallback selection after removal.
- [ ] Preserve all removal semantics as non-destructive workspace-membership updates only.
- [ ] Add tests for:
  - add repo root
  - add nested folder in repo
  - add linked worktree path
  - dedupe behavior
  - remove active project
  - remove inactive project
  - empty-state fallback after last removal

## Phase 3: Active Project Rebinding

- [ ] Introduce a centralized `switch_active_project(...)` flow inside desktop controllers.
- [ ] Rebind all active project fields on switch:
  - `project_path`
  - `repo_root`
  - `workspace_targets`
  - `active_workspace_target_id`
  - workflow snapshot state
  - recent commits state
  - repo tree state
  - file-search state
  - open file/editor selection
- [ ] Ensure active repo watch teardown/restart happens only through the switch flow.
- [ ] Hydrate from project-local cache before cold refresh when possible.
- [ ] Keep inactive projects out of hot-path watch/refresh loops.
- [ ] Add tests for:
  - cache hydration on project switch
  - repo watcher rebinding
  - tree/editor reset correctness
  - recent-commit restoration
  - active workspace target restoration

## Phase 4: Toolbar Project Picker and Menu Integration

- [ ] Add a project picker delegate/component modeled after `workspace_target_picker`.
- [ ] Add toolbar UI for the active project picker.
- [ ] Add project remove action in the picker UI.
- [ ] Update application menus:
  - keep `Open Project...`
  - add `Remove Project` for the active project
- [ ] Update empty/open-project states to use the same additive project-open flow.
- [ ] Ensure the toolbar reflects active-project display name and repo path consistently after switching.
- [ ] Add tests for picker search, selection, and remove actions.

## Phase 5: Files View Project Switching

- [ ] Rebind repo tree to the active project only.
- [ ] Rebind quick open to the active project only.
- [ ] Rebind file editor state safely on project switch.
- [ ] Decide whether editor tabs are cleared on project switch or persisted per project.
- [ ] Rebind files terminal cwd to the active project.
- [ ] Ensure file-creation, rename, and delete actions remain scoped to the active project root.
- [ ] Add tests for:
  - quick open searches only active project files
  - file actions operate in the correct project
  - editor selection and tabs after project switch

## Phase 6: Git View Project-Local Worktree Preservation

- [ ] Keep existing workspace-target picker project-local to the active project.
- [ ] Restore per-project active workspace-target persistence on project switch.
- [ ] Ensure all existing Git actions continue to operate on the selected workspace target inside the active project.
- [ ] Validate that project switching does not leak one project's target catalog into another's UI.
- [ ] Add tests for:
  - workspace target restoration per project
  - Git action scoping after project switch
  - branch/worktree selection isolation across projects

## Phase 7: Review View Project Isolation

- [ ] Keep Review compare sources project-local.
- [ ] Restore compare selections from per-project persisted state.
- [ ] Ensure project switch recomputes compare sources from the active project's workspace targets and branches only.
- [ ] Preserve existing worktree-aware Review behavior inside each project.
- [ ] Add tests for:
  - compare-selection persistence per project
  - compare-source isolation between projects
  - correct defaults after switching projects

## Phase 8: AI Project Sections

- [ ] Replace the flat AI thread sidebar rendering with project sections.
- [ ] Group threads by primary repo root, not exact cwd.
- [ ] Sort sections with active project first, then stable deterministic order.
- [ ] Cap visible threads per project section to 5.
- [ ] Add `Show more` / `Show less` state per section.
- [ ] Keep existing thread sort order inside each section.
- [ ] Selecting a thread from another project section must:
  - activate that project
  - preserve thread cwd/worktree binding
  - update visible runtime state correctly
- [ ] Ensure new AI drafts bind to the active project's current workspace target.
- [ ] Add tests for:
  - section grouping by project
  - worktree thread grouping under parent project
  - section capping
  - thread selection switching active project
  - draft binding after active-project switch

## Phase 9: AI Catalog Refresh Across Workspace Projects

- [ ] Extend AI catalog refresh input roots from active-project-only assumptions to all workspace projects and their workspace targets.
- [ ] Keep refresh lightweight and catalog-only for inactive projects.
- [ ] Prune AI project state when a project is removed from the workspace.
- [ ] Make sure hidden runtimes and background thread state still behave correctly if the associated project is removed or re-added.
- [ ] Add tests for:
  - catalog loading across multiple projects
  - pruning removed-project state
  - preserving active-project AI state on unrelated project removal

## Phase 10: Final Cleanup and Validation

- [ ] Audit all `project_path` / `repo_root` assumptions in desktop controllers and renderers.
- [ ] Remove dead single-project persistence code once migration is stable.
- [ ] Normalize naming so "project", "workspace", and "workspace target" are not conflated in code or UI.
- [ ] Update relevant docs to explain:
  - workspace membership
  - project switching
  - AI section behavior
  - non-destructive project removal
- [ ] Run final validation once at the end:
- [ ] `./scripts/run_with_macos_sdk_env.sh cargo build --workspace`
- [ ] `./scripts/run_with_macos_sdk_env.sh cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `./scripts/run_with_macos_sdk_env.sh cargo test --workspace`
- [ ] Use `./scripts/resolve_cargo_target_dir.sh` or existing `just` recipes so all commands write to `target-shared`.

## File and Module Impact

The main implementation is expected to touch these areas:

- `crates/hunk-domain/src/state.rs`
- `crates/hunk-domain/tests/app_state.rs`
- `crates/hunk-desktop/src/app.rs`
- `crates/hunk-desktop/src/app/controller/core_bootstrap.rs`
- `crates/hunk-desktop/src/app/controller/core_snapshot.rs`
- `crates/hunk-desktop/src/app/controller/core_workspace_targets.rs`
- `crates/hunk-desktop/src/app/controller/recent_commits.rs`
- `crates/hunk-desktop/src/app/controller/file_quick_open.rs`
- `crates/hunk-desktop/src/app/controller/review_compare.rs`
- `crates/hunk-desktop/src/app/controller/ai/catalog.rs`
- `crates/hunk-desktop/src/app/controller/ai/helpers.rs`
- `crates/hunk-desktop/src/app/controller/ai/core_workspace.rs`
- `crates/hunk-desktop/src/app/render/toolbar.rs`
- `crates/hunk-desktop/src/app/render/ai.rs`
- `crates/hunk-desktop/src/app/render/ai_workspace_sections.rs`
- new project-picker UI/delegate files in `crates/hunk-desktop/src/app/`

The exact write set should still be kept minimal per phase.

## Acceptance Criteria

The feature is complete when all of the following are true:

- The user can add multiple Git repos to one Hunk workspace.
- Restarting Hunk restores the project list and last active project.
- Files, Git, and Review all follow one global active project.
- Git worktree behavior still works exactly within the active project.
- Quick open, repo tree, and recent commits are always scoped to the active project.
- The AI thread sidebar shows project sections rather than one flat mixed list.
- AI threads from linked worktrees appear under the correct project section.
- Selecting an AI thread from another project activates that project automatically.
- Removing a project is non-destructive and updates the workspace immediately.
- Performance of the active project remains effectively on par with the current single-project app.

## Recommended Implementation Order

Land the work in this order:

1. Phase 1: workspace persistence and project-aware caches
2. Phase 2: add/remove flows and canonical identity
3. Phase 3: active project rebinding
4. Phase 4: toolbar/menu project switching UI
5. Phase 5 through 7: Files, Git, and Review isolation
6. Phase 8 and 9: AI project sections and catalog expansion
7. Phase 10: cleanup, documentation, and final validation

This order keeps the highest-risk refactor surface early, while delaying UI polish until the underlying state model is stable.
