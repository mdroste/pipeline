# Tauri's Windows signing hook. Azure login supplies the CLI credential; each
# invocation must verify the publisher and timestamp before bundling continues.
param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [switch]$VerifyOnly
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$target = (Resolve-Path -LiteralPath $FilePath).Path
function Test-PipelineSignature($Signature) {
    return $Signature.Status -eq 'Valid' -and
        $null -ne $Signature.TimeStamperCertificate -and
        $null -ne $Signature.SignerCertificate -and
        $Signature.SignerCertificate.GetNameInfo(
            [System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false
        ) -ceq 'Michael Droste'
}

$signature = Get-AuthenticodeSignature -LiteralPath $target
if (-not (Test-PipelineSignature $signature) -and -not $VerifyOnly) {
    foreach ($name in @('AZURE_SIGNING_ENDPOINT', 'AZURE_SIGNING_ACCOUNT', 'AZURE_SIGNING_PROFILE')) {
        if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) {
            throw "Missing signing setting: $name"
        }
    }
    Import-Module ArtifactSigning -RequiredVersion 0.1.8
    $options = @{
        Endpoint = $env:AZURE_SIGNING_ENDPOINT
        CodeSigningAccountName = $env:AZURE_SIGNING_ACCOUNT
        CertificateProfileName = $env:AZURE_SIGNING_PROFILE
        Files = $target
        FileDigest = 'SHA256'
        TimestampRfc3161 = 'http://timestamp.acs.microsoft.com'
        TimestampDigest = 'SHA256'
        ExcludeEnvironmentCredential = $true
        ExcludeWorkloadIdentityCredential = $true
        ExcludeManagedIdentityCredential = $true
        ExcludeSharedTokenCacheCredential = $true
        ExcludeVisualStudioCredential = $true
        ExcludeVisualStudioCodeCredential = $true
        ExcludeAzureCliCredential = $false
        ExcludeAzurePowerShellCredential = $true
        ExcludeAzureDeveloperCliCredential = $true
        ExcludeInteractiveBrowserCredential = $true
    }
    Invoke-ArtifactSigning @options
    $signature = Get-AuthenticodeSignature -LiteralPath $target
}
if (-not (Test-PipelineSignature $signature)) {
    throw "Expected a valid, timestamped Michael Droste signature: $target (status: $($signature.Status))"
}
Write-Host "Verified signed file: $target"
