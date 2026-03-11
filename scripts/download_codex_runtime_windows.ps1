param(
    [string]$CodexTag = ""
)

$ErrorActionPreference = "Stop"

$rootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($CodexTag)) {
    if ($env:HUNK_CODEX_TAG) {
        $CodexTag = $env:HUNK_CODEX_TAG
    } else {
        $cargoTomlPath = Join-Path $rootDir "crates/hunk-desktop/Cargo.toml"
        $codexTagLine = Get-Content $cargoTomlPath | Select-String 'tag = "rust-v[^"]+"' | Select-Object -First 1
        if (-not $codexTagLine) {
            throw "Failed to resolve Codex release tag from $cargoTomlPath"
        }
        $CodexTag = [regex]::Match($codexTagLine.Line, 'tag = "(rust-v[^"]+)"').Groups[1].Value
    }
}

$assetName = "codex-x86_64-pc-windows-msvc.exe.zip"
$downloadUrl = "https://github.com/openai/codex/releases/download/$CodexTag/$assetName"
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("hunk-codex-runtime-" + [System.Guid]::NewGuid().ToString("N"))
$archivePath = Join-Path $tempDir $assetName
$extractDir = Join-Path $tempDir "extract"
$destination = Join-Path $rootDir "assets/codex-runtime/windows/codex.exe"

New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
New-Item -ItemType Directory -Path $extractDir -Force | Out-Null
try {
    Write-Host "Downloading Codex runtime from $downloadUrl"
    Invoke-WebRequest -Uri $downloadUrl -OutFile $archivePath
    Write-Host "Extracting Codex runtime archive $archivePath"
    Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force

    $sourceBinary = Get-ChildItem -Path $extractDir -File -Filter "*.exe" -Recurse | Select-Object -First 1
    if (-not $sourceBinary) {
        throw "Expected a Windows Codex executable inside $archivePath"
    }

    New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force | Out-Null
    Copy-Item -Path $sourceBinary.FullName -Destination $destination -Force
    Write-Host "Prepared bundled Codex runtime at $destination"
    Write-Output $destination
} finally {
    if (Test-Path $tempDir) {
        Remove-Item -Path $tempDir -Recurse -Force
    }
}
