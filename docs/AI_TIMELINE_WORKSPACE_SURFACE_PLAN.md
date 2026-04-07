# AI Timeline Workspace Surface Plan

## Status

- Implemented: session-backed AI timeline surface is live and the legacy-parity pass landed
- Owner: Hunk
- Last Updated: 2026-04-06
- Supersedes: `docs/AI_CHAT_TIMELINE_V2_TODO.md` for the AI timeline render architecture

## Summary

This document defines the next AI timeline architecture for Hunk.

The current AI tab renders the thread body as a virtualized GPUI list with heterogeneous row widgets. That was a reasonable first step, but it is now the wrong rendering model for the product we want:

- Windows performance is not acceptable on some machines.
- streaming updates still create row measurement and layout churn
- the future product direction needs inline review inside the AI tab itself
- AI is now heavy enough that it should behave like its own workspace surface, not a stack of cards

The new direction is:

- replace the thread-body `ListState` path with a painted AI-specific workspace surface
- reuse the same buffer-backed projection and viewport principles that power Files and Review
- keep AI-specific rendering and interaction semantics instead of forcing the thread into the existing review/file editor abstraction unchanged
- add a right-side review pane inside the AI tab so timeline-selected diffs render in place instead of bouncing to the Review tab

We do not need feature-flag gating. Breaking changes are acceptable. The legacy timeline path should be removed as part of this migration, not preserved indefinitely.

## Current Assessment

The backend migration succeeded and the parity pass is now in the codebase.

What shipped:

- the AI tab no longer depends on the old list-based thread body
- the timeline scrolls through a dedicated session-backed surface with cached geometry
- inline review stays inside the AI tab
- the new surface uses neutral legacy-aligned palette choices instead of the earlier synthetic chrome
- message projection restores markdown block structure, inline emphasis, links, and fenced-code syntax colors
- plan rows restore explanation emphasis and completed-step strike-through behavior
- expansion moved back toward disclosure-row semantics instead of the temporary `Show more` pill

Remaining work is no longer architectural. The follow-up is validation on representative Windows hardware and targeted visual cleanup if any parity drift is still found during manual QA.

## Problem Statement

Today the AI thread body is primarily implemented in:

- `crates/hunk-desktop/src/app/render/ai_timeline_list_view.rs`
- `crates/hunk-desktop/src/app/render/ai_helpers/timeline_rows.rs`
- `crates/hunk-desktop/src/app/render/ai_helpers/timeline_markdown.rs`
- `crates/hunk-desktop/src/app/render/ai_helpers/selectable_text.rs`

That path has several structural costs:

- the timeline is a `ListState` of mixed GPUI row trees instead of one painted viewport
- markdown, code blocks, selectable text, shell details, tool details, and diff summaries all create their own layout and interaction surfaces
- streaming content invalidates row measurements and can force expensive visible-range recalculation
- root AI rendering still does too much work outside the chat body, especially after the multi-project AI expansion

The review/files architecture is materially stronger for this class of workload:

- it projects many logical documents into one shared surface
- it caches display rows and viewport geometry
- it paints only the visible region
- hit-testing is surface-local instead of widget-tree-local

However, the existing review/files implementation is still a text-row projection model, not a generic conversation renderer. `WorkspaceDisplayRow` in `crates/hunk-editor/src/workspace_display.rs` only carries plain text plus raw/display column metadata. That is not sufficient for rich AI timeline blocks by itself.

## Reference Findings

### Hunk findings

- `crates/hunk-desktop/src/app/review_workspace_session.rs` is the right reference for viewport snapshots, geometry caching, and projected display rows.
- `crates/hunk-desktop/src/app/workspace_surface.rs` is the right reference for a paint-first surface with hit-testing and row-local actions.
- `crates/hunk-editor/src/workspace.rs` and `crates/hunk-editor/src/workspace_display.rs` provide good buffer/excerpt projection primitives, but their display row model is intentionally text-centric.
- `docs/Lessons Learned.md` already points in this direction: AI should become its own workspace surface with cached visible-frame state, and Windows perf investigation showed that the remaining AI regressions are not solved by list virtualization alone.

### Zed findings

Zed is useful as a reference for multi-surface editor architecture, but not as proof that its current AI conversation panel is multibuffer-backed in the same way as the editor.

- Zed's current agent thread view is still list-style UI for conversation entries.
- Zed mounts true editor surfaces for embedded diffs and terminal-style content.
- The important takeaway is the split between conversation chrome and embedded editor surfaces, not a literal "everything is one multibuffer thread editor" implementation.

That means Hunk should not copy a nonexistent Zed pattern. We should instead build the architecture that fits Hunk's current needs and future inline review direction.

## Goals

- Make AI timeline scrolling and streaming feel like a paint-driven surface, not a stack of cards.
- Cut AI thread-body render work enough to hit the Hunk frame budget of 8ms on representative hardware.
- Make the left side of the AI tab stable and cheap even for long threads with markdown, code blocks, plans, and tool output.
- Support in-tab review: timeline on the left, review pane on the right.
- Reuse existing workspace/buffer primitives where they genuinely help.
- Remove the old list-based timeline path once the new surface reaches parity.
- Preserve old timeline visuals and behavior at feature parity.

## Non-Goals

- Do not make the AI timeline a generic text editor with full editor semantics.
- Do not force every AI UI element into `WorkspaceDisplayRow` as it exists today.
- Do not try to generalize all review/files/AI surface code into one giant abstraction before the AI path is proven.
- Do not preserve a legacy fallback timeline path behind a flag.
- Do not redesign the composer in this project unless the new split layout requires small focus or placement fixes.

## Locked Product Decisions

1. The AI thread body will move to a dedicated painted surface.
2. The AI tab will own its own surface/session model instead of depending on `ListState` row widgets for the thread body.
3. The AI timeline will reuse Hunk's buffer-backed projection concepts, but not the exact review/files row model unchanged.
4. The AI tab will gain a right-side review pane for timeline-selected diffs.
5. Full-file diff review inside the AI tab should reuse the existing review workspace session/surface rather than inventing a second diff renderer.
6. The current click-to-Review-tab flow is temporary and should be replaced by in-tab review as part of this migration.
7. Breaking changes are allowed if they simplify the final architecture.
8. There will be no feature flag and no long-lived dual render path.
9. We should prefer copy-then-converge over premature deep abstraction between AI and Review.
10. Files must stay under 1000 lines as the new surface/session code is introduced.

## Parity Work Landed

- Restored legacy lane widths and neutralized the experimental palette drift.
- Removed the synthetic surface chrome that made the AI timeline look redesigned.
- Restored message markdown projection with headings, paragraphs, lists, quotes, thematic breaks, inline links, inline code, bold, italic, and strike-through styling.
- Restored fenced-code syntax coloring in the painted surface path.
- Restored raw markdown link hit-testing and text-selection surface mapping.
- Restored plan explanation emphasis and completed-step strike-through handling in the projected layout.
- Kept inline review in the AI tab while preserving the left-side timeline surface path.
- Kept the new session/surface path cached and paint-first rather than reintroducing the legacy list renderer.

## Reopened Parity Follow-up

The screenshot review uncovered a second parity pass that was not optional. The new surface had kept the paint-first backend but was still projecting the old timeline into generic blocks, which changed real user-visible behavior.

Confirmed regressions from the screenshot audit:

- lane placement drifted left instead of staying inside the old centered content lane
- grouped rows lost their old disclosure-header presentation and collapsed into generic stacked previews
- command batches no longer summarized as a single group row with expandable nested command rows
- command execution rows lost the old collapsed-header plus expandable transcript treatment
- file-change rows lost the compact diff summary presentation and their old review-tab navigation
- file-change groups stopped collapsing into one summary row and instead behaved like generic groups

Follow-up parity work completed in the surface path:

- restored centered lane math for assistant/tool rows and right-aligned user rows inside the old lane widths
- restored disclosure-style group headers with inline summaries instead of stacked title/preview cards
- restored nested child rows for expanded command, exploration, and collaboration groups
- restored command execution projection to collapsed header plus expandable transcript body
- restored file-change and diff rows to compact diff summaries that open the Review tab again
- restored per-group and per-child invalidation signatures so expanded group content rebuilds when nested rows change

The latest screenshot review reopened one more parity slice. These are not design tweaks; they are missing behaviors from the old timeline that still need to be restored on the painted surface.

Confirmed remaining parity gaps:

- diff/file summary rows still render as plain text instead of restoring semantic color treatment for filenames and added/removed counters
- plan rows still restore strike-through semantics but not the old checklist state colors for completed, in-progress, and pending items
- assistant rows lost the old copy-message affordance
- user rows lost the old copy-message affordance
- expanded command transcript rows lost the old copy-command-transcript affordance
- markdown code fences lost the old copy-code-block affordance
- assistant and user message copy affordances are still too ephemeral; they need to stay visibly present instead of effectively appearing only on hover
- disclosure-style rows such as commands, diff summaries, and file edits lost the old row-wide hover highlight treatment
- expanded command action buttons are positioned too high and can overlap the disclosure chevron hit area, which makes copy unreliable
- command execution rows still miss the old `Run in terminal` action in the expanded transcript header
- in-progress command execution rows still miss the old running/streaming status indicator treatment
- pending steer rows still miss the old `Waiting to steer running turn...` indicator
- queued user messages still miss the old `queued, waiting for current turn to finish.` status treatment

Implementation plan for the reopened slice:

1. Extend the AI surface projection with semantic style metadata instead of hard-coded plain-text styling.
   This includes filename accent spans, added/removed line-stat spans, and plan checklist state color spans.
2. Restore block-level copy actions in the painted surface session model.
   Message blocks need copy-message metadata and command transcript blocks need copy-transcript metadata.
3. Restore code-block copy actions in the markdown projection.
   Markdown code fences need explicit copy regions so the painted surface can place action buttons over the correct code block instead of only offering whole-message copy.
4. Render the missing copy affordances on top of the painted surface without reintroducing the old list renderer.
   The surface remains paint-first for text/content, while a small overlay action layer handles the interactive copy buttons for visible blocks/code regions.
5. Re-run full workspace verification, then commit and push only after the parity slice is green.

Implementation plan for the current follow-up:

1. Restore stable action affordances in the overlay layer.
   Message copy needs to remain visibly present, command transcript actions need a dedicated preview-header action lane, and the overlay positions need to stop colliding with disclosure toggles.
2. Restore row hover feedback in the painted surface.
   The surface needs block-level hover tracking so disclosure and diff rows can reuse the old dark/light hover shading without bringing back per-row GPUI widgets.
3. Restore command-row action parity.
   Expanded command execution rows need the old transcript header treatment again: status indicator, `Run in terminal`, and transcript copy.
4. Restore transient user-state parity.
   Pending steer and queued user messages need the old waiting/queued status indicators projected through the surface model so users can tell why a message has not executed yet.
5. Re-run full workspace verification, then commit and push once the parity slice is green.

## Validation and Follow-up

- Run manual parity QA against the old AI timeline on representative threads, especially long markdown-heavy and command-heavy turns.
- Validate smooth scrolling and streaming on the Windows hardware that originally exposed the regression.
- Treat any remaining visual drift as normal follow-up bug fixes, not architecture work.

## Performance SLOs

- AI thread-body scrolling should stay under the 8ms frame budget on representative threads once caches are warm.
- Streaming a delta into the latest assistant/tool row must only invalidate the affected row/block and dependent geometry, not the full visible list.
- Switching selected rows or toggling tool expansion must not rebuild unrelated markdown/layout state.
- Opening the right-side review pane must not force a rebuild of the left-side timeline projection.
- The AI tab must not depend on per-row GPUI entities for the main conversation body after the cutover.

## Architecture Direction

### 1. Build an AI-specific surface, not an AI-flavored `ListState`

Introduce a new AI thread body surface in `hunk-desktop`, parallel to the review surface:

- `ai_workspace_surface.rs`
- `ai_workspace_session.rs`
- `ai_workspace_session_geometry.rs`
- `ai_workspace_surface_paint.rs`
- `ai_workspace_surface_hit_test.rs`

The new surface should own:

- viewport math
- visible block selection
- geometry cache
- surface-local hit testing
- surface-local selection and copy behavior
- row/block actions such as expand, collapse, open review, open link, retry command, and scroll to latest

It should not own:

- Codex thread state
- thread/reducer business logic
- review diff computation
- workspace-wide AI shell state

That state should remain controller-owned and be projected into an AI surface session snapshot.

### 2. Reuse buffers and workspace projection ideas, but with AI-specific row metadata

The correct reuse level is:

- reuse `TextBuffer`
- reuse synthetic workspace documents/excerpts where that lowers complexity
- reuse the review/files approach of "many logical buffers projected into one viewport"
- reuse the caching discipline of visible rows plus geometry snapshots

The wrong reuse level is:

- pretending that a plan row, tool card, diff summary, markdown code fence, and shell transcript are all just plain `WorkspaceDisplayRow`s

Introduce an AI-specific projection model, likely centered on these concepts:

- `AiWorkspaceDocument`
- `AiWorkspaceBlockId`
- `AiWorkspaceBlockKind`
- `AiWorkspaceBlockLayout`
- `AiWorkspaceViewportSnapshot`
- `AiWorkspaceVisibleBlock`
- `AiWorkspaceHitRegion`

Each visible AI block should be able to reference:

- one or more text buffers
- block-local styled spans
- block-local interactive regions
- block-local attachments such as diff handles, link targets, command actions, or expansion affordances

This gives us one shared painted surface without flattening all semantics into a single fake editor row type.

### 3. Use synthetic AI documents and excerpts

The thread should be projected into synthetic documents instead of rendered as a widget tree.

Initial document kinds:

- user message
- assistant message
- plan summary
- tool status row
- command transcript
- diff summary
- system/informational row

Suggested modeling rule:

- one logical AI block owns one synthetic document buffer for its primary text payload
- optional secondary text payloads, such as command details or expanded tool output, can attach additional buffers or structured block metadata

This is intentionally closer to Hunk's multibuffer projection than to one giant monolithic chat buffer. It keeps streaming updates localized and makes per-block invalidation simpler.

### 4. Separate text projection from block chrome

The AI surface needs both:

- text projection for content that scrolls and selects like text
- chrome metadata for non-text affordances such as avatars, bubbles, tool icons, expansion toggles, badges, and inline action pills

The architecture should therefore split block rendering into:

- a text payload layer
- a chrome layer
- an interaction layer

Do not mutate `hunk-editor` to make it understand AI chrome in phase 1 unless a very small reusable addition is obviously warranted. Keep AI-specific render metadata in `hunk-desktop` first.

### 5. Keep markdown as a projection step, not a widget subtree

Assistant content should still be authored as markdown, but the surface should consume a projected representation:

- block tree
- styled spans
- fenced code block segments
- link regions
- ordered/unordered list markers
- quote/code/heading presentation metadata

This means `timeline_markdown.rs` should evolve from "build row widgets" into "build cached markdown projection for a surface block".

Important rule:

- parsing and syntax-highlighting markdown must be cached by content hash and width-dependent layout bucket

The paint path should consume cached projections only.

### 6. Put review on the right side of the AI tab

The target shell should be:

- left: AI timeline workspace surface
- right: review pane for the currently selected diff-bearing row

The right pane should reuse the existing review workspace machinery where possible:

- `review_workspace_session.rs`
- `workspace_surface.rs`
- review paint helpers and viewport snapshots

The left side should not attempt to inline a full side-by-side diff renderer into the main timeline flow in the first pass. The left side should show a compact diff summary and selection affordance. The right side should host the true review experience.

This gives us:

- lower left-side complexity
- much better product flow than jumping tabs
- a cleaner path to future "timeline + review" workflows

### 7. AI shell state should project into a visible-frame snapshot

The existing lessons learned still apply:

- do not keep deriving expensive AI state inside root render
- do not let scroll-driven renders repeatedly recompute thread-wide toolbar state

Introduce a controller-owned visible snapshot for the AI tab that feeds both surfaces:

- selected thread metadata
- visible timeline block ids
- selected block id
- follow-output state
- right-pane review selection
- toolbar/composer feedback snapshot
- width-sensitive cache keys where required

This should be cheap to diff and cheap to hand to render.

### 8. Focus and interaction must be designed up front

The AI tab will become a split multi-surface view. That means focus handling is now architecture, not polish.

The plan must explicitly handle:

- timeline text selection versus review-pane selection
- keyboard navigation between timeline and review pane
- restoring focus after closing overlays
- preserving composer focus when non-destructive timeline actions occur
- preserving selection when the right pane opens or changes

Use `docs/Lessons Learned.md` as the reference for focus restoration pitfalls.

## Proposed Module Shape

### New modules

- `crates/hunk-desktop/src/app/ai_workspace_surface.rs`
- `crates/hunk-desktop/src/app/ai_workspace_session.rs`
- `crates/hunk-desktop/src/app/ai_workspace_session_geometry.rs`
- `crates/hunk-desktop/src/app/ai_workspace_surface_paint.rs`
- `crates/hunk-desktop/src/app/ai_workspace_surface_hit_test.rs`
- `crates/hunk-desktop/src/app/ai_workspace_surface_selection.rs`
- `crates/hunk-desktop/src/app/ai_workspace_split_view.rs`

### Existing modules to refactor

- `crates/hunk-desktop/src/app/render/ai_timeline_list_view.rs`
- `crates/hunk-desktop/src/app/render/ai_helpers/timeline_rows.rs`
- `crates/hunk-desktop/src/app/render/ai_helpers/timeline_markdown.rs`
- `crates/hunk-desktop/src/app/render/ai_helpers/selectable_text.rs`
- `crates/hunk-desktop/src/app/controller/ai/helpers.rs`
- `crates/hunk-desktop/src/app/controller/ai/runtime.rs`

### Existing modules to reuse

- `crates/hunk-desktop/src/app/review_workspace_session.rs`
- `crates/hunk-desktop/src/app/workspace_surface.rs`
- `crates/hunk-desktop/src/app/workspace_display_buffers.rs`
- `crates/hunk-editor/src/workspace.rs`
- `crates/hunk-editor/src/workspace_display.rs`
- `crates/hunk-text/*`

## Migration Strategy

We should not try to replace everything in one giant commit. The correct migration shape is:

1. lock the target architecture and instrumentation
2. create the AI surface/session alongside the current timeline path
3. cut the AI tab over to the new surface as soon as the core text path is viable
4. port richer block kinds and right-side review
5. delete the legacy row-widget implementation

This is still a breaking-change migration, not a feature-flag rollout. "Alongside" here means temporary implementation coexistence in the branch until cutover, not a permanent runtime option.

## Completed Rollout

### Architecture and surface model

- [x] Replaced the AI thread-body `ListState` renderer with a dedicated painted AI workspace surface.
- [x] Introduced `AiWorkspaceSession` with stable block ids, width-bucketed geometry caching, viewport snapshots, and session rebuild invalidation keyed by visible-row sequence plus expansion state.
- [x] Kept AI-specific block/session modeling in `hunk-desktop` instead of forcing the Review/File `WorkspaceDisplayRow` abstraction onto conversation content.
- [x] Extended AI perf instrumentation for session rebuild time, geometry rebuild time, paint time, and hit-test count.

### Timeline projection and rendering

- [x] Projected the selected thread into ordered AI workspace blocks for user messages, assistant messages, plans, tool/status rows, pending steers, queued prompts, and diff summaries.
- [x] Switched the left timeline surface from preview-only cards to a readable wrapped-thread surface:
  - messages and plans render full wrapped text by default
  - tool and status rows render compact summaries with expand/collapse support
  - diff summaries stay compact and act as the selection anchor for review
- [x] Kept block chrome and actions surface-local instead of per-row GPUI entities.
- [x] Removed the old widget/list renderer and the dead timeline helper modules from production.

### Interaction parity

- [x] Implemented surface-local block selection, keyboard block navigation, and reveal-into-view behavior.
- [x] Implemented text selection, select-all, and copy on the painted AI surface.
- [x] Implemented raw link hit-testing and open behavior for URLs and workspace file targets rendered on the AI surface.
- [x] Implemented expand/collapse for tool and status detail blocks.
- [x] Restored follow-output and explicit scroll-to-bottom affordances on the painted timeline shell.
- [x] Preserved thread-switch behavior by clearing stale surface selection, syncing review state per thread, and rebuilding the session off stable row ids instead of visible indexes.

### Inline review inside AI

- [x] Added the split AI shell with left timeline and right inline review pane.
- [x] Reused the existing `ReviewWorkspaceSession` and review surface on the right side.
- [x] Replaced the old “bounce to Review tab” diff flow with in-tab review as the primary interaction.
- [x] Kept review selection thread-local so selecting a different diff block, clearing selection, and returning to a thread restores the last inline-review anchor naturally.

### Quality gates

- [x] `./scripts/run_with_macos_sdk_env.sh cargo check --workspace`
- [x] `./scripts/run_with_macos_sdk_env.sh cargo test --workspace`
- [x] `./scripts/run_with_macos_sdk_env.sh cargo clippy --workspace --all-targets -- -D warnings`

## External Validation

These are no longer implementation tasks, but they should be verified on representative hardware after merge:

- Windows perf confirmation on the thread corpus that originally reproduced frame drops
- warm-cache versus cold-cache scroll behavior on long AI threads
- long-thread memory growth with expanded tool output
- focus restoration around timeline, review pane, composer, and overlays

## Detailed Engineering Notes

### Cache keys

The AI surface will need multiple cache layers. At minimum:

- thread-to-block projection cache
- markdown parse/projection cache
- width-bucketed layout cache
- geometry cache
- hit-region cache
- optional syntax-highlight cache for fenced code blocks

Minimum invalidation inputs:

- selected thread id
- block id
- content hash
- expansion state
- viewport width bucket
- theme mode if styling changes line metrics or code-block layout

### Stable identity rules

Every visible block needs a stable id that survives:

- thread refresh
- streaming deltas
- plan status updates
- tool detail expansion/collapse
- diff summary recomputation when the underlying turn changes

The AI surface should not key geometry or selection by visible index.

### Review-pane integration rules

- A diff summary block is the selection anchor, not the entire turn.
- A thread can have zero or one active review selection at a time in the first implementation.
- Opening the review pane must not disturb left-side follow-output unless the user explicitly navigates to a diff.
- Thread-local review selection state should be restorable when moving between threads if that falls out naturally from the model.

### Focus rules

- Clicking timeline text gives focus to the AI timeline surface.
- Clicking review gives focus to the review pane.
- Sending a new message restores composer focus.
- Closing review or overlays restores focus to the surface that owned it previously.

### Markdown scope discipline

The first production version of the AI surface does not need to reproduce every subtle markdown edge case from the widget path before cutover. It does need to cover:

- paragraphs
- headings
- emphasis and inline code
- lists
- block quotes
- fenced code blocks
- links

Tables, unusual nesting, and exotic markdown behaviors can be explicitly downgraded or deferred if they are not materially used in current AI output.

## Risks and Mitigations

### Risk: trying to over-generalize with Review too early

If we try to make one grand unified workspace surface abstraction before AI is proven, we will spend time on the wrong layer.

Mitigation:

- keep AI session/surface adjacent to Review first
- extract only stable common pieces after Phase 5 or later

### Risk: markdown layout still dominates even after moving to paint

Painting alone will not help if markdown parsing, code highlighting, or width-sensitive layout is recomputed too often.

Mitigation:

- cache markdown projections aggressively
- prewarm expensive syntax paths
- instrument cache hit rates, not just frame time

### Risk: focus bugs in a split multi-surface shell

The AI tab will now contain at least timeline, review pane, composer, and overlays.

Mitigation:

- implement focus restoration as part of the architecture
- test focus transitions explicitly before phase close

### Risk: a fake "shared buffer" becomes harder to maintain than the current list

If we contort the AI timeline into a text editor model that does not actually match the product, we will gain complexity without enough perf benefit.

Mitigation:

- reuse buffers and projection principles, not editor semantics wholesale
- keep block chrome as AI-specific metadata

### Risk: memory usage grows with long-thread caches

AI threads can contain large payloads, command output, and markdown/code content.

Mitigation:

- bound caches
- evict width-bucket variants aggressively
- avoid duplicating large strings across projection layers

## Definition of Done

This project is done when all of the following are true:

- the AI thread body is rendered by a painted AI workspace surface
- the legacy `ListState` timeline body path is removed
- selecting a diff in the AI timeline opens review inside the AI tab
- scroll and streaming performance are materially better on Windows and do not regress on macOS
- workspace build, tests, and clippy pass

## Immediate Next Step

The first implementation task after approving this plan should be Phase 0 plus the minimal Phase 1 scaffolding:

- add AI surface/session types
- add perf instrumentation around the new path
- render a basic painted thread from projected block text

That will validate the architecture before the markdown and inline-review work begins.
