param()

$ErrorActionPreference = "Stop"

$rootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$cargoTomlPath = Join-Path $rootDir "crates/hunk-desktop/Cargo.toml"
$versionLabel = if ($env:HUNK_RELEASE_VERSION) {
    $env:HUNK_RELEASE_VERSION
} else {
    $versionLine = Get-Content $cargoTomlPath | Select-String '^version = "' | Select-Object -First 1
    if (-not $versionLine) {
        throw "Failed to resolve Hunk version from $cargoTomlPath"
    }
    [regex]::Match($versionLine.Line, '^version = "(.*)"$').Groups[1].Value
}

Push-Location $rootDir
try {
    $targetDir = (bash ./scripts/resolve_cargo_target_dir.sh $rootDir).Trim()
    Write-Host "Downloading bundled Codex runtime for Windows..."
    & ./scripts/download_codex_runtime_windows.ps1 | Out-Null
    Write-Host "Validating bundled Codex runtime for Windows..."
    bash ./scripts/validate_codex_runtime_bundle.sh --strict --platform windows | Out-Null
    Write-Host "Building Windows MSI bundle..."
    cargo bundle -p hunk-desktop --format msi --release --target x86_64-pc-windows-msvc
} finally {
    Pop-Location
}

$distDir = Join-Path $targetDir "dist"
$bundleMsiPath = Join-Path $targetDir "x86_64-pc-windows-msvc/release/bundle/msi/Hunk.msi"
$releaseMsiPath = Join-Path $distDir "Hunk-$versionLabel-windows-x86_64.msi"

if (-not (Test-Path $bundleMsiPath)) {
    throw "Expected MSI output at $bundleMsiPath"
}

New-Item -ItemType Directory -Path $distDir -Force | Out-Null
Copy-Item -Path $bundleMsiPath -Destination $releaseMsiPath -Force

Write-Host "Created Windows release artifact at $releaseMsiPath"

Write-Output $releaseMsiPath
