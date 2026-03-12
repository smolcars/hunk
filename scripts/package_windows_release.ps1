param()

$ErrorActionPreference = "Stop"

$rootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$resolveTargetDirScript = Join-Path $PSScriptRoot "resolve_cargo_target_dir.ps1"
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
$originalCargoTargetDir = $env:CARGO_TARGET_DIR
try {
    $targetDir = (& $resolveTargetDirScript -RootDir $rootDir).Trim()
    $packagerOutDir = Join-Path $targetDir "packager"
    $env:CARGO_TARGET_DIR = $targetDir
    Write-Host "Downloading bundled Codex runtime for Windows..."
    & ./scripts/download_codex_runtime_windows.ps1 | Out-Null
    Write-Host "Validating bundled Codex runtime for Windows..."
    bash ./scripts/validate_codex_runtime_bundle.sh --strict --platform windows | Out-Null
    Write-Host "Building Windows release binary..."
    cargo build -p hunk-desktop --release --target x86_64-pc-windows-msvc --locked
    Write-Host "Building Windows MSI package..."
    cargo packager -p hunk-desktop --release -f wix --target x86_64-pc-windows-msvc --out-dir $packagerOutDir
} finally {
    if ($null -eq $originalCargoTargetDir) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $originalCargoTargetDir
    }
    Pop-Location
}

$distDir = Join-Path $targetDir "dist"
$bundleMsi = Get-ChildItem -Path $packagerOutDir -Filter "*.msi" | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
$releaseMsiPath = Join-Path $distDir "Hunk-$versionLabel-windows-x86_64.msi"

if (-not $bundleMsi) {
    throw "Expected cargo-packager to produce an MSI under $packagerOutDir"
}

New-Item -ItemType Directory -Path $distDir -Force | Out-Null
Copy-Item -Path $bundleMsi.FullName -Destination $releaseMsiPath -Force

Write-Host "Created Windows release artifact at $releaseMsiPath"

Write-Output $releaseMsiPath
