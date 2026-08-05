#!/usr/bin/env python3
import json, subprocess, sys

def cmux(args):
    r = subprocess.run(["cmux"] + args, capture_output=True, text=True, timeout=5)
    if r.returncode != 0:
        return None
    return r.stdout

windows_raw = cmux(["list-windows", "--json"])
if not windows_raw:
    print(json.dumps({"workspaces": [], "notifications": []}))
    sys.exit(0)

windows = json.loads(windows_raw)
all_workspaces = []
for win in windows:
    raw = cmux(["workspace", "list", "--json", "--id-format", "both", "--window", win["id"]])
    if raw:
        data = json.loads(raw)
        all_workspaces.extend(data.get("workspaces", []))

notifs_raw = cmux(["list-notifications", "--json"])
notifications = json.loads(notifs_raw) if notifs_raw else []

print(json.dumps({"workspaces": all_workspaces, "notifications": notifications}))
