"""Bounded, observational shell diagnostics inside the same networkless token."""
import json
import os
import subprocess
import sys

if "PROBE_TLS_DIAGNOSTICS" in os.environ:
    ps = os.path.join(os.environ["SystemRoot"], "System32", "WindowsPowerShell", "v1.0", "powershell.exe")
    curl = os.path.join(os.environ["SystemRoot"], "System32", "curl.exe")
    command = "$ErrorActionPreference='Stop'; Set-Location -LiteralPath ([Environment]::CurrentDirectory); [System.Net.WebRequest]::DefaultWebProxy=New-Object System.Net.WebProxy($env:http_proxy); (Invoke-WebRequest -UseBasicParsing -TimeoutSec 5 'https://example.com').StatusCode"
    for name, args in [("python", [sys.executable, "workload.py", "--child-request"]), ("curl", [curl, "-v", "--max-time", "5", "https://example.com"]), ("powershell", [ps, "-NoProfile", "-NonInteractive", "-Command", command])]:
        result = subprocess.run(args, capture_output=True, text=True, timeout=15)
        print(json.dumps(dict(test=name, code=result.returncode, stdout=result.stdout[:2048], stderr=result.stderr[:6000])), flush=True)
    raise SystemExit(0)  # Observations above, not a qualification pass.

if "PROBE_LPAC_WORKLOAD" in os.environ:
    result = subprocess.run([sys.executable, "workload.py", "--child-request"], capture_output=True, text=True, timeout=10)
    print(json.dumps(dict(test="lpac_client_https", code=result.returncode, stdout=result.stdout, stderr=result.stderr)), flush=True)
    raise SystemExit(result.returncode)

if "PROBE_FILESYSTEM" in os.environ:
    result = subprocess.run([sys.executable, "filesystem.py"])
    raise SystemExit(result.returncode)

ps = os.path.join(os.environ["SystemRoot"], "System32", "WindowsPowerShell", "v1.0", "powershell.exe")
invoke = "& '{}' '{}' --child-request; exit $LASTEXITCODE".format(sys.executable, os.path.join(os.getcwd(), "workload.py"))
cases = {
    "providers": "$ErrorActionPreference='Continue'; Get-PSProvider | Format-List *; Get-PSDrive | Format-List *; $Error | Format-List * -Force; [System.Environment]::CurrentDirectory; $pwd; $env:PATHEXT",
    "dotnet": "[IO.Directory]::Exists('C:\\'); [IO.Directory]::Exists($env:USERPROFILE); [IO.File]::Exists('" + sys.executable + "'); [IO.Directory]::GetLogicalDrives()",
    "set_location": "$ErrorActionPreference='Stop'; Set-Location -LiteralPath $env:USERPROFILE; " + invoke,
    "new_drive": "$ErrorActionPreference='Stop'; New-PSDrive -Name Probe -PSProvider FileSystem -Root $env:USERPROFILE | Out-Null; Set-Location Probe:\\; " + invoke,
    "new_c_drive": "$ErrorActionPreference='Stop'; New-PSDrive -Name C -PSProvider FileSystem -Root 'C:\\' | Out-Null; Set-Location -LiteralPath $env:USERPROFILE; " + invoke,
    "remove_trace": "$ErrorActionPreference='Stop'; New-PSDrive -Name Probe -PSProvider FileSystem -Root $env:USERPROFILE | Out-Null; Set-Location Probe:\\; 'x' | Set-Content -LiteralPath './remove-probe.txt'; try { Remove-Item -LiteralPath './remove-probe.txt' } catch { $_.Exception.ToString(); $_.ScriptStackTrace }; [IO.File]::Delete($env:USERPROFILE + '\\remove-probe.txt'); 'dotnet-delete-ok'",
}
for label, target in [("bare", "'remove-probe.txt'"), ("absolute", "($env:USERPROFILE + '\\remove-probe.txt')"), ("psdrive", "'Probe:\\remove-probe.txt'"), ("dot_force", "'./remove-probe.txt' -Force")]:
    cases["remove_" + label] = "$ErrorActionPreference='Stop'; New-PSDrive -Name Probe -PSProvider FileSystem -Root $env:USERPROFILE | Out-Null; Set-Location Probe:\\; 'x' | Set-Content -LiteralPath 'remove-probe.txt'; Remove-Item -LiteralPath " + target + "; 'ok'"
cases["drive_current_location"] = "$ErrorActionPreference='Stop'; $p=[Environment]::CurrentDirectory; (Get-PSDrive -Name $p.Substring(0,1)).CurrentLocation=$p.Substring(3); Set-Location ($p.Substring(0,2)); 'x' | Set-Content -LiteralPath './remove-probe.txt'; Remove-Item -LiteralPath './remove-probe.txt'; " + invoke
for name, command in cases.items():
    try:
        p = subprocess.run([ps, "-NoProfile", "-NonInteractive", "-Command", command], capture_output=True, text=True, timeout=15)
        print(json.dumps(dict(test=name, code=p.returncode, stdout=p.stdout[:14000], stderr=p.stderr[:6000])), flush=True)
    except Exception as e:
        print(json.dumps(dict(test=name, error=str(e))), flush=True)
