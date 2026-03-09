# AI Thread + Worktree Redo Plan

## Goal
Replace the AI view thread start and handoff flow so users choose how to start (`Local` or `Worktree`) from one `New` entry point, remove the AI composer workspace target dropdown, auto-derive branch names from the first prompt, and add a one-click `Open PR` action that commits/pushes/opens the review URL.

## Critical Invariants
- New thread entry flow must come from one menu with two choices:
  - `Local thread`
  - `Worktree thread`
- AI composer workspace target dropdown must be removed from the AI input area, including tied state/subscriptions.
- Worktree thread creation must never be based on arbitrary HEAD:
  - Resolve the repository’s default/mainline branch (`main`, `master`, or remote default head).
  - Sync that base branch locally.
  - Create the managed worktree from that synced base branch.
- First prompt in a new draft should trigger branch naming and branch/worktree setup before starting the thread turn.
- Timeline header should expose current branch and `Open PR`.

## Phases

### Phase 1: New Thread UX + Remove Dropdown
- Add explicit AI draft start mode state (`Local` vs `Worktree`).
- Change AI sidebar `New` button to dropdown menu with `Local thread` and `Worktree thread`.
- Keep keyboard `cmd/ctrl + n` for local-start behavior.
- Remove AI composer workspace target `Select` control and related helper text.
- Remove AI workspace target picker state wiring tied only to that dropdown.
- Deep review:
  - Ensure no stale picker sync calls remain.
  - Ensure creating a thread still restores/focuses composer correctly.

### Phase 2: Prompt-Driven Branch + Worktree Provisioning
- Add branch-name suggestion helper (fast deterministic prompt-to-branch slug + sanitize).
- On first prompt of a new draft:
  - `Local`: create/switch to generated branch in current workspace repo.
  - `Worktree`: sync default/base branch, create managed worktree from that synced base, target the new workspace, then start thread.
- Run this as background git action with clear status feedback.
- Add/extend hunk-git APIs needed for:
  - Syncing an arbitrary branch from upstream (not only currently checked-out branch).
  - Creating managed worktrees from an explicit base branch.
- Deep review:
  - Ensure no shelling out from app code.
  - Validate error messages for dirty tree, missing remotes, missing base branch.

### Phase 3: Timeline Branch Badge + Open PR Button
- Add branch chip on timeline header right side.
- Add `Open PR` button next to branch chip.
- Implement one-click flow:
  - Auto-generate commit message from AI thread context (with safe fallback).
  - Commit working copy changes if present.
  - Publish/push branch.
  - Build/open PR/MR URL in browser (same provider mapping behavior as Git tab).
- Deep review:
  - Confirm button loading/disabled states match git action lifecycle.
  - Confirm behavior for no changes / already published / no remote.

### Phase 4: Tests + Verification + Cleanup
- Update or add tests for:
  - Worktree creation from explicit synced base branch.
  - Branch name suggestion helper behavior.
  - New `Open PR` flow helper logic (pure pieces).
- Final pass:
  - Build workspace.
  - Run workspace clippy once.
  - Run tests once.
  - Address any regressions.

## Execution Tracking
- [x] Phase 1 complete
- [x] Phase 2 complete
- [x] Phase 3 complete
- [x] Phase 4 complete
