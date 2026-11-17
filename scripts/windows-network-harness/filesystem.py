"""Read/write/deny controls against new host-side canaries only."""
import json
import os
import ctypes
import subprocess
import sys
from pathlib import Path

root = Path(os.environ["PROBE_FILESYSTEM"])
failures = []
def check(name, expected, fn):
    try:
        value = fn()
        ok = expected == "allow"
        row = dict(test=name, result=str(value), ok=ok)
    except PermissionError as e:
        ok = expected == "deny"
        row = dict(test=name, denied=e.winerror, ok=ok)
    except Exception as e:
        ok = False
        row = dict(test=name, error=repr(e), ok=ok)
    print(json.dumps(row), flush=True)
    if not ok:
        failures.append(name)

for folder, read, write in [("readonly", "allow", "deny"), ("work", "allow", "allow"), ("work/.git", "allow", "deny"), ("work/.secrets", "deny", "deny"), ("outside", "deny", "deny")]:
    path = root / folder / "canary.txt"
    check(folder + ":read", read, path.read_text)
    check(folder + ":write", write, lambda p=path: p.write_text("changed-canary"))
    check(folder + ":new", write, lambda p=path.parent: (p / "new.txt").write_text("new"))
    check(folder + ":delete", write, path.unlink)

def access_handle(path, access):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateFileW.argtypes = [ctypes.c_wchar_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_void_p]
    kernel.CreateFileW.restype = ctypes.c_void_p
    kernel.CloseHandle.argtypes = [ctypes.c_void_p]
    handle = kernel.CreateFileW(str(path), access, 7, None, 3, 0x02000000, None)
    if handle == ctypes.c_void_p(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    kernel.CloseHandle(handle)
    return "opened"

for name in ["readonly", "work", "work/.git", "work/.secrets"]:
    for access, label in [(0x40000, "write_dacl"), (0x80000, "write_owner"), (0x40, "delete_child")]:
        check(name + ":" + label, "deny", lambda n=name, a=access: access_handle(root / n, a))
for name in ["readonly", "work/.git", "work/.secrets"]:
    check(name + ":rename_dir", "deny", lambda n=name: (root / n).rename(root / "work" / (Path(n).name + "-moved")))
    check(name + ":rename_file", "deny", lambda n=name: (root / n / "canary.txt").rename(root / "work" / (Path(n).name + "-moved.txt")))
check("readonly:child_read", "allow", lambda: subprocess.check_output([sys.executable, "-c", "from pathlib import Path; import sys; assert Path(sys.argv[1]).read_text() == 'original-canary'; print('ok')", str(root / "readonly/canary.txt")], text=True, timeout=10))
print(json.dumps(dict(filesystem_assertion_failures=failures)), flush=True)
raise SystemExit(bool(failures))
