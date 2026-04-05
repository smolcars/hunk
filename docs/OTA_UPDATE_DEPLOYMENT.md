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
- `HUNK_UPDATE_PUBLIC_KEY`
  - embedded into release builds at compile time
  - also accepted as a runtime env override for local testing

The release workflow now reads:

- `HUNK_UPDATE_PRIVATE_KEY_BASE64` to generate `stable.json` and signatures
- `HUNK_UPDATE_PUBLIC_KEY` while compiling release builds so shipped apps can verify downloads

## GitHub Actions setup

Add these repository secrets:

1. `HUNK_UPDATE_PRIVATE_KEY_BASE64`
2. `HUNK_UPDATE_PUBLIC_KEY`
3. `HUNK_UPDATE_MANIFEST_R2_ACCESS_KEY_ID`
4. `HUNK_UPDATE_MANIFEST_R2_SECRET_ACCESS_KEY`
5. `HUNK_UPDATE_MANIFEST_R2_S3_API_URL`

You can keep the public key in a secret for convenience even though it is not sensitive.

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

without touching the production tag-triggered release workflow.

Important:

- this manual workflow does **not** create or upload a GitHub Release
- it writes whatever `asset_base_url` you pass into `stable.json`
- that means it is good for testing R2 publishing and manifest generation, but end-to-end updater downloads only work if `asset_base_url` points to real public binaries

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
2. Publish a newer release and generate `stable.json`.
3. Point Hunk at the test manifest with `HUNK_UPDATE_MANIFEST_URL`.
4. Launch Hunk.
5. Use `Check for Updates...` or the Settings updater controls.
6. Confirm:
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
