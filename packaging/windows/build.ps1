# Builds dist\TalkNap.exe (the release asset; it installs itself as TalkNap.exe) and
# dist\SHA256SUMS.txt. Needs Rust (rustup, x86_64-pc-windows-msvc) and the Visual Studio C++ build tools.
#   powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1                 # version from VERSION
#   powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1 -Version 1.2.0  # override (the release workflow checks it matches the tag)

param([string] $Version)

$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..\..')
if (-not $Version) { $Version = (Get-Content (Join-Path $repo 'VERSION') -Raw).Trim() }
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "version must look like 1.2.3 (got '$Version')" }

Push-Location $repo
try {
  $env:TALKNAP_VERSION = $Version
  cargo build --release --target x86_64-pc-windows-msvc
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
} finally { Pop-Location }

$dist = Join-Path $repo 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$out = Join-Path $dist 'TalkNap.exe'
Copy-Item (Join-Path $repo 'target\x86_64-pc-windows-msvc\release\TalkNap.exe') $out -Force

$hash = (Get-FileHash $out -Algorithm SHA256).Hash.ToLower()
"$hash  TalkNap.exe" | Set-Content -Path (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii
Write-Host "sha256 $hash"
Write-Host "built $out (version $Version)"
