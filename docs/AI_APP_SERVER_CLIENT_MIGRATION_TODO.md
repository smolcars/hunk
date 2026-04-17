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
- Hunk no longer shells out to `codex exec` for AI branch-name or commit-message generation; those one-shot structured-output requests now use the same embedded app-server seam as the rest of the AI workspace.
- Hunk-owned crates now consume Codex protocol types through `hunk-codex::protocol` instead of reaching into upstream crate paths directly. That gives the workspace one local protocol boundary for future Codex bumps.

Important dependency note:
- Hunk now pins the Codex fork explicitly through `workspace.dependencies` instead of using a blanket root patch override for the entire upstream workspace.
- That keeps the fork dependency visible in the two crates that actually consume Codex and makes eventual upstream removal simpler.
- `hunk-desktop` no longer depends on Codex protocol crates directly; those types are re-exported through `hunk-codex` so the UI crate talks to a Hunk-owned seam instead of upstream crates.
- Hunk's Linux forge auth store now uses the same async-persistent keyring backend feature set as embedded Codex. The `keyring` crate still exposes a synchronous `Entry` API, so Hunk's auth-store code does not need a separate async rewrite.
- Hunk-owned websocket/host dependencies were removed from `hunk-codex`.
- Some websocket/tungstenite crates still appear transitively through upstream embedded Codex crates.
- A larger remaining cost is that upstream `codex-app-server` still pulls login, keyring, plugin, analytics, and websocket surfaces even for Hunk's embedded-only use case.
- That remaining cost is no longer spread across the app. It is concentrated in `crates/hunk-codex` and specifically in the local in-process wrapper around upstream `codex-app-server`.
- Reducing those remaining transitive dependencies requires a deeper upstream carve-out or vendoring a smaller in-process runtime surface locally.
