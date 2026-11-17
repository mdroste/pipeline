"""Fixed bounded probes, never arbitrary user commands. JSONL reports observations.

Timeouts are failures of connectivity, not alone proof of OS enforcement.
Compare the positive baseline, identity proxy, host proxy, and offline cases.
"""
import json
import os
import socket
import subprocess
import sys
import time
import ctypes
import threading
import uuid
import inspect
from concurrent.futures import ThreadPoolExecutor

import requests

if sys.argv[1:] == ["--child-request"]:
    print(requests.get("https://example.com", timeout=3).status_code)
    raise SystemExit(0)


def probe(name, fn):
    start = time.monotonic()
    try:
        result = fn()
        print(json.dumps(dict(test=name, result=result, seconds=round(time.monotonic()-start, 3))), flush=True)
    except Exception as exc:
        print(json.dumps(dict(test=name, error=str(exc), seconds=round(time.monotonic()-start, 3))), flush=True)


def request(url, direct=False):
    with requests.Session() as session:
        session.trust_env = not direct
        response = session.get(url, timeout=3, allow_redirects=False)
        return dict(status=response.status_code, bytes=len(response.content), example=b"Example Domain" in response.content)


def child(args, env=None):
    result = subprocess.run(args, env=env, capture_output=True, text=True, timeout=12)
    return dict(code=result.returncode, stdout=result.stdout[:16384], stderr=result.stderr[:2048])


def tcp(address, port, family=socket.AF_INET):
    with socket.socket(family, socket.SOCK_STREAM) as sock:
        if family == socket.AF_INET6:
            sock.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 0)
        sock.settimeout(2)
        sock.connect((address, port))
        return "connected"


def udp():
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.settimeout(2)
        sock.sendto(b"probe", ("127.0.0.1", int(os.environ["PROBE_UDP"])))
        return repr(sock.recvfrom(64))

def udp_public_dns():
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.settimeout(2)
        query = b"\x01\x02\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x07example\x03org\x00\x00\x01\x00\x01"
        sock.sendto(query, ("1.1.1.1", 53))
        return repr(sock.recvfrom(2048))


def dns_wire(name):
    api = ctypes.WinDLL("dnsapi")
    api.DnsQuery_W.argtypes = [ctypes.c_wchar_p, ctypes.c_ushort, ctypes.c_ulong, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p]
    api.DnsQuery_W.restype = ctypes.c_long
    api.DnsRecordListFree.argtypes = [ctypes.c_void_p, ctypes.c_int]
    records = ctypes.c_void_p()
    # WIRE_ONLY | BYPASS_CACHE | NO_HOSTS_FILE. No secret data in the query.
    status = api.DnsQuery_W(name, 1, 0x148, None, ctypes.byref(records), None)
    if records:
        api.DnsRecordListFree(records, 1)
    return dict(name=name, status=status)


def ingress():
    listener = socket.socket()
    listener.bind(("0.0.0.0", 0))
    listener.listen(4)
    with open(os.path.join(os.environ["PROBE_RUN"], "listener.txt"), "w") as ready:
        ready.write(str(listener.getsockname()[1]))
    def accept():
        listener.settimeout(30)
        try:
            while True:
                stream, _ = listener.accept()
                stream.close()
        except TimeoutError:
            pass
        finally:
            listener.close()
    threading.Thread(target=accept, daemon=True).start()
    return "listening"


def intra_container():
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(2)
    listener.settimeout(6)
    port = listener.getsockname()[1]
    def serve():
        try:
            for _ in range(2):
                stream, _ = listener.accept()
                with stream:
                    stream.settimeout(2)
                    stream.recv(1024)
                    stream.sendall(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nself")
        except (TimeoutError, OSError):
            pass
        finally:
            listener.close()
    threading.Thread(target=serve, daemon=True).start()
    probe("intra_container_same_process", lambda: request(f"http://127.0.0.1:{port}", True))
    probe("intra_container_subprocess", lambda: child([sys.executable, "-c", f"import requests; s=requests.Session(); s.trust_env=False; print(s.get('http://127.0.0.1:{port}',timeout=2).status_code)"]))


probe("token_in_subprocess", lambda: child([os.environ["PROBE_EXE"], "identity"]))
intra_container()
if "PROBE_RUN" in os.environ:
    probe("ingress_listener", ingress)
print(json.dumps(dict(python=sys.version, requests=requests.__version__, proxies=requests.utils.get_environ_proxies("https://example.com"))), flush=True)
for scheme in ["http", "https"]:
    for domain in ["example.com", "example.org"]:
        probe(f"requests_{scheme}_{domain}", lambda s=scheme, d=domain: request(f"{s}://{d}"))
probe("python_subprocess_https", lambda: child([sys.executable, "-c", "import requests; r=requests.get('https://example.com', timeout=3); print(r.status_code, 'Example Domain' in r.text)"]))
powershell = os.path.join(os.environ["SystemRoot"], "System32", "WindowsPowerShell", "v1.0", "powershell.exe")
# A PSDrive rooted at the permitted working folder avoids PowerShell 5's
# attempted traversal through inaccessible ancestors. This does not grant ACLs.
ps_bootstrap = "$ErrorActionPreference='Stop'; New-PSDrive -Name Pipeline -PSProvider FileSystem -Root ([Environment]::CurrentDirectory) | Out-Null; Set-Location Pipeline:\\; "
if os.environ.get("PROBE_DRIVE_ALIAS") == "1":
    ps_bootstrap = "$ErrorActionPreference='Stop'; Set-Location -LiteralPath ([Environment]::CurrentDirectory); "
ps_command = ps_bootstrap + "& '"+sys.executable.replace("'", "''")+"' '"+os.path.abspath(__file__).replace("'", "''")+"' --child-request; exit $LASTEXITCODE"
probe("powershell_python_subprocess", lambda: child([powershell, "-NoProfile", "-NonInteractive", "-Command", ps_command]))
ps_web = ps_bootstrap + "[System.Net.WebRequest]::DefaultWebProxy = New-Object System.Net.WebProxy($env:http_proxy); (Invoke-WebRequest -UseBasicParsing -TimeoutSec 5 'https://example.com').StatusCode"
probe("powershell_invoke_webrequest", lambda: child([powershell, "-NoProfile", "-NonInteractive", "-Command", ps_web]))
ps_files = ps_bootstrap + "'shell-file-canary' | Set-Content -LiteralPath './shell-file.txt'; $value = Get-Content -LiteralPath './shell-file.txt'; Remove-Item -LiteralPath './shell-file.txt'; if ($value -ne 'shell-file-canary') {throw 'wrong file content'}; 'ok'"
probe("powershell_relative_files", lambda: child([powershell, "-NoProfile", "-NonInteractive", "-Command", ps_files]))
if "PROBE_FILESYSTEM" in os.environ:
    probe("filesystem_subprocess", lambda: child([sys.executable, os.path.join(os.path.dirname(__file__), "filesystem.py")]))
def concurrent_requests():
    with ThreadPoolExecutor(max_workers=6) as pool:
        return list(pool.map(lambda _: request("https://example.com")["status"], range(6)))
probe("concurrent_requests", concurrent_requests)
if "PROBE_REUSABLE" in os.environ:
    def reuse_channels():
        results = []
        for i in range(48):
            try:
                results.append(request("https://example.com")["status"])
            except Exception as e:
                raise RuntimeError(f"connection {i+1}/48 after {len(results)} successes: {e}") from e
        return results
    probe("reused_channels", reuse_channels)
curl = os.path.join(os.environ["SystemRoot"], "System32", "curl.exe")
for url in ["http://example.com", "https://example.com", "https://example.org"]:
    probe("curl_"+url, lambda url=url: child([curl, "-sS", "-f", "--max-time", "4", url]))
probe("curl_no_proxy", lambda: child([curl, "-sS", "-f", "--noproxy", "*", "--max-time", "3", "https://example.com"]))
probe("requests_trust_env_false", lambda: request("https://example.com", True))
probe("raw_public_v4", lambda: tcp("1.1.1.1", 443))
probe("raw_public_v6", lambda: tcp("2606:4700:4700::1111", 443, socket.AF_INET6))
probe("raw_host_v4", lambda: tcp("127.0.0.1", int(os.environ["PROBE_TCP4"])))
probe("raw_host_v6", lambda: tcp("::1", int(os.environ["PROBE_TCP6"]), socket.AF_INET6))
probe("raw_host_mapped_v6", lambda: tcp("::ffff:127.0.0.1", int(os.environ["PROBE_TCP4"]), socket.AF_INET6))
probe("udp_host_echo", udp)
probe("udp_public_dns", udp_public_dns)
probe("proxy_local_target", lambda: request("http://127.0.0.1:"+os.environ["PROBE_TCP4"]))
probe("proxy_ip_literal", lambda: request("http://1.1.1.1"))
probe("same_identity_alternate_port", lambda: requests.get("https://example.com", proxies={"https":"http://127.0.0.1:"+os.environ["PROBE_ALTERNATE"]}, timeout=3).status_code)
probe("dns_resolution", lambda: repr(socket.getaddrinfo("example.com", 443)))
probe("dns_wire_disallowed_domain", lambda: dns_wire("example.org"))
probe("dns_wire_unique_disallowed_name", lambda: dns_wire("pipeline-probe-"+uuid.uuid4().hex+".example.org"))
probe("dns_wire_allowed_domain", lambda: dns_wire("example.com"))
probe("subprocess_dns", lambda: child([sys.executable, "-c", "import ctypes\n"+inspect.getsource(dns_wire)+"\nprint(dns_wire('example.org')['status'])"]))
if "PROBE_OUTSIDE" in os.environ:
    probe("outside_copy_read", lambda: open(os.environ["PROBE_OUTSIDE"], "rb").read(64).decode())
if "PROBE_RELAY_FAULT" in os.environ:
    marker = os.environ["PROBE_RELAY_FAULT"]
    with open(marker, "w") as ready:
        ready.write("drop proxy")
    deadline = time.monotonic()+15
    while not os.path.exists(marker+".ack") and time.monotonic()<deadline:
        time.sleep(0.025)
    probe("relay_failure_ack", lambda: os.path.exists(marker+".ack"))
    probe("relay_failure_request", lambda: request("https://example.com"))
    probe("relay_failure_dns", lambda: dns_wire("example.org"))
    probe("relay_failure_direct", lambda: tcp("1.1.1.1",443))
if "PROBE_RUN" in os.environ:
    # Keep the listener alive until both reverse probes finish; otherwise a
    # fast workload exit could be misreported as an inbound policy denial.
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        if os.path.exists(os.path.join(os.environ["PROBE_RUN"], "observer.done")):
            break
        time.sleep(0.025)
