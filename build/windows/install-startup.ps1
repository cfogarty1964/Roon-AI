# Roon AI — Startup folder shortcut installer
#
# Creates a shortcut in the per-user Startup folder so roon-ai launches at
# Windows login. Pass -Uninstall to remove it.
#
# Usage:
#   .\install-startup.ps1                                   # use default exe path
#   .\install-startup.ps1 -ExePath "D:\path\to\roon-ai.exe" # explicit path
#   .\install-startup.ps1 -Uninstall                        # remove the shortcut

param(
    [string]$ExePath = "",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"

$startupFolder = [Environment]::GetFolderPath("Startup")
$shortcutPath = Join-Path $startupFolder "Roon AI.lnk"

if ($Uninstall) {
    if (Test-Path $shortcutPath) {
        Remove-Item $shortcutPath -Force
        Write-Host "Removed: $shortcutPath" -ForegroundColor Green
    } else {
        Write-Host "No shortcut found at: $shortcutPath" -ForegroundColor Yellow
    }
    exit 0
}

# Resolve the exe path
if (-not $ExePath) {
    # Default: assume this script lives at <repo>/build/windows/install-startup.ps1
    # so the release binary is at <repo>/target/release/roon-ai.exe
    $repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
    $ExePath = Join-Path $repoRoot "target\release\roon-ai.exe"
}

if (-not (Test-Path $ExePath)) {
    Write-Error "Binary not found: $ExePath"
    Write-Error "Build it first with: cargo build --features server --release"
    exit 1
}

# Resolve to absolute path so the shortcut points correctly even if cwd changes
$ExePath = (Resolve-Path $ExePath).Path
$workingDir = Split-Path -Parent $ExePath

# Create the shortcut via WScript.Shell COM object
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $ExePath
$shortcut.WorkingDirectory = $workingDir
$shortcut.Description = "Roon AI - hi-fi control bridge with conversational AI"
$shortcut.WindowStyle = 7  # Minimised (so even if console were visible, it'd be hidden)
$shortcut.Save()

Write-Host "Installed: $shortcutPath" -ForegroundColor Green
Write-Host "  Target:   $ExePath"
Write-Host "  WorkDir:  $workingDir"
Write-Host ""
Write-Host "Roon AI will now launch at every Windows login." -ForegroundColor Cyan
Write-Host "Right-click the system tray icon to access menu (Open Web UI / Quit / etc.)"
Write-Host ""
Write-Host "To remove: .\install-startup.ps1 -Uninstall"
