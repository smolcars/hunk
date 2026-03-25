param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CargoArgs
)

$ErrorActionPreference = "Stop"
if ($PSVersionTable.PSVersion.Major -ge 7) {
    $PSNativeCommandUseErrorActionPreference = $true
}

$rootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$targetDirScript = Join-Path $PSScriptRoot "resolve_cargo_target_dir.ps1"
$downloadRuntimeScript = Join-Path $PSScriptRoot "download_codex_runtime_windows.ps1"
$runtimeDir = Join-Path $rootDir "assets/codex-runtime/windows"
$runtimeLauncher = Join-Path $runtimeDir "codex.cmd"
$runtimeBinary = Join-Path $runtimeDir "codex.exe"

if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    $env:CARGO_TARGET_DIR = (& $targetDirScript $rootDir).Trim()
}

if (-not (Test-Path $runtimeLauncher) -or -not (Test-Path $runtimeBinary)) {
    Write-Host "Bundled Windows Codex runtime not found; downloading pinned runtime..."
    & $downloadRuntimeScript | Out-Null
}

$resolvedRuntimeBinary = (Resolve-Path $runtimeBinary).Path
$env:HUNK_CODEX_EXECUTABLE = $resolvedRuntimeBinary

Write-Host "Using CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
Write-Host "Using HUNK_CODEX_EXECUTABLE=$resolvedRuntimeBinary"

Push-Location $rootDir
try {
    cargo run --release -p hunk-desktop @CargoArgs
} finally {
    Pop-Location
}
