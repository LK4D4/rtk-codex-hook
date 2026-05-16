$ErrorActionPreference = "Stop"

$Repo = if ($env:RTK_CODEX_HOOK_REPO) { $env:RTK_CODEX_HOOK_REPO } else { "LK4D4/rtk-codex-hook" }
$InstallDir = if ($env:RTK_CODEX_HOOK_INSTALL_DIR) {
    $env:RTK_CODEX_HOOK_INSTALL_DIR
} else {
    $LocalRoot = if ($env:LOCALAPPDATA) {
        $env:LOCALAPPDATA
    } else {
        Join-Path $env:USERPROFILE "AppData\Local"
    }
    Join-Path $LocalRoot "rtk-codex-hook\bin"
}

function Test-PathListContains([string] $PathValue, [string] $Dir) {
    if (-not $PathValue) {
        return $false
    }
    $Expected = $Dir.TrimEnd('\')
    foreach ($Entry in $PathValue -split ';') {
        if ($Entry.Trim().TrimEnd('\').Equals($Expected, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }
    return $false
}

function Add-UserPath([string] $Dir) {
    if ($env:RTK_CODEX_HOOK_NO_PATH_UPDATE -eq "1") {
        return
    }
    $ProcessPath = [Environment]::GetEnvironmentVariable("Path", "Process")
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ((Test-PathListContains $ProcessPath $Dir) -or (Test-PathListContains $UserPath $Dir)) {
        return
    }

    if ([string]::IsNullOrWhiteSpace($UserPath)) {
        $NextUserPath = $Dir
    } else {
        $NextUserPath = $UserPath.TrimEnd(';') + ";" + $Dir
    }
    [Environment]::SetEnvironmentVariable("Path", $NextUserPath, "User")
    Write-Output "Added $Dir to User PATH. Open a new terminal before running rtk-codex-hook directly."
}

$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($Arch -ne [System.Runtime.InteropServices.Architecture]::X64) {
    throw "unsupported Windows architecture: $Arch"
}

$Asset = "rtk-codex-hook-x86_64-pc-windows-msvc.zip"
$DownloadBase = if ($env:RTK_CODEX_HOOK_DOWNLOAD_BASE_URL) {
    $env:RTK_CODEX_HOOK_DOWNLOAD_BASE_URL
} else {
    "https://github.com/$Repo/releases/latest/download"
}
$Url = "$DownloadBase/$Asset"
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("rtk-codex-hook-" + [System.Guid]::NewGuid())
$Archive = Join-Path $TempDir $Asset

New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

try {
    Invoke-WebRequest -Uri $Url -OutFile $Archive
    Expand-Archive -LiteralPath $Archive -DestinationPath $TempDir -Force
    $Binary = Get-ChildItem -LiteralPath $TempDir -Recurse -Filter "rtk-codex-hook.exe" |
        Select-Object -First 1
    if (-not $Binary) {
        throw "downloaded archive did not contain rtk-codex-hook.exe"
    }

    $Destination = Join-Path $InstallDir "rtk-codex-hook.exe"
    Copy-Item -LiteralPath $Binary.FullName -Destination $Destination -Force
    & $Destination --install-codex-hook
    Add-UserPath $InstallDir

    Write-Output "Installed rtk-codex-hook to $Destination"
}
finally {
    Remove-Item -LiteralPath $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
