param(
    [string]$CodexTag = ""
)

$ErrorActionPreference = "Stop"

function Get-CodexNativeBinary {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootDir
    )

    Get-ChildItem -Path $RootDir -Recurse -File |
        Where-Object { $_.Extension -eq ".exe" } |
        Where-Object { $_.Name -notmatch "sandbox|setup|command-runner" } |
        Sort-Object FullName |
        Select-Object -First 1
}

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
$destinationDir = Join-Path $rootDir "assets/codex-runtime/windows"

New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
New-Item -ItemType Directory -Path $extractDir -Force | Out-Null
try {
    Write-Host "Downloading Codex runtime from $downloadUrl"
    Invoke-WebRequest -Uri $downloadUrl -OutFile $archivePath
    Write-Host "Extracting Codex runtime archive $archivePath"
    Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force

    $selectedBinary = Get-CodexNativeBinary -RootDir $extractDir
    if ($null -eq $selectedBinary) {
        throw "Expected a native Windows Codex binary inside $archivePath"
    }

    if (Test-Path $destinationDir) {
        Remove-Item -Path $destinationDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $destinationDir -Force | Out-Null

    $stagedNativeBinary = Join-Path $destinationDir "codex.exe"
    $sourceRuntimeDir = $selectedBinary.Directory.FullName
    Get-ChildItem -Path $sourceRuntimeDir -Force | ForEach-Object {
        if ($_.PSIsContainer) {
            Copy-Item -Path $_.FullName -Destination $destinationDir -Recurse -Force
            return
        }

        if ($_.FullName -eq $selectedBinary.FullName) {
            Copy-Item -Path $_.FullName -Destination $stagedNativeBinary -Force
            return
        }

        Copy-Item -Path $_.FullName -Destination $destinationDir -Force
    }

    $sourceArchRoot = $selectedBinary.Directory.Parent
    if ($null -ne $sourceArchRoot) {
        $sourcePathDir = Join-Path $sourceArchRoot.FullName "path"
        if (Test-Path $sourcePathDir) {
            Copy-Item -Path $sourcePathDir -Destination (Join-Path $destinationDir "path") -Recurse -Force
        }
    }

    $launcherPath = Join-Path $destinationDir "codex.cmd"
    $launcherContents = @"
@ECHO off
SETLOCAL
SET "SCRIPT_DIR=%~dp0"
SET "PATH=%SCRIPT_DIR%path;%PATH%"
"%SCRIPT_DIR%codex.exe" %*
EXIT /b %ERRORLEVEL%
"@
    Set-Content -Path $launcherPath -Value $launcherContents -NoNewline

    $stagedLauncher = $launcherPath
    Write-Host "Prepared bundled Codex runtime at $destinationDir using native binary $($selectedBinary.FullName)"
    Write-Output $stagedLauncher
} finally {
    if (Test-Path $tempDir) {
        Remove-Item -Path $tempDir -Recurse -Force
    }
}
