# AI Codex WebSocket TODO

Archived.

The old Hunk-managed Codex websocket transport has been removed. Hunk now boots the Codex app server in-process for AI workspace operations.

Any future work in this area should focus on:
- reducing remaining transitive websocket/tungstenite dependencies from upstream embedded crates
- removing the bundled `codex` executable only after upstream helper-runtime requirements are replaced
