#Requires -Version 5.1
param(
  [switch]$KeepHistory
)
$ErrorActionPreference = "Stop"

if (-not $KeepHistory -and [Environment]::UserInteractive) {
  $ans = Read-Host "Keep dictation history? [y/N]"
  if ($ans -match '^[Yy]') {
    $KeepHistory = $true
  }
}

$root = if ($env:LOCALFLOW_DATA_DIR) {
  $env:LOCALFLOW_DATA_DIR
} else {
  Join-Path $env:APPDATA "LocalFlow"
}

$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
if (Get-ItemProperty -Path $runKey -Name "LocalFlow" -ErrorAction SilentlyContinue) {
  Remove-ItemProperty -Path $runKey -Name "LocalFlow"
  Write-Host "removed autostart registry value LocalFlow"
}

$removed = @()
function Remove-Path {
  param([string]$Path)
  if (Test-Path -LiteralPath $Path) {
    Remove-Item -LiteralPath $Path -Recurse -Force
    $script:removed += $Path
    Write-Host "removed $Path"
  }
}

Write-Host "Uninstalling LocalFlow data in $root"
Remove-Path (Join-Path $root "audio")
Remove-Path (Join-Path $root "models")
Remove-Path (Join-Path $root "logs")
Remove-Path (Join-Path $root "config")
if (-not $KeepHistory) {
  Remove-Path (Join-Path $root "database")
  if (Test-Path -LiteralPath $root) {
    try {
      Remove-Item -LiteralPath $root -Force -ErrorAction Stop
      $removed += $root
    } catch {
      Write-Host "kept $root (not empty)"
    }
  }
} else {
  Write-Host "kept $(Join-Path $root 'database')"
}

Write-Host "Removed components:"
if ($removed.Count -eq 0) {
  Write-Host "  (none)"
} else {
  $removed | ForEach-Object { Write-Host "  $_" }
}
