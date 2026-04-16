# AI App-Server Client Migration

Status: complete for Hunk-owned runtime architecture.

Current state:
- Hunk AI workspace is embedded-only.
- The legacy remote bundled app-server transport, host manager, websocket client, and runtime fallback path were removed.
- The desktop worker, catalog loader, and archive helpers all boot the embedded app server directly.

What was removed:
- `crates/hunk-codex/src/app_server_client_remote.rs`
- `crates/hunk-codex/src/host.rs`
- `crates/hunk-codex/src/ws_client.rs`
- remote/runtime transport selection in desktop AI runtime
- legacy host/websocket integration tests

What still remains:
- Hunk still bundles a `codex` executable in `assets/codex-runtime/...`.
- Embedded startup still passes `codex_self_exe` to upstream internals.
- Hunk still shells out to `codex exec` in `crates/hunk-desktop/src/app/ai_thread_flow.rs`.

Important dependency note:
- Hunk-owned websocket/host dependencies were removed from `hunk-codex`.
- Some websocket/tungstenite crates still appear transitively through upstream embedded Codex crates.
- Removing those remaining transitive dependencies requires a deeper upstream carve-out or vendoring the in-process runtime pieces we actually use.
