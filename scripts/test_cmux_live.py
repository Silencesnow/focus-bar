#!/usr/bin/env python3
"""Read-only cmux integration doctor for Focus Bar.

Use --jump workspace:<n> only when intentionally testing workspace selection.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


STANDARD_CLI_PATHS = [
    Path("/Applications/cmux.app/Contents/Resources/bin/cmux"),
    Path.home() / "Applications/cmux.app/Contents/Resources/bin/cmux",
]


def resolve_cli() -> Path | None:
    configured = os.environ.get("CMUX_BUNDLED_CLI_PATH")
    candidates = [Path(configured)] if configured else []
    found = shutil.which("cmux")
    if found:
        candidates.append(Path(found))
    candidates.extend(STANDARD_CLI_PATHS)
    return next((candidate for candidate in candidates if candidate.is_file() and os.access(candidate, os.X_OK)), None)


def resolve_socket() -> str | None:
    configured = os.environ.get("CMUX_SOCKET_PATH")
    if configured:
        return configured
    marker = Path.home() / ".local/state/cmux/last-socket-path"
    if marker.is_file():
        value = marker.read_text().strip()
        if value:
            return value
    return None


def classify_error(detail: str) -> str:
    lowered = detail.lower()
    if any(value in lowered for value in ("broken pipe", "operation not permitted", "permission denied", "unauthenticated")):
        return "ACCESS_DENIED"
    if any(value in lowered for value in ("no such file", "failed to connect", "connection refused")):
        return "CMUX_NOT_RUNNING"
    if "timed out" in lowered:
        return "TIMEOUT"
    return "CMUX_NOT_RUNNING"


class CmuxDoctor:
    def __init__(self, cli: Path, socket_path: str | None):
        self.cli = cli
        self.env = os.environ.copy()
        if socket_path:
            self.env["CMUX_SOCKET_PATH"] = socket_path

    def run(self, *args: str, timeout: int = 5) -> str:
        try:
            result = subprocess.run(
                [str(self.cli), *args],
                capture_output=True,
                text=True,
                timeout=timeout,
                env=self.env,
            )
        except subprocess.TimeoutExpired as error:
            raise RuntimeError("TIMEOUT: cmux command timed out") from error
        if result.returncode != 0:
            detail = result.stderr.strip() or result.stdout.strip()
            raise RuntimeError(f"{classify_error(detail)}: {detail}")
        return result.stdout

    def json(self, *args: str):
        raw = self.run(*args)
        try:
            return json.loads(raw)
        except json.JSONDecodeError as error:
            raise RuntimeError(f"INVALID_RESPONSE: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--jump", metavar="WORKSPACE_REF", help="intentionally select one workspace")
    args = parser.parse_args()

    cli = resolve_cli()
    if not cli:
        print("FAIL CLI_NOT_FOUND: cmux CLI was not found")
        return 1
    socket_path = resolve_socket()
    print(f"PASS cli: {cli}")
    print(f"INFO socket: {socket_path or 'cmux auto-discovery'}")

    doctor = CmuxDoctor(cli, socket_path)
    try:
        pong = doctor.run("ping").strip()
        print(f"PASS ping: {pong}")
        windows = doctor.json("list-windows", "--json")
        if not isinstance(windows, list):
            raise RuntimeError("INVALID_RESPONSE: list-windows was not an array")
        workspaces = []
        for window in windows:
            payload = doctor.json(
                "workspace", "list", "--json", "--id-format", "both", "--window", window["id"]
            )
            workspaces.extend(payload.get("workspaces", []))
        notifications = doctor.json("list-notifications", "--json")
        if not isinstance(notifications, list):
            raise RuntimeError("INVALID_RESPONSE: list-notifications was not an array")
        print(f"PASS snapshot: {len(windows)} windows, {len(workspaces)} workspaces, {len(notifications)} notifications")
        if args.jump:
            doctor.run("workspace", "select", args.jump)
            print(f"PASS jump: selected {args.jump}; notifications were not modified")
    except RuntimeError as error:
        print(f"FAIL {error}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
