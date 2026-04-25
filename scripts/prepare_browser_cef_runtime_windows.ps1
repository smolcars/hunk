param(
    [string]$TargetTriple = "x86_64-pc-windows-msvc",
    [string]$RuntimeDir
)

$ErrorActionPreference = "Stop"
if ($PSVersionTable.PSVersion.Major -ge 7) {
    $PSNativeCommandUseErrorActionPreference = $true
}

$rootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (-not $RuntimeDir) {
    $RuntimeDir = Join-Path $rootDir "assets/browser-runtime/cef/windows/runtime"
}

if ($TargetTriple -notin @("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc", "i686-pc-windows-msvc")) {
    throw "Unsupported Windows CEF target: $TargetTriple"
}

$cefRsRepo = if ($env:HUNK_CEF_RS_REPO) { $env:HUNK_CEF_RS_REPO } else { "https://github.com/tauri-apps/cef-rs.git" }
$cefRsRev = if ($env:HUNK_CEF_RS_REV) { $env:HUNK_CEF_RS_REV } else { "f20249dd2e34afdc0102af347f30f0218dd67e7b" }
$cefRsDir = if ($env:HUNK_CEF_RS_DIR) { $env:HUNK_CEF_RS_DIR } else { Join-Path $env:TEMP "cef-rs" }
$forceExport = $env:HUNK_CEF_FORCE_EXPORT -eq "1"
$validator = Join-Path $PSScriptRoot "validate_browser_cef_windows.ps1"

function Test-ExistingRuntime {
    if ($forceExport) {
        return $false
    }

    try {
        & $validator -RuntimeDir $RuntimeDir | Out-Null
        return $true
    } catch {
        return $false
    }
}

if (Test-ExistingRuntime) {
    Write-Host "Using existing Windows CEF runtime at $RuntimeDir"
    exit 0
}

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    throw "git is required to fetch cef-rs"
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo is required to run cef-rs export-cef-dir"
}

if (-not (Test-Path (Join-Path $cefRsDir ".git") -PathType Container)) {
    if (Test-Path $cefRsDir) {
        Remove-Item -Path $cefRsDir -Recurse -Force
    }
    git clone --depth=1 $cefRsRepo $cefRsDir
}

git -C $cefRsDir fetch --depth=1 origin $cefRsRev
git -C $cefRsDir checkout --detach $cefRsRev | Out-Null

$runtimeParent = Split-Path -Parent $RuntimeDir
New-Item -ItemType Directory -Path $runtimeParent -Force | Out-Null

Write-Host "Exporting Windows CEF runtime for $TargetTriple to $RuntimeDir"
Push-Location $cefRsDir
try {
    cargo run -p export-cef-dir -- --force --target $TargetTriple $RuntimeDir
} finally {
    Pop-Location
}

& $validator -RuntimeDir $RuntimeDir
Write-Host "CEF runtime ready at $RuntimeDir"
