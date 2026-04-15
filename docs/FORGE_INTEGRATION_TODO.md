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
- Use `octocrab` as the GitHub client implementation instead of handwritten raw HTTP.
- For auth:
  - use browser-based GitHub.com sign-in as the default GitHub auth path,
  - keep PAT auth for GitHub Enterprise and other self-hosted GitHub hosts,
  - keep PAT entry as a fallback and recovery path even on `github.com`.
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
- For Rust crate decisions, prefer inspecting version-pinned source and docs directly over ad hoc web searches:
  - clone or read the exact crate version being adopted,
  - record the pinned version in this doc,
  - use the crate's real source layout and builder/model types as the implementation reference.

## Dependency Reference Strategy

- GitHub client reference:
  - crate: `octocrab`
  - pinned version: `0.49.7`
  - source tag/commit inspected directly for implementation decisions
- GitHub auth flow reference:
  - crate: `oauth2`
  - pinned version: `5.0.0`
  - use a browser-based authorization code flow with PKCE semantics where supported by the chosen GitHub app registration shape
- Forge secret storage reference:
  - crate: `keyring`
  - pinned version: `3.6.3`
  - feature set: `apple-native`, `windows-native`, `sync-secret-service`
- Guidance:
  - prefer direct inspection of the pinned crate source in `/tmp` or the local Cargo registry,
  - use versioned docs as a supplement,
  - avoid designing around unpinned latest web examples.

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
- `github.rs` using `octocrab`
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

### Current shipped auth

- Use personal access tokens.
- Keep tokens out of Hunk config.
- Store only non-secret credential metadata and repo bindings in config.
- Store the actual token material in the OS credential store.
- Keep environment variables as a bootstrap or developer fallback, not the steady-state product path.

### Next target auth UX

- `github.com`:
  - default to a browser-based sign-in flow,
  - create or refresh a reusable GitHub session in the keychain,
  - stop making manual PAT entry the primary experience.
- GitHub Enterprise and custom GitHub hosts:
  - keep PAT auth in v1,
  - do not attempt browser sign-in on arbitrary enterprise hosts yet.
- GitLab:
  - remain PAT-based until the GitLab integration slice exists.

### Later

- Add richer account management UX.
- Add GitLab-specific sign-in only after the GitHub path is stable.
- Revisit enterprise-hosted browser sign-in only if there is a clear product need.

## Auth Model

Credential lookup should be keyed by:

- provider
- host
- repo path

Do not treat auth as a single global GitHub token or a single global GitLab token.

### Stored Metadata

Keep this non-secret metadata in config:

- credential id
- provider
- host
- account label
- whether the credential is the default for that provider and host
- exact repo-to-credential bindings

Keep this secret material out of config:

- PATs
- refresh tokens
- OAuth access tokens

### Credential Kinds

Support two concrete credential kinds behind the same provider-neutral model:

- `PersonalAccessToken`
- `GitHubComSession`

`GitHubComSession` should carry:

- access token
- refresh token when available
- expiry metadata
- authenticated account login
- authenticated account display label

These fields belong in the OS credential store, not in config.

### Resolution Rules

For any forge action, resolve credentials in this order:

1. Exact repo binding for `provider + host + repo path`
2. Host default credential for `provider + host`
3. Single configured credential for `provider + host`
4. If multiple credentials exist and none resolve cleanly, prompt the user

This allows Hunk to support:

- `github.com` and `gitlab.com` side by side
- self-hosted GitHub Enterprise and GitLab hosts
- multiple accounts on the same host
- multiple open projects with different forge credentials

### Implementation Phases

Phase A:

- add provider-neutral credential metadata and repo binding models
- resolve credentials by `provider + host + repo`
- stop treating forge auth as a host-only GitHub token cache
- keep secrets temporary and in-memory while the model settles

Phase B:

- replace in-memory secrets with OS credential store access
- key stored secrets by credential id
- keep repo bindings and defaults unchanged
- use the Rust `keyring` crate as the desktop implementation:
  - macOS: Keychain
  - Windows: Credential Manager
  - Linux: Secret Service via synchronous access
- keep keychain reads off the passive UI path where possible:
  - resolve credentials synchronously,
  - load saved secrets inside background work or explicit submit flows,
  - retain an in-process credential-id keyed cache to avoid repeated secret-store reads

Phase C:

- add account chooser UX when resolution is ambiguous
- add token import from `gh` or `glab` as an optional convenience
- add better account labeling and manual credential management

Phase D:

- add browser-based GitHub.com sign-in
- exchange the browser authorization result for a reusable GitHub session
- refresh GitHub.com sessions automatically when the provider allows it
- keep GitHub Enterprise and custom hosts on PATs

## GitHub.com Browser Sign-In

### Scope

- This flow is only for `github.com`.
- Do not apply it to GitHub Enterprise or arbitrary self-hosted GitHub hosts in the first implementation.
- The user-facing goal is:
  - click `Sign in with GitHub`,
  - complete auth in the browser,
  - return to Hunk already signed in,
  - create PRs without ever pasting a token.

### Flow Shape

1. The user clicks `Sign in with GitHub` from a forge auth entry point.
2. Hunk starts a local loopback callback listener on `127.0.0.1` with an ephemeral port.
3. Hunk generates:
   - state token
   - PKCE verifier and challenge
4. Hunk opens the GitHub authorization URL in the browser.
5. The user signs in and approves Hunk on `github.com`.
6. GitHub redirects back to the local callback URL with the authorization result.
7. Hunk validates the returned state and exchanges the authorization code for session tokens.
8. Hunk calls the authenticated user endpoint to identify the account.
9. Hunk stores the session in the OS credential store and stores only non-secret account metadata in config.
10. Future GitHub API calls reuse the stored session and refresh it before expiry where supported.

### Why This Shape

- It gives `github.com` users a much better first-run experience than manual PAT entry.
- It fits the current keychain-backed credential storage model.
- It avoids forcing an enterprise-host story before the product has a reason to support it.
- It keeps provider-neutral credential resolution intact while changing only how `github.com` credentials are acquired.

### Enterprise Fallback

- For GitHub Enterprise and any non-`github.com` GitHub host:
  - continue using PAT entry,
  - continue storing the PAT in the OS credential store,
  - continue using the same `provider + host + repo path` credential resolution logic.
- The UI should make this explicit:
  - `github.com`: `Sign in with GitHub` primary action, PAT entry secondary
  - enterprise hosts: PAT entry only

### Token Lifecycle

- Access token:
  - loaded from the OS credential store,
  - reused for normal API calls,
  - refreshed before or after expiry where supported by the provider flow.
- Refresh token:
  - stored only in the OS credential store,
  - never written to config,
  - revoked or deleted on sign-out.
- Account identity:
  - login and display label stored in config metadata for chooser UI,
  - used to label multiple `github.com` accounts.

### Rust Implementation Strategy

- Keep `octocrab` for GitHub REST calls after auth succeeds.
- Use the `oauth2` crate for the browser sign-in flow.
- Reuse `keyring` for all secret persistence.
- Prefer a minimal loopback callback server built with `std::net::TcpListener` unless Hunk later needs a richer embedded HTTP surface.
- Keep token exchange and refresh work in background tasks, not on the main GPUI thread.

### Engineering Plan

1. Extend forge auth models to represent credential kind and optional session metadata without putting secrets in config.
2. Add a GitHub.com auth service in `hunk-forge` that can:
   - build the browser authorization URL,
   - validate callback state,
   - exchange the code for tokens,
   - refresh stored sessions.
3. Add a desktop auth coordinator in `hunk-desktop` that:
   - starts the local callback listener,
   - opens the browser,
   - waits for the callback in a background task,
   - persists the resulting session.
4. Add `Sign in with GitHub` UI for `github.com`.
5. Keep PAT entry UI for:
   - GitHub Enterprise,
   - custom hosts,
   - manual recovery on `github.com`.
6. Teach the GitHub client bootstrap path to:
   - load a stored session,
   - refresh if needed,
   - fall back to prompting only when no usable auth exists.
7. Add sign-out and re-authenticate actions.
8. Add targeted tests for:
   - state generation and callback validation,
   - token exchange response mapping,
   - refresh behavior,
   - host-based auth mode selection,
   - persistence of non-secret metadata versus secret material.

### Acceptance Criteria

- A `github.com` user can sign in without pasting a PAT.
- After sign-in, PR creation works without showing the token field as the primary path.
- Restarting Hunk preserves the signed-in session through the keychain.
- Expired sessions refresh automatically when possible.
- GitHub Enterprise users still have a clear PAT path that behaves exactly as today.

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

- [x] Add `crates/hunk-forge`.
- [x] Define provider-neutral models for open review lookup and create-review requests.
- [x] Add a small remote-to-forge resolver.
- [x] Move remote host and repo parsing that is forge-relevant into a reusable API surface.
- [x] Add provider-neutral forge credential metadata and repo binding models.
- [x] Add credential resolution rules for `provider + host + repo path`.
- [x] Add secure token storage abstraction for GitHub and GitLab hosts.
- [x] Move actual secret storage behind a credential-id keyed interface.
- [x] Back desktop secret storage with `keyring` `3.6.3`.
- [x] Add GitHub client first via `octocrab`.
- [x] Implement `find_open_review_for_branch`.
- [x] Implement `create_review`.
- [x] Support the common same-repo branch flow first.
- [ ] Fail clearly for unsupported fork or permission cases in v1.
- [x] Replace Git tab browser-based PR/MR open path with forge-backed lookup/create.
- [x] Replace AI flow browser-based PR open path with the same forge-backed lookup/create path.
- [x] Add current open PR/MR summary state to desktop view state.
- [x] Render current open PR/MR summary in the Git tab.
- [x] Render current open PR/MR summary in the AI view for the active thread branch.
- [x] Keep `Copy Review URL` as a simple copy action for the real PR/MR URL after creation or lookup.
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

- [ ] Whether the `github.com` browser sign-in should be backed by a GitHub App user-token flow or an OAuth app registration, while preserving the same Hunk-side UX
- [ ] Whether the first `github.com` sign-in flow should expose a secondary `Use PAT instead` path directly in the auth dialog or only behind an advanced/manual option
- [ ] Whether the initial `github.com` browser sign-in slice should support multiple `github.com` accounts on day one or land as single-account-first
- [ ] Whether repo bindings should remain exact-path matches in v1 or also support namespace-level defaults later
- [ ] Whether the first account chooser should live in the PR dialog or in a separate forge settings surface
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
5. Add `github.com` browser sign-in and session refresh.
6. Replace Git tab `Open PR/MR`.
7. Replace AI view `Open PR`.
8. Render current open PR summary in both places.
9. Add GitLab parity after the GitHub path is stable.

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
