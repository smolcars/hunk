# Forge Integration TODO

Date: 2026-04-13
Owner: Hunk
Status: Proposed
Scope: Add real GitHub and GitLab forge integration to Hunk, starting with in-app pull request and merge request creation.

## Decision

- Use provider APIs as the production integration path.
- Add a new crate: `crates/hunk-forge`.
- Keep Git repository mechanics in `crates/hunk-git`.
- Do not depend on `gh` or `glab` for runtime product behavior.
- Start with a small PR-first slice:
  - replace browser-prefilled PR/MR creation from the Git tab,
  - replace browser-prefilled PR creation from the AI flow,
  - show the current open PR/MR inside Hunk with a link to it.

## Why This Shape

- Hunk already has the right split for this:
  - `hunk-git` knows how to resolve remotes, branches, and upstream state.
  - `hunk-desktop` owns the UI actions and progress states.
- The current PR/MR flow is still URL-only:
  - Git tab action builds a review URL and opens the browser.
  - AI flow builds a review URL and opens the browser.
- API-first scales better than CLI-first for:
  - typed responses,
  - predictable error handling,
  - background refresh,
  - pagination,
  - host support,
  - tests,
  - packaging.
- CLI availability is not reliable enough to make it the product boundary.

## Product Goals

1. Create a PR/MR from inside Hunk instead of sending the user to a prefilled browser page.
2. Let both current entry points use the same forge-backed path:
   - Git tab `Open PR/MR`
   - AI view `Open PR`
3. Show the current open PR/MR for the active branch in Hunk.
4. Keep the design provider-neutral so GitLab can follow the GitHub slice without a rewrite.
5. Leave room for later CI and issues features without reworking the crate boundary.

## Non-Goals For The First Slice

1. No embedded full web review page.
2. No full issue browser yet.
3. No CI dashboard yet.
4. No reviewer assignment, labels, milestones, or draft toggles in v1 unless they fall out naturally.
5. No runtime dependency on `gh` or `glab`.
6. No secrets in `~/.hunkdiff/config.toml`.

## Architecture Boundary

### `crates/hunk-git`

Responsibilities:

- resolve the active remote for a branch,
- normalize remote host and repo path,
- expose enough metadata for forge API requests,
- remain the source of truth for branch, upstream, and push state.

Non-responsibilities:

- HTTP API clients,
- auth token storage,
- PR/MR creation requests,
- issue and CI domain models.

### `crates/hunk-forge`

New crate responsibilities:

- host/provider detection from Git remote metadata,
- auth storage abstraction,
- GitHub and GitLab API clients,
- provider-neutral forge models,
- PR/MR lookup and creation,
- later issue and CI reads.

Suggested initial modules:

- `provider.rs`
- `remote.rs`
- `auth.rs`
- `github.rs`
- `gitlab.rs`
- `pull_requests.rs`
- `models.rs`

### `crates/hunk-desktop`

Responsibilities:

- dialogs/forms for PR/MR creation,
- calling `hunk-forge` in background tasks,
- showing progress, success, and error states,
- caching and rendering the current PR/MR summary in the Git tab and AI view.

## Auth Direction

### v1

- Use personal access tokens.
- Store tokens in the OS credential store, not in Hunk config.
- Keep only non-secret host/provider preferences in config if needed.

### Later

- Add optional OAuth or device-flow login where it is operationally reasonable.
- GitHub App or OAuth decisions can wait until after the PR-first slice proves the UX and crate shape.

## Provider Model

Start with a small provider-neutral surface:

- `ForgeProvider`
- `ForgeHost`
- `ForgeRepoRef`
- `ForgeBranchRef`
- `OpenReviewSummary`
- `CreateReviewInput`
- `CreateReviewResult`

Suggested minimum fields:

- provider
- host
- repo owner or namespace
- repo name or path
- source branch
- target branch
- title
- body
- review number or iid
- review url
- review state

## Phase 1: Real PR/MR Creation In Hunk

### User-facing outcome

- Clicking `Open PR/MR` in the Git tab opens an in-app create flow.
- Clicking `Open PR` in the AI view uses the same forge-backed create flow.
- If a PR/MR already exists for the active branch, Hunk shows the existing one instead of blindly opening a new one.
- Hunk displays the current open PR/MR for the active branch, including a link and basic metadata.

### Implementation TODO

- [ ] Add `crates/hunk-forge`.
- [ ] Define provider-neutral models for open review lookup and create-review requests.
- [ ] Add a small remote-to-forge resolver.
- [ ] Move remote host and repo parsing that is forge-relevant into a reusable API surface.
- [ ] Add secure token storage abstraction for GitHub and GitLab hosts.
- [ ] Add GitHub client first.
- [ ] Implement `find_open_review_for_branch`.
- [ ] Implement `create_review`.
- [ ] Support the common same-repo branch flow first.
- [ ] Fail clearly for unsupported fork or permission cases in v1.
- [ ] Replace Git tab browser-based PR/MR open path with forge-backed lookup/create.
- [ ] Replace AI flow browser-based PR open path with the same forge-backed lookup/create path.
- [ ] Add current open PR/MR summary state to desktop view state.
- [ ] Render current open PR/MR summary in the Git tab.
- [ ] Render current open PR/MR summary in the AI view for the active thread branch.
- [ ] Keep `Copy Review URL` as a simple copy action for the real PR/MR URL after creation or lookup.
- [ ] Rename progress and status text away from browser-specific wording.
- [ ] Add targeted tests for:
  - [ ] remote parsing and provider resolution,
  - [ ] existing PR lookup mapping,
  - [ ] create-review request mapping,
  - [ ] unsupported-state handling.

### Suggested UX for v1

- Git tab button:
  - if no open PR/MR exists, show a small modal with:
    - title
    - base branch
    - body
  - on submit, create the PR/MR via API and show the created result in place
- AI view button:
  - keep the current branch creation, commit, and push flow
  - replace the final browser-open step with the same create-review modal or a lightweight confirm flow
  - after success, show the created PR in the AI workspace UI

### Acceptance Criteria

- A user can create a GitHub PR from the Git tab without leaving Hunk.
- A user can create a GitHub PR from the AI view without leaving Hunk.
- If the active branch already has an open GitHub PR, Hunk shows it instead of creating a duplicate.
- The Git tab shows the current open PR for the active branch with its number, title, state, and link.
- The AI view shows the same current open PR for the thread branch.
- The old browser-prefill path is no longer the primary behavior for GitHub.

## Phase 2: GitLab Parity

- [ ] Add GitLab API client to `hunk-forge`.
- [ ] Implement open MR lookup for branch.
- [ ] Implement MR creation.
- [ ] Reuse the same desktop UI flow and models where possible.
- [ ] Support self-hosted GitLab hosts from remote discovery.
- [ ] Add host-scoped auth UX for GitLab.

### Acceptance Criteria

- GitLab users can create and view an MR from the same Git tab and AI entry points.
- GitHub and GitLab share the same desktop interaction model.

## Phase 3: CI Status

- [ ] Add provider-neutral CI summary models.
- [ ] Show CI status for the current open PR/MR.
- [ ] GitHub:
  - [ ] summarize check runs and commit status state
- [ ] GitLab:
  - [ ] summarize MR pipeline state
- [ ] Render a compact CI section in the Git tab and AI view.

### Acceptance Criteria

- Hunk shows pass, fail, or pending for the current open PR/MR.
- The user can open the CI details link from Hunk.

## Phase 4: Issues

- [ ] Add provider-neutral issue summary models.
- [ ] Add issue list queries for the active repository.
- [ ] Allow linking a PR/MR to an issue from the create-review flow.
- [ ] Show a compact issue list or search view in Hunk.

### Acceptance Criteria

- Hunk can list issues for the active repo.
- Hunk can link a newly created PR/MR to a selected issue.

## Open Questions

- [ ] Whether the first GitHub auth UX should be:
  - manual PAT entry only,
  - PAT entry plus import from existing `gh` auth,
  - or browser/device OAuth immediately
- [ ] Whether the first PR create flow should expose:
  - draft,
  - reviewers,
  - labels,
  - maintainer-can-modify
- [ ] Whether current PR/MR summary belongs in the main workflow snapshot or in a separate forge snapshot cache

## Recommended First Implementation Order

1. Add `crates/hunk-forge` with provider-neutral models.
2. Add GitHub host detection and auth storage.
3. Implement `find_open_review_for_branch`.
4. Implement `create_review`.
5. Replace Git tab `Open PR/MR`.
6. Replace AI view `Open PR`.
7. Render current open PR summary in both places.
8. Add GitLab parity after the GitHub path is stable.

## Current Codepaths To Replace First

- Git tab:
  - `crates/hunk-desktop/src/app/controller/git_ops.rs`
  - `crates/hunk-desktop/src/app/render/git_workspace_panel.rs`
- AI flow:
  - `crates/hunk-desktop/src/app/controller/ai_git_ops.rs`
  - `crates/hunk-desktop/src/app/render/ai.rs`
  - `crates/hunk-desktop/src/app/render/ai_workspace_sections.rs`
- Current URL builder:
  - `crates/hunk-git/src/branch.rs`

The existing URL builder should remain useful as a fallback or copy-link helper, but it should stop being the primary implementation for `Open PR/MR`.
