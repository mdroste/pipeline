param([Parameter(Mandatory = $true)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$version = (Get-Content -LiteralPath (Join-Path $root 'gui/package.json') -Raw | ConvertFrom-Json).version
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$OutputDirectory = (Resolve-Path -LiteralPath $OutputDirectory).Path
$stage = Join-Path $OutputDirectory 'gpu-runtime-stage'
$archive = Join-Path $OutputDirectory "Pipeline-$version-GPU-Windows-x64.zip"
$manifest = Join-Path $OutputDirectory "Pipeline-$version-GPU-Windows-x64.json"
$source = Join-Path $root 'gui/src-tauri/resources/llama-vulkan'
# Fail closed before packaging: every executable retains Windows trust even
# though it will be downloaded outside the desktop installer.
Get-ChildItem -LiteralPath $source -Recurse -File |
    Where-Object { $_.Extension -in @('.exe', '.dll') } |
    ForEach-Object {
        & (Join-Path $PSScriptRoot 'windows-sign.ps1') -FilePath $_.FullName -VerifyOnly
        if (-not $?) { throw "GPU runtime signature verification failed: $($_.FullName)" }
    }
node (Join-Path $PSScriptRoot 'gpu-runtime.mjs') prepare $source $stage
if ($LASTEXITCODE -ne 0) { throw 'Cannot prepare GPU runtime inventory' }
Compress-Archive -LiteralPath (Join-Path $stage 'runtime'), (Join-Path $stage 'integrity.json') -DestinationPath $archive
node (Join-Path $PSScriptRoot 'gpu-runtime.mjs') manifest $archive (Join-Path $stage 'integrity.json') $manifest
if ($LASTEXITCODE -ne 0) { throw 'Cannot prepare GPU runtime manifest' }
Push-Location (Join-Path $root 'gui')
try {
    npm run tauri -- signer sign --app-version $version $manifest
    if ($LASTEXITCODE -ne 0) { throw 'Cannot sign GPU runtime manifest' }
} finally { Pop-Location }
node (Join-Path $PSScriptRoot 'gpu-runtime.mjs') verify $archive $manifest "$manifest.sig"
if ($LASTEXITCODE -ne 0) { throw 'Cannot verify GPU runtime package' }
# Exercise the archive layout and extracted DLLs that users actually download.
$unpacked = Join-Path $OutputDirectory 'gpu-runtime-check'
Expand-Archive -LiteralPath $archive -DestinationPath $unpacked
& (Join-Path $unpacked 'runtime/llama-server.exe') --version
if ($LASTEXITCODE -ne 0) { throw 'Packaged GPU runtime failed to start' }
