"""Observe before/after closing only the environment creation handle."""
import ctypes
import json
import os
from pathlib import Path
import socket
import subprocess
import sys

root = Path(os.environ['PROBE_FILESYSTEM'])

def probe(name, fn):
    try:
        print(json.dumps(dict(test=name, result=fn())), flush=True)
    except Exception as e:
        print(json.dumps(dict(test=name, error=repr(e))), flush=True)

def self_connect():
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        listener.listen()
        with socket.create_connection(listener.getsockname(), timeout=2):
            return 'connected'

def dns():
    api = ctypes.WinDLL('dnsapi')
    api.DnsQuery_W.argtypes = [ctypes.c_wchar_p, ctypes.c_ushort, ctypes.c_ulong, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p]
    records = ctypes.c_void_p()
    status = api.DnsQuery_W('example.org', 1, 0x148, None, ctypes.byref(records), None)
    if records:
        api.DnsRecordListFree.argtypes = [ctypes.c_void_p, ctypes.c_int]
        api.DnsRecordListFree(records, 1)
    return status

def network(address, port):
    with socket.create_connection((address, port), timeout=2):
        return 'connected'

def phase(label):
    p = subprocess.run([os.environ['PROBE_EXE'], 'identity'], capture_output=True, text=True, timeout=5)
    print(json.dumps(dict(test=label + ':token', code=p.returncode, stdout=p.stdout, stderr=p.stderr)), flush=True)
    probe(label + ':self', self_connect)
    probe(label + ':dns', dns)
    probe(label + ':host', lambda: network('127.0.0.1', int(os.environ['PROBE_HOST_TCP'])))
    probe(label + ':public', lambda: network('1.1.1.1', 443))
    for name in ['readonly', 'work/.git', 'work/.secrets', 'outside']:
        probe(label + ':' + name, lambda n=name: (root / n / 'canary.txt').read_text())
    probe(label + ':write', lambda: (root / 'work/new.txt').write_text(label))
    ps = os.path.join(os.environ['SystemRoot'], 'System32/WindowsPowerShell/v1.0/powershell.exe')
    command = "$ErrorActionPreference='Stop'; 'x'|Set-Content -LiteralPath './ps.txt'; Remove-Item -LiteralPath './ps.txt'; 'ok'"
    p = subprocess.run([ps, '-NoProfile', '-NonInteractive', '-Command', command], text=True, capture_output=True, timeout=10)
    print(json.dumps(dict(test=label + ':powershell', code=p.returncode, stdout=p.stdout, stderr=p.stderr)), flush=True)

phase('before')
(root / 'work/ready.txt').write_text('ready')
assert sys.stdin.readline().strip() == 'go'
phase('after')
