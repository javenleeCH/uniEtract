#!/usr/bin/env python3
"""Assert the libtest inventory size matches the documented contract count.

Uses `-- --list` so no test body executes. `cargo` is invoked through argv
(no shell quoting), so this behaves identically under bash and pwsh. Keep
EXPECTED in sync with README / CHANGELOG / the two design documents.
"""

import subprocess
import sys

EXPECTED = 157


def main():
    run = subprocess.run(
        ["cargo", "test", "--all-targets", "--locked", "--", "--list"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if run.returncode != 0:
        print(run.stderr[-2000:])
        return run.returncode
    names = [line for line in run.stdout.splitlines() if line.endswith(": test")]
    if len(names) != EXPECTED:
        print(f"test inventory: {len(names)} (expected {EXPECTED})")
        return 1
    print(f"test inventory OK: {len(names)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
