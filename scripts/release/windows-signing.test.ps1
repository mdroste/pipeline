# Exercise the signature gate without Azure or a signing certificate.
param([Parameter(Mandatory = $true)][string]$Target)
$ErrorActionPreference = 'Stop'
foreach ($script in @('windows-sign.ps1', 'verify-windows-package.ps1')) {
    $tokens = $null
    $errors = $null
    [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $script), [ref]$tokens, [ref]$errors) | Out-Null
    if ($errors.Count) { throw ($errors | Out-String) }
}
function Get-AuthenticodeSignature { param($LiteralPath) return $global:FixtureSignature }
function Import-Module { throw 'Verification must never import the signing module' }
function Invoke-ArtifactSigning { throw 'Verification must never modify the file' }
foreach ($case in @(
    @{ Status = 'Valid'; Publisher = 'Michael Droste'; Timestamp = $true; Accept = $true },
    @{ Status = 'NotSigned'; Publisher = 'Michael Droste'; Timestamp = $true; Accept = $false },
    @{ Status = 'HashMismatch'; Publisher = 'Michael Droste'; Timestamp = $true; Accept = $false },
    @{ Status = 'NotTrusted'; Publisher = 'Michael Droste'; Timestamp = $true; Accept = $false },
    @{ Status = 'Valid'; Publisher = 'Someone Else'; Timestamp = $true; Accept = $false },
    @{ Status = 'Valid'; Publisher = 'Michael Droste'; Timestamp = $false; Accept = $false }
)) {
    $certificate = [pscustomobject]@{ Publisher = $case.Publisher }
    $certificate | Add-Member -MemberType ScriptMethod -Name GetNameInfo -Value { param($Type, $Issuer) return $this.Publisher }
    $global:FixtureSignature = [pscustomobject]@{
        Status = $case.Status
        SignerCertificate = $certificate
        TimeStamperCertificate = $(if ($case.Timestamp) { $certificate } else { $null })
    }
    $accepted = $true
    try { & (Join-Path $PSScriptRoot 'windows-sign.ps1') -FilePath $Target -VerifyOnly }
    catch { $accepted = $false }
    if ($accepted -ne $case.Accept) { throw "Wrong signature decision: $($case | ConvertTo-Json -Compress)" }
}
