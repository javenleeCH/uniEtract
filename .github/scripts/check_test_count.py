#!/usr/bin/env python3
"""Assert the libtest inventory size matches the documented contract count.

Uses `-- --list` so no test body executes. `cargo` is invoked through argv
(no shell quoting), so this behaves identically under bash and pwsh. Keep
EXPECTED in sync with README / CHANGELOG / the design documents under docs/.
"""

import re
import subprocess
import sys

EXPECTED = 167

# libtest prints one summary line per test binary, e.g. "26 tests, 0 benchmarks".
SUMMARY = re.compile(r"(\d+)\s+tests?,\s+\d+\s+benchmarks?")


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

    summaries = [int(match.group(1)) for match in SUMMARY.finditer(run.stdout)]
    if summaries:
        count = sum(summaries)
    else:
        # Fall back to the per-test listing if a future libtest drops summaries.
        count = sum(
            1 for line in run.stdout.splitlines() if line.rstrip().endswith(": test")
        )

    if count != EXPECTED:
        print(f"test inventory: {count} (expected {EXPECTED})")
        return 1
    print(f"test inventory OK: {count}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
