# Verify the actual installed payload: Tauri restores its unsigned build-tree
# GUI binary after bundling, so inspecting target/release alone is insufficient.
param([Parameter(Mandatory = $true)][string]$InstallDirectory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'windows-installer.ps1')
$installPlan = Get-NsisInstallPlan -Directory $InstallDirectory
$InstallDirectory = $installPlan.Directory
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$bundle = Join-Path $root 'gui/src-tauri/target/release/bundle/nsis'
$installers = @(Get-ChildItem -LiteralPath $bundle -Filter '*setup.exe' -File)
if ($installers.Count -ne 1) { throw 'Expected exactly one NSIS installer' }
if (Test-Path -LiteralPath $InstallDirectory) { throw 'Installation check needs a fresh directory' }
$verify = Join-Path $PSScriptRoot 'windows-sign.ps1'
& $verify -FilePath $installers[0].FullName -VerifyOnly
if (-not $?) { throw 'Installer signature verification failed' }

$installer = Start-Process -FilePath $installers[0].FullName -ArgumentList $installPlan.Arguments -Wait -PassThru
if ($installer.ExitCode -ne 0) { throw "Installer failed: $($installer.ExitCode)" }
if (-not (Test-Path -LiteralPath $InstallDirectory -PathType Container)) {
    throw "Installer returned success but did not create the requested directory: $InstallDirectory"
}
Write-Host "Installed package in: $InstallDirectory"
foreach ($name in @('pipeline-gui.exe', 'pipeline-cli.exe', 'uninstall.exe')) {
    & $verify -FilePath (Join-Path $InstallDirectory $name) -VerifyOnly
    if (-not $?) { throw "Installed signature verification failed: $name" }
}
$expectedVersion = (Get-Content -LiteralPath (Join-Path $root 'gui/package.json') -Raw | ConvertFrom-Json).version
$guiVersion = (Get-Item -LiteralPath (Join-Path $InstallDirectory 'pipeline-gui.exe')).VersionInfo.ProductVersion
if ($guiVersion -notin @($expectedVersion, "$expectedVersion.0")) {
    throw "Installed GUI version $guiVersion does not match $expectedVersion"
}
$cliVersion = & (Join-Path $InstallDirectory 'pipeline-cli.exe') --version
if ($LASTEXITCODE -ne 0 -or "$cliVersion".Trim() -cne "pipeline-cli $expectedVersion") {
    throw "Installed CLI version does not match $expectedVersion : $cliVersion"
}
Write-Host "Verified installed GUI and CLI version: $expectedVersion"
node (Join-Path $PSScriptRoot 'verify-cli-launcher.mjs') (Join-Path $InstallDirectory 'pipeline-cli.exe')
if ($LASTEXITCODE -ne 0) { throw 'Installed command launcher qualification failed' }
$inventories = @(Get-ChildItem -LiteralPath $InstallDirectory -Filter 'BUNDLE_INVENTORY.json' -Recurse -File)
if ($inventories.Count -ne 1) { throw 'Expected one installed Poppler inventory' }
$poppler = $inventories[0].DirectoryName
foreach ($name in @('pdftotext.exe', 'pdftoppm.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $poppler "Library/bin/$name") -PathType Leaf)) {
        throw "Missing installed Poppler tool: $name"
    }
}
Get-ChildItem -LiteralPath $poppler -Recurse -File |
    Where-Object { $_.Extension -in @('.exe', '.dll') } |
    ForEach-Object {
        & $verify -FilePath $_.FullName -VerifyOnly
        if (-not $?) { throw "Installed resource signature verification failed: $($_.FullName)" }
    }
node (Join-Path $PSScriptRoot 'poppler-inventory.mjs') --verify $poppler
if ($LASTEXITCODE -ne 0) { throw 'Installed Poppler inventory verification failed' }
node (Join-Path $PSScriptRoot 'smoke-poppler.mjs') $poppler
if ($LASTEXITCODE -ne 0) { throw 'Installed Poppler tools failed to read a PDF' }
node (Join-Path $PSScriptRoot 'qualify-poppler.mjs') $poppler $poppler
if ($LASTEXITCODE -ne 0) { throw 'Installed Poppler PDF corpus failed' }
$llama = Join-Path (Split-Path -Parent $poppler) 'llama-vulkan'
if (Test-Path -LiteralPath $llama) { throw 'Optional GPU runtime must not be embedded in the desktop installer' }
