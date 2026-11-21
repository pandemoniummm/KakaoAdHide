<#
.SYNOPSIS
  Creates the update signing keys and puts the public keys into src/keys.rs (both platforms verify updates with
  the same keys).

.DESCRIPTION
  Makes two RSA-3072 key pairs:
    primary  - signs every release. Its private key goes into the SIGNING_PRIVATE_KEY repository secret.
    backup   - kept offline. Only needed if the primary key is lost or has to be replaced.
  Files in the key folder (default %USERPROFILE%\.keys, outside the repository):
    signing-primary-private.xml, signing-backup-private.xml   private keys (never printed)
    signing-primary-public.xml,  signing-backup-public.xml    public keys (safe to keep anywhere)
  Existing files are never overwritten. To use the same keys in another repository, run it there with
  -UseExisting (it only needs the public key files).

.PARAMETER KeyDir
  The key folder. Default: %USERPROFILE%\.keys

.PARAMETER SetSecret
  Also store the primary private key as the SIGNING_PRIVATE_KEY secret of this repository (needs the gh CLI,
  logged in).

.PARAMETER UseExisting
  Don't create keys; only (re)write src/keys.rs from the existing public key files.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\new-signing-keys.ps1 -SetSecret
#>
param(
  [string] $KeyDir = (Join-Path $env:USERPROFILE '.keys'),
  [switch] $SetSecret,
  [switch] $UseExisting
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$KeyDir = [IO.Path]::GetFullPath($KeyDir)
if (($KeyDir + '\').StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) {
  throw "The key folder must be outside the repository ($root)"
}
$utf8 = New-Object Text.UTF8Encoding $false

function Key-File([string] $name, [string] $kind) { Join-Path $KeyDir "signing-$name-$kind.xml" }

function New-KeyPair([string] $name) {
  $priv = Key-File $name 'private'; $pub = Key-File $name 'public'
  if ((Test-Path -LiteralPath $priv) -or (Test-Path -LiteralPath $pub)) { Write-Host "keeping existing $name key"; return }
  $rsa = New-Object System.Security.Cryptography.RSACryptoServiceProvider 3072
  try {
    $rsa.PersistKeyInCsp = $false
    [IO.File]::WriteAllText($priv, $rsa.ToXmlString($true), $utf8)
    [IO.File]::WriteAllText($pub, $rsa.ToXmlString($false), $utf8)
  } finally { $rsa.Dispose() }
  Write-Host "created  $priv"
  Write-Host "created  $pub"
}

function Get-PublicXml([string] $name) {
  $pub = Key-File $name 'public'
  if (Test-Path -LiteralPath $pub) { return ([IO.File]::ReadAllText($pub)).Trim() }
  $priv = Key-File $name 'private'
  if (-not (Test-Path -LiteralPath $priv)) { throw "no $name key in $KeyDir" }
  $rsa = New-Object System.Security.Cryptography.RSACryptoServiceProvider
  try { $rsa.PersistKeyInCsp = $false; $rsa.FromXmlString([IO.File]::ReadAllText($priv)); return $rsa.ToXmlString($false) }
  finally { $rsa.Dispose() }
}

if (-not $UseExisting) {
  New-Item -ItemType Directory -Force -Path $KeyDir | Out-Null
  New-KeyPair 'primary'
  New-KeyPair 'backup'
}

# public keys -> src/keys.rs
$keys = @((Get-PublicXml 'primary'), (Get-PublicXml 'backup'))
$lines = @(
  '// Public keys that update packages must be signed with (RSA, .NET XML format). Written by'
  '// tools/new-signing-keys.ps1. An update is accepted when its signature matches any of these keys.'
  '//   [0] primary: its private key is the SIGNING_PRIVATE_KEY repository secret (used by the release workflow)'
  '//   [1] backup : its private key is kept offline; used only if the primary is lost or has to be replaced'
  '// These are the keys KakaoAdHide 1.0.0 (Windows) shipped with: changing them strands installed copies.'
  ''
  'pub const PUBLIC_KEYS: &[&str] = &['
)
foreach ($k in $keys) { $lines += '    "' + $k + '",' }
$lines += '];'
$out = Join-Path $root 'src\keys.rs'
[IO.File]::WriteAllText($out, (($lines -join "`n") + "`n"), $utf8)
Write-Host "wrote    $out"

if ($SetSecret) {
  $priv = Key-File 'primary' 'private'
  if (-not (Test-Path -LiteralPath $priv)) { throw "no primary private key: $priv" }
  if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw 'gh CLI not found' }
  # Feed the file to gh directly. Piping a string from Windows PowerShell to a native program re-encodes
  # it with $OutputEncoding, which can prepend a BOM and corrupt the key.
  $p = Start-Process gh -ArgumentList 'secret', 'set', 'SIGNING_PRIVATE_KEY' -WorkingDirectory $root `
         -RedirectStandardInput $priv -NoNewWindow -Wait -PassThru
  if ($p.ExitCode -ne 0) { throw 'gh secret set failed' }
  Write-Host 'stored   SIGNING_PRIVATE_KEY repository secret'
}

Write-Host ''
Write-Host 'Next:'
Write-Host '  1. Back up BOTH private keys (password manager, encrypted USB):'
Write-Host ('       ' + (Key-File 'primary' 'private'))
Write-Host ('       ' + (Key-File 'backup' 'private'))
Write-Host '     GitHub secrets cannot be read back, so your backup is the only copy you can recover from.'
Write-Host '  2. The backup private key is not needed for releases: keep it offline and delete it from this PC.'
if (-not $SetSecret) { Write-Host '  3. Store the primary key as the SIGNING_PRIVATE_KEY secret: run again with -UseExisting -SetSecret' }
Write-Host '  Then commit src/keys.rs (it holds only the public keys).'
