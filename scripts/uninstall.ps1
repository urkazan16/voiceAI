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

Write-Host "Uninstalling LocalFlow data in $root"
function Remove-Path {
  param([string]$Path)
  if (Test-Path -LiteralPath $Path) {
    Remove-Item -LiteralPath $Path -Recurse -Force
    Write-Host "removed $Path"
  }
}
if (-not $KeepHistory) {
  Remove-Path $root
} else {
  Remove-Path (Join-Path $root "audio")
  Remove-Path (Join-Path $root "models")
  Remove-Path (Join-Path $root "logs")
  Remove-Path (Join-Path $root "config")
  Remove-Path (Join-Path $root "localflow.lock")
  Remove-Path (Join-Path $root "clipboard-restore.txt")
  Write-Host "kept $(Join-Path $root 'database')"
}
Write-Host "LocalFlow data and models removed."
