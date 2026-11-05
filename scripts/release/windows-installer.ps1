# NSIS removes forward slashes in $INSTDIR instead of treating them as path
# separators. /D must be the final, unquoted argument, including for spaces.
# https://nsis.sourceforge.io/Docs/Chapter3.html#installerusage
function Get-NsisInstallPlan {
    param([Parameter(Mandatory = $true)][string]$Directory)
    $normalized = $Directory.Replace('/', '\')
    if ($normalized -notmatch '^[A-Za-z]:\\' -or $normalized.Substring(3) -match '[\x00-\x1f:*?"<>|]') {
        throw 'NSIS verification requires an absolute Windows drive path without invalid filename characters'
    }
    return @{
        Directory = $normalized
        Arguments = @('/S', "/D=$normalized")
    }
}
