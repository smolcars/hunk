# Bundled Codex Runtime Layout

Hunk resolves a bundled Codex executable using these platform-specific paths:

- `assets/codex-runtime/macos/codex`
- `assets/codex-runtime/linux/codex`
- `assets/codex-runtime/windows/codex.exe`

At bundle time, package these files into application resources so runtime discovery can
prefer bundled binaries before PATH fallback.

Expected source is the pinned Codex release tag documented in `docs/AI_CODEX_SPEC.md`.

Local macOS workflow:

1. `./scripts/install_codex_runtime_macos.sh`
2. `./scripts/validate_codex_runtime_bundle.sh --strict --platform macos`
3. `./scripts/stage_codex_runtime_macos.sh`
4. `cargo test -p hunk-codex --test real_runtime_smoke -- --ignored`

Linux and Windows runtime assets come from:

- `./scripts/download_codex_runtime_unix.sh linux`
- `pwsh ./scripts/download_codex_runtime_windows.ps1`
