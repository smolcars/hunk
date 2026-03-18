param(
    [Parameter(Mandatory = $true)]
    [string]$RootDir,
    [string]$PackagerOutDir
)

$ErrorActionPreference = "Stop"
if ($PSVersionTable.PSVersion.Major -ge 7) {
    $PSNativeCommandUseErrorActionPreference = $true
}

function Assert-Exists {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$Description
    )

    if (-not (Test-Path $Path)) {
        throw "Missing ${Description}: ${Path}"
    }
}

function Assert-NoHelixReferences {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path $Path)) {
        return
    }

    $matches = Get-ChildItem -Path $Path -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Extension -in @(".wxs", ".wixproj", ".xml", ".json", ".toml", ".txt") } |
        Select-String -Pattern "helix|hx-runtime|queries|grammars" -SimpleMatch:$false

    if ($matches) {
        $details = $matches | ForEach-Object { "$($_.Path):$($_.LineNumber): $($_.Line.Trim())" }
        throw ("Found forbidden Helix-era packaging references:`n" + ($details -join "`n"))
    }
}

$runtimeDir = Join-Path $RootDir "assets/codex-runtime/windows"
Assert-Exists -Path $runtimeDir -Description "Windows Codex runtime directory"
Assert-Exists -Path (Join-Path $runtimeDir "codex.cmd") -Description "Windows Codex launcher"
Assert-Exists -Path (Join-Path $runtimeDir "codex.exe") -Description "Windows Codex binary"

$cargoTomlPath = Join-Path $RootDir "crates/hunk-desktop/Cargo.toml"
Assert-Exists -Path $cargoTomlPath -Description "desktop packager manifest"
$cargoToml = Get-Content $cargoTomlPath -Raw
if ($cargoToml -notmatch 'resources\s*=\s*\[\s*"../../assets/codex-runtime"\s*\]') {
    throw "Desktop packager manifest no longer bundles ../../assets/codex-runtime"
}
if ($cargoToml -match 'helix|hx-runtime|queries|grammars') {
    throw "Desktop packager manifest still references Helix-era resources"
}

Assert-NoHelixReferences -Path (Join-Path $RootDir ".github/workflows/release.yml")
Assert-NoHelixReferences -Path (Join-Path $RootDir "scripts")

if ($PackagerOutDir) {
    Assert-NoHelixReferences -Path $PackagerOutDir
}

Write-Host "Validated Windows release bundle inputs."
