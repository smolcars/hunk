param(
    [string]$RootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
)

$ErrorActionPreference = "Stop"

function Convert-BashPathToWindowsPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if ($Path -match '^/mnt/([A-Za-z])/(.*)$') {
        $drive = $Matches[1].ToUpperInvariant()
        $rest = $Matches[2].Replace('/', '\')
        return "{0}:\{1}" -f $drive, $rest
    }

    if ($Path -match '^/([A-Za-z])/(.*)$') {
        $drive = $Matches[1].ToUpperInvariant()
        $rest = $Matches[2].Replace('/', '\')
        return "{0}:\{1}" -f $drive, $rest
    }

    if ($Path -match '^[A-Za-z]:/') {
        return $Path.Replace('/', '\')
    }

    return $Path
}

Push-Location $RootDir
try {
    $targetDir = (bash ./scripts/resolve_cargo_target_dir.sh).Trim()
} finally {
    Pop-Location
}

if ([string]::IsNullOrWhiteSpace($targetDir)) {
    throw "Failed to resolve CARGO_TARGET_DIR via scripts/resolve_cargo_target_dir.sh"
}

Write-Output ([System.IO.Path]::GetFullPath((Convert-BashPathToWindowsPath -Path $targetDir)))
