"""Fixed descendant for the owning native job's cancellation test."""
import json
import os
import subprocess
import sys
import time
import requests

descendant = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(90)"])
detached = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(90)"], creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP)
response = requests.get("https://example.com", timeout=3)
with open("lifecycle.json", "w") as marker:
    json.dump(dict(workload=os.getpid(), descendant=descendant.pid, detached=detached.pid, http=response.status_code), marker)
time.sleep(90)
