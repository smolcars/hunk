param(
    [Parameter(Mandatory = $true)]
    [string]$RuntimeDir,
    [ValidateSet("runtime", "package")]
    [string]$Mode = "runtime",
    [string]$PackageDir
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

function Test-BrowserCefRuntime {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    Assert-Exists -Path (Join-Path $Path "libcef.dll") -Description "Windows CEF libcef.dll"
    Assert-Exists -Path (Join-Path $Path "chrome_elf.dll") -Description "Windows CEF chrome_elf.dll"
    Assert-Exists -Path (Join-Path $Path "icudtl.dat") -Description "Windows CEF ICU data"
    Assert-Exists -Path (Join-Path $Path "resources.pak") -Description "Windows CEF resources.pak"
    Assert-Exists -Path (Join-Path $Path "chrome_100_percent.pak") -Description "Windows CEF chrome_100_percent.pak"
    Assert-Exists -Path (Join-Path $Path "chrome_200_percent.pak") -Description "Windows CEF chrome_200_percent.pak"

    $localesDir = Join-Path $Path "locales"
    Assert-Exists -Path $localesDir -Description "Windows CEF locales directory"
    $localePak = Get-ChildItem -Path $localesDir -Filter "*.pak" -File -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $localePak) {
        throw "Windows CEF locales directory has no .pak files: $localesDir"
    }
}

Test-BrowserCefRuntime -Path $RuntimeDir

if ($Mode -eq "package") {
    if (-not $PackageDir) {
        throw "PackageDir is required when Mode is package."
    }
    Assert-Exists -Path (Join-Path $PackageDir "hunk-browser-helper.exe") -Description "Windows CEF helper"
    Test-BrowserCefRuntime -Path $PackageDir
}

Write-Host "CEF Windows validation passed."
