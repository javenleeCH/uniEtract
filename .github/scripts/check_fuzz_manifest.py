#!/usr/bin/env python3
"""Verify the fuzz workspace resolves under --locked without shell quoting.

Replaces the earlier `python -c '...'` inline: identical behaviour under bash
and pwsh, and a failure prints the real cargo stderr instead of a traceback.
"""

import subprocess
import sys


def main():
    run = subprocess.run(
        [
            "cargo",
            "metadata",
            "--manifest-path",
            "fuzz/Cargo.toml",
            "--locked",
            "--format-version",
            "1",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )
    if run.returncode != 0:
        print(run.stderr[-2000:])
        return run.returncode
    print("fuzz manifest resolves under --locked: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
