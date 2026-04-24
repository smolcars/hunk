param()

$ErrorActionPreference = "Stop"
if ($PSVersionTable.PSVersion.Major -ge 7) {
    $PSNativeCommandUseErrorActionPreference = $true
}

function Get-WindowsPackagerVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Version
    )

    $versionPattern = '^(?<base>\d+\.\d+\.\d+)(?:[-+].*)?$'
    if ($Version -notmatch $versionPattern) {
        throw "Unsupported Hunk version '$Version'. Expected semver 'major.minor.patch[-prerelease][+build]'."
    }

    return $Matches["base"]
}

function Test-WindowsCodexRuntimeBundle {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RootDir
    )

    $runtimeDir = Join-Path $RootDir "assets/codex-runtime/windows"
    if (-not (Test-Path $runtimeDir -PathType Container)) {
        throw "Missing Windows Codex runtime directory: $runtimeDir"
    }

    foreach ($fileName in @("codex.cmd", "codex.exe")) {
        $filePath = Join-Path $runtimeDir $fileName
        if (-not (Test-Path $filePath -PathType Leaf)) {
            throw "Missing Windows Codex runtime file: $filePath"
        }
    }
}

function Get-WindowsRuntimeSidecarDlls {
    param(
        [Parameter(Mandatory = $true)]
        [string]$TargetDir,
        [Parameter(Mandatory = $true)]
        [string]$TargetTriple
    )

    $releaseDir = Join-Path $TargetDir "$TargetTriple/release"
    if (-not (Test-Path $releaseDir -PathType Container)) {
        throw "Missing Windows release directory: $releaseDir"
    }

    $sidecarMap = @{}

    Get-ChildItem -Path $releaseDir -Filter "*.dll" -File -ErrorAction SilentlyContinue |
        Sort-Object Name |
        ForEach-Object {
            $sidecarMap[$_.Name.ToLowerInvariant()] = $_.FullName
        }

    $buildDir = Join-Path $releaseDir "build"
    if (Test-Path $buildDir -PathType Container) {
        Get-ChildItem -Path $buildDir -Recurse -File -ErrorAction SilentlyContinue |
            Where-Object {
                $_.Extension -ieq ".dll" -and $_.FullName -match '[\\/]+ghostty-install[\\/]+(?:bin|lib)[\\/]'
            } |
            Sort-Object FullName |
            ForEach-Object {
                $key = $_.Name.ToLowerInvariant()
                if (-not $sidecarMap.ContainsKey($key)) {
                    $sidecarMap[$key] = $_.FullName
                }
            }
    }

    return @(
        $sidecarMap.GetEnumerator() |
            Sort-Object Name |
            ForEach-Object { Get-Item -LiteralPath $_.Value }
    )
}

function Stage-WindowsPackagerSidecars {
    param(
        [Parameter(Mandatory = $true)]
        [string]$TargetDir,
        [Parameter(Mandatory = $true)]
        [string]$TargetTriple
    )

    $sidecars = @(Get-WindowsRuntimeSidecarDlls -TargetDir $TargetDir -TargetTriple $TargetTriple)
    if (-not ($sidecars | Where-Object { $_.Name -ieq "ghostty-vt.dll" })) {
        throw "Failed to locate ghostty-vt.dll in the Windows build output; cannot build a self-contained MSI"
    }

    $stageDir = Join-Path $TargetDir "windows-packager-sidecars"
    if (Test-Path $stageDir) {
        Remove-Item -Path $stageDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $stageDir -Force | Out-Null

    $fileNames = New-Object System.Collections.Generic.List[string]
    foreach ($sidecar in $sidecars) {
        $destination = Join-Path $stageDir $sidecar.Name
        Copy-Item -Path $sidecar.FullName -Destination $destination -Force
        [void]$fileNames.Add($sidecar.Name)
        Write-Host ("Staging Windows runtime sidecar " + $sidecar.Name + " from " + $sidecar.FullName)
    }

    return @{
        Directory = $stageDir
        FileNames = @($fileNames)
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

    $validatedFileNames = @($ExpectedFileNames | Sort-Object -Unique)
    $validatedFileList = $validatedFileNames -join ", "
    Write-Host "Validated Windows MSI payload includes: $validatedFileList"
}

function Invoke-CargoPackagerWithManifestOverride {
    param(
        [Parameter(Mandatory = $true)]
        [string]$CargoTomlPath,
        [Parameter(Mandatory = $true)]
        [string]$CargoLockPath,
        [Parameter(Mandatory = $true)]
        [string]$OriginalVersion,
        [Parameter(Mandatory = $true)]
        [string]$WindowsPackagerVersion,
        [Parameter(Mandatory = $true)]
        [string]$TargetTriple,
        [Parameter(Mandatory = $true)]
        [string]$WorkingDirectory,
        [Parameter(Mandatory = $true)]
        [string]$PackagerOutDir,
        [string]$WindowsSidecarResourcePath
    )

    $originalCargoToml = Get-Content $CargoTomlPath -Raw
    $updatedCargoToml = $originalCargoToml
    if ($WindowsPackagerVersion -ne $OriginalVersion) {
        $versionPattern = '(?ms)^(\[package\]\s.*?^version = ")([^"]+)(")'
        $versionReplacement = '${1}' + $WindowsPackagerVersion + '${3}'
        $updatedCargoToml = [regex]::Replace($updatedCargoToml, $versionPattern, $versionReplacement, 1)

        if ($updatedCargoToml -eq $originalCargoToml) {
            throw "Failed to rewrite [package] version in $CargoTomlPath"
        }
    }

    if ($WindowsSidecarResourcePath) {
        $normalizedSidecarPath = $WindowsSidecarResourcePath.Replace('\', '/')
        $cargoTomlBeforeResourceRewrite = $updatedCargoToml
        $windowsResourcesReplacement = @(
            "resources = [",
            '  "../../assets/codex-runtime",',
            '  "../../assets/browser-runtime",',
            "  { src = ""$normalizedSidecarPath"", target = ""."" },",
            "]"
        ) -join "`n"
        $resourcePattern = '(?ms)^resources\s*=\s*\[\s*"../../assets/codex-runtime"\s*,\s*"../../assets/browser-runtime"\s*,?\s*\]\s*$'
        $updatedCargoToml = [regex]::Replace($updatedCargoToml, $resourcePattern, $windowsResourcesReplacement, 1)

        if ($updatedCargoToml -eq $cargoTomlBeforeResourceRewrite) {
            throw "Failed to inject Windows sidecar DLL resources into $CargoTomlPath"
        }
    }

    $originalCargoLockBytes = $null
    $cargoLockExisted = Test-Path $CargoLockPath
    if ($cargoLockExisted) {
        $originalCargoLockBytes = [System.IO.File]::ReadAllBytes($CargoLockPath)
    }

    $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
    try {
        [System.IO.File]::WriteAllText($CargoTomlPath, $updatedCargoToml, $utf8NoBom)
        if ($WindowsPackagerVersion -ne $OriginalVersion) {
            Write-Host "Using Windows packager version $WindowsPackagerVersion for Cargo version $OriginalVersion"
        }
        Push-Location $WorkingDirectory
        try {
            cargo packager `
                -p hunk-desktop `
                --manifest-path Cargo.toml `
                --release `
                -f wix `
                --target $TargetTriple `
                --out-dir $PackagerOutDir
        } finally {
            Pop-Location
        }
    } finally {
        [System.IO.File]::WriteAllText($CargoTomlPath, $originalCargoToml, $utf8NoBom)
        if ($cargoLockExisted) {
            [System.IO.File]::WriteAllBytes($CargoLockPath, $originalCargoLockBytes)
        } elseif (Test-Path $CargoLockPath) {
            Remove-Item -Path $CargoLockPath -Force -ErrorAction SilentlyContinue
        }
    }
}

$rootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$validateBundleScript = Join-Path $PSScriptRoot "validate_windows_release_bundle.ps1"
$cargoTomlPath = Join-Path $rootDir "crates/hunk-desktop/Cargo.toml"
$cargoLockPath = Join-Path $rootDir "Cargo.lock"
$desktopCrateDir = Split-Path $cargoTomlPath -Parent
$targetTriple = "x86_64-pc-windows-msvc"
$targetDir = Join-Path $rootDir "target"
$versionLabel = if ($env:HUNK_RELEASE_VERSION) {
    $env:HUNK_RELEASE_VERSION
} else {
    $versionLine = Get-Content $cargoTomlPath | Select-String '^version = "' | Select-Object -First 1
    if (-not $versionLine) {
        throw "Failed to resolve Hunk version from $cargoTomlPath"
    }
    [regex]::Match($versionLine.Line, '^version = "(.*)"$').Groups[1].Value
}
$windowsPackagerVersion = Get-WindowsPackagerVersion -Version $versionLabel
$windowsSidecarBundle = $null
$windowsSidecarResourcePath = $null
$windowsSidecarFileNames = @()

Push-Location $rootDir
try {
    $packagerOutDir = Join-Path $targetDir "packager"
    Write-Host "Downloading bundled Codex runtime for Windows..."
    & ./scripts/download_codex_runtime_windows.ps1 | Out-Null
    Write-Host "Validating bundled Codex runtime for Windows..."
    Test-WindowsCodexRuntimeBundle -RootDir $rootDir
    & $validateBundleScript -RootDir $rootDir
    Write-Host "Building Windows release binary..."
    cargo build -p hunk-desktop --release --target $targetTriple --locked
    $windowsSidecarBundle = Stage-WindowsPackagerSidecars -TargetDir $targetDir -TargetTriple $targetTriple
    if ($windowsSidecarBundle.Directory) {
        $windowsSidecarResourcePath = [System.IO.Path]::GetRelativePath($desktopCrateDir, $windowsSidecarBundle.Directory)
        $windowsSidecarFileNames = @($windowsSidecarBundle.FileNames)
    }
    Write-Host "Building Windows MSI package..."
    Invoke-CargoPackagerWithManifestOverride `
        -CargoTomlPath $cargoTomlPath `
        -CargoLockPath $cargoLockPath `
        -OriginalVersion $versionLabel `
        -WindowsPackagerVersion $windowsPackagerVersion `
        -TargetTriple $targetTriple `
        -WorkingDirectory $desktopCrateDir `
        -PackagerOutDir $packagerOutDir `
        -WindowsSidecarResourcePath $windowsSidecarResourcePath
    & $validateBundleScript -RootDir $rootDir -PackagerOutDir $packagerOutDir
} finally {
    Pop-Location
}

$distDir = Join-Path $targetDir "dist"
$bundleMsi = Get-ChildItem -Path $packagerOutDir -Filter "*.msi" | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
$releaseMsiPath = Join-Path $distDir "Hunk-$versionLabel-windows-x86_64.msi"

if (-not $bundleMsi) {
    if (Test-Path $packagerOutDir) {
        Write-Host "Packager output under ${packagerOutDir}:"
        Get-ChildItem -Path $packagerOutDir -Recurse | ForEach-Object {
            Write-Host (" - " + $_.FullName)
        }
    }
    throw "Expected cargo-packager to produce an MSI under $packagerOutDir"
}

New-Item -ItemType Directory -Path $distDir -Force | Out-Null
Assert-WindowsMsiContainsFiles -MsiPath $bundleMsi.FullName -ExpectedFileNames $windowsSidecarFileNames
Copy-Item -Path $bundleMsi.FullName -Destination $releaseMsiPath -Force

Write-Host "Created Windows release artifact at $releaseMsiPath"

Write-Output $releaseMsiPath
