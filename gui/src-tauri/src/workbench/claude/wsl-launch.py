"""Linux process-group owner for Pipeline's Windows Claude transport."""
import json
import os
import signal
import subprocess
import sys
import threading

with open(sys.argv[1], encoding="utf-8") as source:
    config = json.load(source)
os.setsid()
with open(f"/proc/{os.getpid()}/stat", encoding="utf-8") as source:
    stat = source.read()
identity = {"pid": os.getpid(), "start": stat[stat.rfind(")") + 2:].split()[19]}
with open(sys.argv[2], "x", encoding="utf-8") as target:
    json.dump(identity, target)
    target.flush()
    os.fsync(target.fileno())
environment = {
    "HOME": config["home"],
    "PATH": config["home"] + "/.local/bin:/usr/local/bin:/usr/bin:/bin",
    "LANG": "C.UTF-8",
}
try:
    child = subprocess.Popen([config["executable"], *config["args"]], cwd=config["cwd"], env=environment, stdin=subprocess.PIPE)

    def relay():
        # A closed Windows transport must also stop Linux descendants, including
        # after a Windows application crash. Do not wait for a model reply.
        try:
            while True:
                data = os.read(sys.stdin.fileno(), 65536)
                if not data:
                    break
                child.stdin.write(data)
                child.stdin.flush()
        except (BrokenPipeError, OSError):
            pass
        finally:
            os.killpg(os.getpid(), signal.SIGKILL)

    threading.Thread(target=relay, daemon=True).start()
    child.wait()
finally:
    # The supervisor remains alive until every member of its group is stopped;
    # Windows cleanup verifies this PID's start identity before signalling it.
    os.killpg(os.getpid(), signal.SIGKILL)
