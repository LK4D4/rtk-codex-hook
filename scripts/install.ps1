$ErrorActionPreference = "Stop"

$Repo = if ($env:RTK_CODEX_HOOK_REPO) { $env:RTK_CODEX_HOOK_REPO } else { "LK4D4/rtk-codex-hook" }
$InstallDir = if ($env:RTK_CODEX_HOOK_INSTALL_DIR) {
    $env:RTK_CODEX_HOOK_INSTALL_DIR
} elseif ($env:CARGO_HOME) {
    Join-Path $env:CARGO_HOME "bin"
} else {
    Join-Path $env:USERPROFILE ".cargo\bin"
}

$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($Arch -ne [System.Runtime.InteropServices.Architecture]::X64) {
    throw "unsupported Windows architecture: $Arch"
}

$Asset = "rtk-codex-hook-x86_64-pc-windows-msvc.zip"
$Url = "https://github.com/$Repo/releases/latest/download/$Asset"
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

    Write-Output "Installed rtk-codex-hook to $Destination"
}
finally {
    Remove-Item -LiteralPath $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
