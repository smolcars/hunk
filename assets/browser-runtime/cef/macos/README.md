# Hunk CEF Runtime: macOS

This folder is reserved for the bundled CEF runtime used by Hunk's embedded AI browser.

Initial spike target:

- OS/architecture: `aarch64-apple-darwin`
- Candidate Rust binding: `tauri-apps/cef-rs`
- Candidate binding version: `146.7.0+146.0.12`
- Candidate CEF version: `146.0.12+g6214c8e+chromium-146.0.7680.179`
- Download source used by cef-rs: `https://cef-builds.spotifycdn.com`

Expected staged files:

- `Chromium Embedded Framework.framework`
- CEF resources and locales
- CEF snapshot/blob assets required by the selected CEF build
- `hunk-browser-helper` subprocess binary or helper app
- `archive.json` or an equivalent pinned manifest with source URL, version, size, and checksum

Checksum process:

1. Download/export the pinned CEF runtime with the selected cef-rs tooling.
2. Record the source URL, archive file name, byte size, and SHA-256 in a manifest.
3. Validate the staged runtime before packaging Hunk.
4. Fail packaging if the staged files do not match the manifest.
