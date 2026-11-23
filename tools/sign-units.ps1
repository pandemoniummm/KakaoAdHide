<#
.SYNOPSIS
  Adds the bottom banner's ad placement ids (ad-units/<platform>.txt), signed, to an update feed.

.DESCRIPTION
  KakaoTalk may change the bottom banner's id (DAN-...) between KakaoAdHide releases; the app then shows
  KakaoTalk's fallback banner. The feed carries the ids as "units", with their own "units_signature" over
  "KakaoAdHide-units\n<platform>\n<id>\n<id>..." (UTF-8, RSA PKCS#1 v1.5, SHA-256, same key as the release
  signature). The feed's other fields and their signature stay as they are, so 1.0.0 still reads the feed.
  The signature is checked against the public keys in src/keys.rs before the feed is written.
  The private key comes from SIGNING_PRIVATE_KEY or -PrivateKeyFile, as in sign-release.ps1.

.EXAMPLE
  ./tools/sign-units.ps1 -Feed dist/latest-windows.json -Platform windows
#>
param(
  [Parameter(Mandatory = $true)] [string] $Feed,
  [Parameter(Mandatory = $true)] [ValidateSet('windows', 'macos')] [string] $Platform,
  [string] $UnitsFile,   # default: ad-units/<platform>.txt
  [string] $PrivateKeyFile
)

$ErrorActionPreference = 'Stop'
if (-not $UnitsFile) { $UnitsFile = Join-Path (Join-Path (Split-Path $PSScriptRoot -Parent) 'ad-units') "$Platform.txt" }

$units = @([IO.File]::ReadAllLines((Resolve-Path $UnitsFile).Path) | ForEach-Object { $_.Trim() } | Where-Object { $_ })
if ($units.Count -eq 0 -or $units.Count -gt 20) { throw "$UnitsFile must list 1 to 20 ids" }
foreach ($u in $units) { if ($u -cnotmatch '^DAN-[A-Za-z0-9]{1,60}$') { throw "not an ad placement id: $u" } }

$keyXml = if ($PrivateKeyFile) { [IO.File]::ReadAllText($PrivateKeyFile) } else { $env:SIGNING_PRIVATE_KEY }
if (-not $keyXml) { throw 'No signing key: set SIGNING_PRIVATE_KEY or pass -PrivateKeyFile' }
$keyXml = $keyXml.Trim([char]0xFEFF, ' ', "`t", "`r", "`n")

$message = [Text.Encoding]::UTF8.GetBytes((@("KakaoAdHide-units", $Platform) + $units) -join "`n")
$rsa = New-Object Security.Cryptography.RSACryptoServiceProvider
try {
  $rsa.PersistKeyInCsp = $false
  $rsa.FromXmlString($keyXml)
  if ($rsa.PublicOnly) { throw 'SIGNING_PRIVATE_KEY holds a public key, not a private key' }
  $sigBytes = $rsa.SignData($message, 'SHA256')
} finally { $rsa.Dispose() }

# the app accepts only the keys in src/keys.rs: check against those, not just the private key's own half
$keysRs = [IO.File]::ReadAllText((Join-Path $PSScriptRoot '..\src\keys.rs'))
$verified = $false
foreach ($m in [regex]::Matches($keysRs, '<RSAKeyValue>.*?</RSAKeyValue>')) {
  $pub = New-Object Security.Cryptography.RSACryptoServiceProvider
  try {
    $pub.PersistKeyInCsp = $false
    $pub.FromXmlString($m.Value)
    if ($pub.VerifyData($message, 'SHA256', $sigBytes)) { $verified = $true }
  } finally { $pub.Dispose() }
}
if (-not $verified) { throw 'the signing key is not one of the public keys in src/keys.rs' }

$path = (Resolve-Path $Feed).Path
$text = [IO.File]::ReadAllText($path).TrimStart([char]0xFEFF)
$old = $text | ConvertFrom-Json
$out = [ordered]@{}
foreach ($p in $old.PSObject.Properties) { if ($p.Name -notin 'units', 'units_signature') { $out[$p.Name] = $p.Value } }
$out['units'] = $units
$out['units_signature'] = [Convert]::ToBase64String($sigBytes)
[IO.File]::WriteAllText($path, ($out | ConvertTo-Json), (New-Object Text.UTF8Encoding $false))
Write-Host "signed $($units.Count) ad placement id(s) for $Platform into $Feed"
