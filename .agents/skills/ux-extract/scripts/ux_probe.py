#!/usr/bin/env python3
"""Deterministic preflight for the ux CLI.

Prints a single JSON report and exits 0 only when the binary is usable:
  - resolves the executable (PATH first, then a local cargo build)
  - reads --version and checks the long-help sections
  - runs one tiny JSONL extraction and asserts stdout/stderr separation

No user data is read. Invoke without a shell; all subprocess calls use argv.
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path

HELP_SECTIONS = ("STAGES", "AGENT CONTRACT", "PATH SPEC", "ROADMAP", "EXAMPLES")


def resolve_ux():
    found = shutil.which("ux")
    if found:
        return found
    rel_paths = (
        "target/release/ux.exe",
        "target/release/ux",
        "target/debug/ux.exe",
        "target/debug/ux",
    )
    # Search the working directory first, then walk up from this script so a
    # project-scoped skill also resolves a locally built binary.
    roots = [Path.cwd()]
    roots.extend(Path(__file__).resolve().parents[:6])
    seen = set()
    for root in roots:
        for rel in rel_paths:
            candidate = root / rel
            if candidate in seen:
                continue
            seen.add(candidate)
            if candidate.is_file():
                return str(candidate.resolve())
    return None


def run(exe, args, data=b""):
    completed = subprocess.run(
        [exe, *args],
        input=data,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return (
        completed.returncode,
        completed.stdout.decode("utf-8", "replace"),
        completed.stderr.decode("utf-8", "replace"),
    )


def main():
    report = {"ok": False, "executable": None}
    exe = resolve_ux()
    if not exe:
        report["error"] = (
            "ux not found on PATH or under target/release|debug; "
            "install with cargo install --path . or build --release"
        )
        print(json.dumps(report, ensure_ascii=False))
        return 1
    report["executable"] = exe

    code, version, err = run(exe, ["--version"])
    if code != 0:
        report["error"] = f"--version failed (rc={code}): {err.strip()}"
        print(json.dumps(report, ensure_ascii=False))
        return 1
    report["version"] = version.strip()

    code, help_text, err = run(exe, ["--help"])
    if code != 0:
        report["error"] = f"--help failed (rc={code}): {err.strip()}"
        print(json.dumps(report, ensure_ascii=False))
        return 1
    report["help_sections"] = {
        section: section in help_text for section in HELP_SECTIONS
    }
    if not all(report["help_sections"].values()):
        report["error"] = "long help is missing expected sections; binary may be outdated"
        print(json.dumps(report, ensure_ascii=False))
        return 1

    code, stdout, stderr = run(
        exe,
        ["--error-format", "json", "-f", "jsonl", "-O", "text", "-e", "jq(.id)"],
        b'{"id":1}\n',
    )
    report["contract"] = {
        "exit_code": code,
        "stdout": stdout,
        "stderr": stderr,
    }
    if code != 0 or stdout != "1\n" or stderr != "":
        report["error"] = "minimal stdout/stderr contract check failed"
        print(json.dumps(report, ensure_ascii=False))
        return 1

    report["ok"] = True
    print(json.dumps(report, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
