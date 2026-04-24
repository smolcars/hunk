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

function Get-WindowsMsiFileNames {
    param(
        [Parameter(Mandatory = $true)]
        [string]$MsiPath
    )

    $installer = New-Object -ComObject WindowsInstaller.Installer
    $database = $installer.OpenDatabase($MsiPath, 0)
    $fileQuery = 'SELECT `FileName` FROM `File`'
    $view = $database.OpenView($fileQuery)
    $view.Execute()

    $fileNames = New-Object System.Collections.Generic.List[string]
    while ($true) {
        $record = $view.Fetch()
        if (-not $record) {
            break
        }

        $fileName = $record.StringData(1)
        $shortNameSeparator = [char]124
        if ($fileName.Contains($shortNameSeparator)) {
            $fileName = $fileName.Substring($fileName.LastIndexOf($shortNameSeparator) + 1)
        }
        [void]$fileNames.Add($fileName)
    }

    return @($fileNames)
}

function Assert-WindowsMsiContainsFiles {
    param(
        [Parameter(Mandatory = $true)]
        [string]$MsiPath,
        [string[]]$ExpectedFileNames
    )

    if (-not $ExpectedFileNames -or $ExpectedFileNames.Count -eq 0) {
        return
    }

    $actualFileNames = @(Get-WindowsMsiFileNames -MsiPath $MsiPath)
    $missing = @(
        $ExpectedFileNames |
            Sort-Object -Unique |
            Where-Object { $actualFileNames -notcontains $_ }
    )

    if ($missing.Count -gt 0) {
        $missingList = $missing -join ", "
        throw "Windows MSI is missing expected files: $missingList"
    }
}

$runtimeDir = Join-Path $RootDir "assets/codex-runtime/windows"
Assert-Exists -Path $runtimeDir -Description "Windows Codex runtime directory"
Assert-Exists -Path (Join-Path $runtimeDir "codex.cmd") -Description "Windows Codex launcher"
Assert-Exists -Path (Join-Path $runtimeDir "codex.exe") -Description "Windows Codex binary"

$cargoTomlPath = Join-Path $RootDir "crates/hunk-desktop/Cargo.toml"
Assert-Exists -Path $cargoTomlPath -Description "desktop packager manifest"
$cargoToml = Get-Content $cargoTomlPath -Raw
if ($cargoToml -notmatch '"../../assets/codex-runtime"') {
    throw "Desktop packager manifest no longer bundles ../../assets/codex-runtime"
}
if ($cargoToml -notmatch '"../../assets/browser-runtime"') {
    throw "Desktop packager manifest no longer bundles ../../assets/browser-runtime"
}

if ($PackagerOutDir) {
    Assert-Exists -Path $PackagerOutDir -Description "Windows packager output directory"
    $bundleMsi = Get-ChildItem -Path $PackagerOutDir -Filter "*.msi" -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if (-not $bundleMsi) {
        throw "Expected cargo-packager to produce an MSI under $PackagerOutDir"
    }

    Assert-WindowsMsiContainsFiles -MsiPath $bundleMsi.FullName -ExpectedFileNames @("ghostty-vt.dll")
}

Write-Host "Validated Windows release bundle inputs."
