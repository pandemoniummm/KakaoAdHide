<#
.SYNOPSIS
  Signs a release and writes latest-windows.json, the update feed the app reads.

.DESCRIPTION
  The signature covers "KakaoAdHide\n<version>\n<sha256 of the exe>", so a signed file can't be passed off as
  another version (no downgrade by relabelling an older release).
  The private key (RSA, XML) comes from the SIGNING_PRIVATE_KEY environment variable (the repository secret
  in the release workflow) or from -PrivateKeyFile. It is never written to disk by this script.

.EXAMPLE
  $env:SIGNING_PRIVATE_KEY = Get-Content ~\.keys\signing-primary-private.xml -Raw
  powershell -File tools\sign-release.ps1 -Exe dist\KakaoAdHide-windows.exe -Version 1.0.1 -Tag v1.0.1
#>
param(
  [Parameter(Mandatory = $true)] [string] $Exe,
  [Parameter(Mandatory = $true)] [string] $Version,
  [string] $Tag = ('v' + $Version),
  [string] $Repo = 'pandemoniummm/KakaoAdHide',
  [string] $PrivateKeyFile,
  [string] $Out = (Join-Path (Split-Path $Exe -Parent) 'latest-windows.json'),
  [string] $AssetUrl   # override, for tests
)

$ErrorActionPreference = 'Stop'

$keyXml = if ($PrivateKeyFile) { [IO.File]::ReadAllText($PrivateKeyFile) } else { $env:SIGNING_PRIVATE_KEY }
if (-not $keyXml) { throw 'No signing key: set SIGNING_PRIVATE_KEY or pass -PrivateKeyFile' }
$keyXml = $keyXml.Trim([char]0xFEFF, ' ', "`t", "`r", "`n")   # tolerate a BOM or stray whitespace around the key

$bytes = [IO.File]::ReadAllBytes((Resolve-Path $Exe).Path)
$sha = (New-Object Security.Cryptography.SHA256Managed).ComputeHash($bytes)
$shaHex = -join ($sha | ForEach-Object { $_.ToString('x2') })
$message = [Text.Encoding]::UTF8.GetBytes("KakaoAdHide`n$Version`n$shaHex")

$rsa = New-Object Security.Cryptography.RSACryptoServiceProvider
try {
  $rsa.PersistKeyInCsp = $false
  $rsa.FromXmlString($keyXml)
  if ($rsa.PublicOnly) { throw 'SIGNING_PRIVATE_KEY holds a public key, not a private key' }
  $sig = [Convert]::ToBase64String($rsa.SignData($message, 'SHA256'))
} finally { $rsa.Dispose() }

if (-not $AssetUrl) { $AssetUrl = "https://github.com/$Repo/releases/download/$Tag/" + (Split-Path $Exe -Leaf) }
$feed = [ordered]@{
  version   = $Version
  url       = $AssetUrl
  size      = $bytes.Length
  sha256    = $shaHex
  signature = $sig
  page      = "https://github.com/$Repo/releases/tag/$Tag"
}
[IO.File]::WriteAllText($Out, ($feed | ConvertTo-Json), (New-Object Text.UTF8Encoding $false))
Write-Host "wrote $Out (version $Version, sha256 $shaHex)"
