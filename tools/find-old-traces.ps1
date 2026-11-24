# Read-only: lists what on this PC still carries the old name KakaoAdHide. Changes nothing.
# Output: target\old-traces.txt in the project folder (target\ is not committed).
$proj = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$null = New-Item -ItemType Directory -Force (Join-Path $proj 'target')
$out = Join-Path $proj 'target\old-traces.txt'
$lines = New-Object System.Collections.Generic.List[string]
function Add-Section($t) { $lines.Add(''); $lines.Add("=== $t") }
function Add-Out($o) { foreach ($l in @($o)) { if ("$l".Trim()) { $lines.Add("$l") } } }

$lines.Add("scan $(Get-Date -Format s)  admin=$(([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole('Administrators'))")

Add-Section 'files in user profile (name contains kakaoadhide; project folder excluded)'
Add-Out (cmd /c dir /s /b /a "$env:USERPROFILE\*kakaoadhide*" 2>$null | Where-Object { -not $_.StartsWith($proj, 'OrdinalIgnoreCase') })

Add-Section 'files in ProgramData, Prefetch, WER, appcompat'
foreach ($d in "$env:ProgramData", "$env:windir\Prefetch") { Add-Out (cmd /c dir /s /b /a "$d\*kakaoadhide*" 2>$null) }
$pca = "$env:windir\appcompat\pca"
if (Test-Path $pca) { Add-Out (Get-ChildItem $pca -File -ErrorAction SilentlyContinue | Select-String -Pattern 'kakaoadhide' -SimpleMatch -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Filename): $($_.Line)" }) }

Add-Section 'registry HKCU (keys, value names, data)'
Add-Out (reg query HKCU /f KakaoAdHide /s 2>$null)
Add-Section 'registry HKCU UserAssist (ROT13 XnxnbNqUvqr)'
Add-Out (reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist" /f XnxnbNqUvqr /s 2>$null)
Add-Section 'registry HKLM\SOFTWARE'
Add-Out (reg query HKLM\SOFTWARE /f KakaoAdHide /s 2>$null)

Add-Section 'certificates (CurrentUser and LocalMachine, any store)'
Add-Out (Get-ChildItem Cert:\CurrentUser, Cert:\LocalMachine -Recurse -ErrorAction SilentlyContinue | Where-Object { $_.Subject -match 'KakaoAdHide' } | ForEach-Object { "$($_.PSParentPath -replace '.*::','') | $($_.Subject) | $($_.Thumbprint) | until $($_.NotAfter.ToString('yyyy-MM-dd'))" })

Add-Section 'hosts file'
Add-Out (Select-String -Path "$env:windir\System32\drivers\etc\hosts" -Pattern 'KakaoAdHide' -SimpleMatch | ForEach-Object { "$($_.LineNumber): $($_.Line)" })

Add-Section 'firewall rules'
Add-Out (Get-NetFirewallApplicationFilter -ErrorAction SilentlyContinue | Where-Object { $_.Program -match 'KakaoAdHide' } | ForEach-Object { $r = $_ | Get-NetFirewallRule; "$($r.DisplayName) | $($r.Direction) | $($_.Program)" })

Add-Section 'scheduled tasks'
Add-Out (Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { ($_.TaskName + ($_.Actions | ForEach-Object { $_.Execute })) -match 'KakaoAdHide' } | ForEach-Object { "$($_.TaskPath)$($_.TaskName)" })

[IO.File]::WriteAllLines($out, $lines, (New-Object Text.UTF8Encoding($false)))
"done: $out ($($lines.Count) lines)"
