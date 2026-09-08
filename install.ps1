#Requires -Version 5.1
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

function Require-Command {
  param([string]$Name, [string]$Hint)
  if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
    Write-Error $Hint
  }
}

Require-Command cargo "Install Rust 1.88+ first: https://rustup.rs"
Require-Command npm "Install Node.js 20+ first: https://nodejs.org"

npm install
if ($env:LOCALFLOW_SKIP_MODEL_DOWNLOAD -ne "1") {
  Write-Host "Downloading Whisper Medium (~1.5 GB) from Hugging Face (SHA-256 verified)..."
  & node scripts/run-with-toolchain.mjs cargo run --manifest-path src-tauri/Cargo.toml --quiet -- download --model whisper-medium
  if ($LASTEXITCODE -ne 0) {
    Write-Host "Whisper download skipped (offline?). LocalFlow will retry on first launch."
  }
}
Write-Host "LocalFlow is ready. Run: npm run tauri dev"
Write-Host "Hold Control+Shift+Space to record; release to process."
Write-Host "Windows needs a microphone permission for LocalFlow. Paste uses Ctrl+V."
