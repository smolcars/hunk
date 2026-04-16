# AI Embedded SQLite Plan

Status: completed.

Outcome:
- Hunk's SQLite stack was aligned so the embedded Codex app-server path could be linked into the desktop app.
- The old remote bundled runtime path was subsequently removed.

Current state:
- AI workspace uses embedded Codex only.
- The previous SQLite blocker is no longer relevant to the runtime architecture.

Remaining follow-up, if desired:
- remove the bundled `codex` executable by replacing `codex_self_exe` helper-runtime needs and Hunk's `codex exec` shell-out usage
- reduce remaining transitive websocket/tungstenite dependencies from upstream embedded crates
