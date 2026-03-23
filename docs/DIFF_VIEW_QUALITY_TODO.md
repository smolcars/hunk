# Diff View Quality Roadmap (VS Code Level)

## Goal

Build a GPUI diff viewer that matches VS Code quality for:

- Readability and visual polish
- Text selection and copy behavior
- Keyboard and mouse interaction
- Diff accuracy (including intra-line changes)
- Performance on large patches

## Non-Goals (for now)

- Full editor integration (editing inline)
- Three-way merge tooling
- Blame/history timelines in the same view

## Definition of Done

- Side-by-side diff has stable, predictable alignment and line numbers.
- Text selection works across gutters and both panes and supports copy.
- Intra-line modified spans are highlighted with accurate tokens.
- Navigation parity for core flow:
- next/previous file
- next/previous hunk
- jump to line
- 60 FPS target on large diffs (10k+ changed lines) with smooth scrolling.

## Work Plan

### Phase 1: Data Model Refactor (foundation)

- [x] Introduce structured patch model (`DiffDocument`, `DiffHunk`, `DiffLine`) in `src/diff.rs`.
- [x] Keep current UI behavior by adapting structured model back to `SideBySideRow`.
- [x] Add stable row IDs and explicit row metadata for future selection/navigation state.
- [x] Split stream construction from rendering concerns (remove synthetic presentation rows from core model).
- [x] Add Phase 1 regression tests for parsing edge-cases:
- empty file changes
- no-newline markers
- mixed meta lines around hunks
- unbalanced remove/add blocks

### Phase 2: Viewer Layout + Typography

- [x] Replace wrapped code lines with editor-like horizontal flow by default.
- [x] Apply measured column widths (remove hardcoded assumptions).
- [x] Implement sticky hunk headers and clearer file boundaries.
- [x] Improve gutter contrast, markers, spacing, and visual rhythm.

### Phase 3: Interaction Model

- [x] Add first-class selection model (line-range via mouse click/shift-click + shift-arrows).
- [x] Add copy selected text action from diff pane.
- [x] Add keyboard navigation actions and focused key context.

### Phase 4: Semantic Diff Fidelity

- [x] Add intra-line diff spans for modified pairs.
- [x] Add syntax highlighting pipeline hooks.

### Phase 5: Hardening

- [ ] Perf instrumentation and benchmarking fixtures for large repos.
- [ ] Snapshot and parser regression tests for representative patch corpus.
- [ ] Final polish pass and parity QA checklist against VS Code.
- [ ] Ensure `cargo fmt`, `cargo check`, and `cargo clippy -- -D warnings` pass.

## Current Status

- Phase 1 foundation complete.
- Phase 2 initial pass complete (fixed-width pan columns + no-wrap line flow + sticky hunk header + gutter polish).
- Phase 3 initial pass complete (line-range selection, copy, keyboard navigation).
- Phase 4 complete (intra-line spans + multi-language Tree-sitter syntax pipeline).
