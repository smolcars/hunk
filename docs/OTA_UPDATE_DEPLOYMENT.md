# Hunk OTA Update Deployment

This document explains what the OTA update "server" is for Hunk, what needs to be deployed, and how to test it safely.

## What the update server actually does

Hunk does **not** need a custom backend for OTA updates right now.

The updater design is:

- Hunk downloads a small signed manifest from:
  - `https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev/stable.json`
- That manifest points at release binaries already hosted on GitHub Releases.
- Hunk verifies the signed payloads before applying them.

That means the "server" is currently just a **static file host** for the manifest, and optionally the detached `.sig` files if you want to publish them there too.

## What is hosted where

### Hosted on Cloudflare R2 public bucket

At minimum:

- `stable.json`
- `stable.json.sig`

Optional:

- detached signature files such as:
  - `Hunk-0.0.1-macos-arm64.app.tar.gz.sig`
  - `Hunk-0.0.1-linux-x86_64.tar.gz.sig`
  - `Hunk-0.0.1-x64.msi.sig`

### Hosted on GitHub Releases

These remain the real downloadable binaries:

- macOS DMG for first install
- macOS OTA tarball
- Windows MSI
- Linux tarball
- Linux `.deb`
- Linux `.rpm`

The manifest can safely point to GitHub release asset URLs. Your custom domain does not need to serve the actual app binaries unless you decide to move them later.

## Required signing configuration

There are two updater keys in play:

- `HUNK_UPDATE_PRIVATE_KEY_BASE64`
  - used only in CI to sign manifests and OTA payloads
  - this must stay secret
- hardcoded updater public key
  - committed in `crates/hunk-updater/src/lib.rs`
  - used by shipped apps to verify manifests and OTA payloads
  - can still be overridden with `HUNK_UPDATE_PUBLIC_KEY` at runtime for local testing

The release workflow now reads:

- `HUNK_UPDATE_PRIVATE_KEY_BASE64` to generate `stable.json` and signatures

## GitHub Actions setup

Add these repository secrets:

1. `HUNK_UPDATE_PRIVATE_KEY_BASE64`
2. `HUNK_UPDATE_MANIFEST_R2_ACCESS_KEY_ID`
3. `HUNK_UPDATE_MANIFEST_R2_SECRET_ACCESS_KEY`
4. `HUNK_UPDATE_MANIFEST_R2_S3_API_URL`

For your current bucket, `HUNK_UPDATE_MANIFEST_R2_S3_API_URL` should be:

```text
https://127ee78df72c8aa0363d47c0672c195e.r2.cloudflarestorage.com/hunk-stable-releases
```

## Release output you need after tagging

After a release workflow runs, you need:

1. the GitHub Release assets
2. the generated updater manifest bundle

The workflow produces:

- release assets uploaded to the GitHub Release
- an artifact named `hunk-update-manifest`
- release assets that include the generated `stable.json` and `.sig` files under `dist/update-manifest`

## How to deploy the server

### Current setup: Cloudflare R2 public bucket

The release workflow can now upload the manifest files directly to your public R2 bucket.

The public URLs are:

- `https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev/stable.json`
- `https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev/stable.json.sig`

### Release procedure

1. Create a release tag such as `v0.0.2`.
2. Wait for the GitHub Actions release workflow to finish.
3. The workflow generates `stable.json` and `stable.json.sig`.
4. The workflow uploads both files to your R2 bucket automatically.
5. Confirm these URLs work:
   - `https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev/stable.json`
   - `https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev/stable.json.sig`
6. Confirm the JSON points to the correct GitHub Release asset URLs.

### Manual workflow test

There is also a separate manual workflow at:

- `.github/workflows/release-dispatch.yml`

Use it with `workflow_dispatch` when you want to test:

- artifact builds
- manifest generation
- detached signatures
- automatic R2 upload
- full OTA downloads from a temporary public test prefix

without touching the production tag-triggered release workflow.

What it does now:

- builds the normal macOS, Windows, and Linux artifacts
- uploads the OTA test assets to your public R2 bucket under:
  - `test/<github-run-id>/<github-run-attempt>/`
- generates `stable.json` against that same public prefix
- uploads `stable.json`, `stable.json.sig`, and the asset `.sig` files beside the assets
- writes the final manifest URL into the GitHub Actions step summary

Important:

- this manual workflow does **not** create or upload a GitHub Release
- it is intended for real end-to-end updater tests against temporary public R2 URLs
- by default it uses:
  - `https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev`
- you can override that with the `public_base_url` workflow input if you move buckets later

## How to test the updater

### Local manifest testing

You can override the production manifest URL:

```bash
HUNK_UPDATE_MANIFEST_URL=http://127.0.0.1:8080/stable.json
```

You can also override the public key at runtime for local builds:

```bash
HUNK_UPDATE_PUBLIC_KEY=<base64-public-key>
```

Serve a local directory:

```bash
cd /path/to/update-manifest
python3 -m http.server 8080
```

### App-side manual test

1. Build or install an older Hunk version.
2. Run the `Release Manual Test` workflow from GitHub Actions.
3. Copy the manifest URL from the workflow summary. It will look like:
   - `https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev/test/<run-id>/<attempt>/stable.json`
4. Point Hunk at that manifest:

```bash
export HUNK_UPDATE_MANIFEST_URL="https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev/test/<run-id>/<attempt>/stable.json"
```

5. Launch Hunk.
6. Use `Check for Updates...` or the Settings updater controls.
7. Confirm:
   - the app detects the newer version
   - `Install Update` appears
   - the update downloads
   - the app exits
   - the helper applies the update
   - the updated app relaunches

### Platform-specific tests

#### macOS

- install from the DMG first
- confirm the updater downloads the `.app.tar.gz`
- confirm the bundle is replaced and reopens cleanly

#### Windows

- install the MSI first
- confirm the updater downloads the new MSI
- confirm `msiexec` upgrades the install
- confirm the app relaunches

#### Linux direct tarball

- extract the tarball to a writable location
- run the bundled launcher
- confirm the tarball update replaces the bundle in place

#### Linux package manager installs

- install the `.deb` or `.rpm`
- confirm self-update stays disabled
- confirm the explanation text tells the user to update via the package manager

## What does not need to be deployed

You do **not** need:

- a database
- an API server
- authentication
- a custom binary download service

For the current design, the updater server is only a static manifest host.
