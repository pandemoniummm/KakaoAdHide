# Builds dist\KakaoAdHide.exe with the C# compiler that ships with Windows (.NET Framework 4.x).
# Nothing to install. Usage:  powershell -ExecutionPolicy Bypass -File build.ps1

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path $csc)) { $csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe' }
if (-not (Test-Path $csc)) { throw "csc.exe (.NET Framework 4.x) not found" }

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$out = Join-Path $dist 'KakaoAdHide.exe'
$src = Get-ChildItem (Join-Path $root 'src') -Filter *.cs | ForEach-Object { $_.FullName }

$cscArgs = @('/nologo', '/target:winexe', '/platform:anycpu', '/optimize+', '/codepage:65001', "/out:$out")
$ico = Join-Path $root 'assets\app.ico'
if (Test-Path $ico) { $cscArgs += "/win32icon:$ico" }

& $csc @cscArgs @src
if ($LASTEXITCODE -ne 0) { throw "build failed ($LASTEXITCODE)" }

$hash = (Get-FileHash $out -Algorithm SHA256).Hash.ToLower()
"$hash  KakaoAdHide.exe" | Set-Content -Path (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii
Write-Host "built $out"
Write-Host "sha256 $hash"
