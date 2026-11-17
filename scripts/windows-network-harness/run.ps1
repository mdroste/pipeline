<#
.SYNOPSIS
Runs Windows networking experiments without administrator elevation.
.DESCRIPTION
Use an ordinary Windows terminal. Supply the real Python executable, not the
WindowsApps alias. Install workload dependencies once with:
  & $Python -m pip install --target ./scripts/windows-network-harness/target/python -r ./scripts/windows-network-harness/requirements.txt
The default mode tests the AppContainer peer plus rejected ingress policy and
wrong-peer controls. 'all' adds an unsandboxed baseline, a host proxy, and offline.
The pipe-relay modes stage Python in disposable AppContainer storage and bridge
its local proxy over inherited pipes. They assert networking, token, and shell
compatibility results, and return nonzero on any failed assertion. The legacy
matrix modes are observational: exit 0 means completion, not isolation success.
The relay has 32 reusable channels and checks 48 additional HTTPS connections.
StageFilesystem adds disposable ACL/integrity-label canaries under target; these
are retained for inspection and do not change any original file's permissions.
These checks do not establish equivalence to PSEC's host filesystem policy.
DriveAlias temporarily maps the disposable profile to an unused per-user drive
letter, allowing PowerShell 5 relative file operations without ancestor ACL
changes. An unelevated helper owns the alias and removes it on normal return or
loss of the host's pipe. Concurrent harness allocation is serialized. Killing
both host and helper can still leave an alias. This is a compatibility experiment,
not a filesystem boundary. Run the executable's drive-checks mode for local
concurrency and forced-host-termination checks; it needs no Python argument.
pipe-relay-diagnostics is observational apart from process exit and optional
filesystem assertions. Production integration is deliberately not included.
psec-lifetime is observational and probes native PSEC boundaries, not networking
qualification. The executable's lpac-matrix mode needs no Python argument.
JSONL is written to stdout; lifecycle/build messages go to stderr.
Profile storage is deleted on normal return; a hard host crash
can leave a profile named Pipeline.NetworkProbe.<pid>.<timestamp> behind.
#>
param(
    [Parameter(Mandatory = $true)][string]$Python,
    [ValidateSet('identity', 'host', 'offline', 'baseline', 'all', 'native-loopback', 'pipe-relay', 'pipe-relay-failure', 'pipe-relay-cancel', 'pipe-relay-diagnostics', 'psec-lifetime')][string]$Mode = 'identity',
    [switch]$StageFilesystem,
    [switch]$DriveAlias
)
$ErrorActionPreference = 'Stop'
if ($StageFilesystem -and -not $Mode.StartsWith('pipe-relay')) {
    throw 'StageFilesystem requires a pipe-relay mode.'
}
if ($DriveAlias -and -not $Mode.StartsWith('pipe-relay')) {
    throw 'DriveAlias requires a pipe-relay mode.'
}
$probePython = (Resolve-Path -LiteralPath $Python).Path
$previousStaging = $env:PIPELINE_PROBE_STAGED_FS
$previousAlias = $env:PIPELINE_PROBE_DRIVE_ALIAS
try {
    if ($StageFilesystem) { $env:PIPELINE_PROBE_STAGED_FS = '1' }
    if ($DriveAlias) { $env:PIPELINE_PROBE_DRIVE_ALIAS = '1' }
    & cargo run --locked --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') -- $probePython $Mode
    $probeExitCode = $LASTEXITCODE
} finally {
    $env:PIPELINE_PROBE_STAGED_FS = $previousStaging
    $env:PIPELINE_PROBE_DRIVE_ALIAS = $previousAlias
}
exit $probeExitCode
