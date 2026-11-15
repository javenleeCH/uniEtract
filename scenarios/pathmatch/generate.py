#!/usr/bin/env python3
"""Regenerate the NativeHeap path-matching fixtures.

Run from anywhere:  python scenarios/pathmatch/generate.py

`tests/pathspec_scenario.rs` owns the assertions and the byte-for-byte
expectations; this script is only a convenience writer for the single fixture
tree at `tests/fixtures/pathmatch/data`. Windows directory names cannot
contain ':', so timestamps on disk use '-' in the time part (19-45-49).

FILES is the source of truth for generation:
    rel path -> (content, path_should_match, expected_kb_or_None)

Writes use write_bytes() with explicit LF so checkout normalization and
Windows CRLF translation cannot change the fixtures.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / "tests" / "fixtures" / "pathmatch"
DATA = BASE / "data"

A1 = "data/2026-11-14T19-45-49_SRRG/render_service/30min/hidumper/txt"
A2 = "data/2026-11-15T08-12-03_SRRG/render_service/45min/hidumper/txt"
A3 = "data/2026-11-16T23-59-59_SRRG/render_service/5min/hidumper/txt"


def hit(kb, tag):
    """One hidumper-style report with exactly one `NativeHeap pss: N kB` line."""
    return (
        "----- render_service hidumper meminfo -----\n"
        "                   PSS   Shared  Private   Heap\n"
        "                  (KB)   Dirty    Dirty   Size\n"
        "                ------  ------   ------  ------\n"
        f"  NativeHeap pss: {kb} kB\n"
        "  Dalvik Heap pss: 2048 kB\n"
        "  .so mmap pss: 4096 kB\n"
        f"TOTAL: {kb} kB  tag={tag}\n"
    )


MISS = (
    "----- render_service hidumper meminfo -----\n"
    "nativeheap pss: 999 kB\n"       # wrong case
    "NativeHeap: 111 kB\n"            # missing `pss`
    "NativeHeap pss 888 kB\n"         # missing colon
    "NativeHeap pss: 777 MB\n"        # wrong unit
    "TOTAL: 0 kB\n"
)

# (content, official PathSpec must match, expected extracted kB or None)
FILES = {
    # ---- A: strictly conforming paths -------------------------------------
    f"{A1}/dump1.txt": (hit(12345, "A1"), True, "12345"),
    f"{A2}/dump2.txt": (hit(67890, "A2"), True, "67890"),
    f"{A3}/dump3.txt": (hit(321, "A3"), True, "321"),
    f"{A1}/dump4.txt": (MISS, True, None),  # right path, zero content matches

    # ---- B: decoy paths, every one DOES contain a NativeHeap pss line ------
    # so a wrong path filter immediately shows up as extra output rows.
    "data/2026-11-14T19-45-49_SRRG/render_service/30min/hidumper/raw/dump5.txt":
        (hit(77777, "B1-raw-instead-of-txt"), False, None),
    f"{A1}/summary.csv": (hit(77777, "B2-not-txt-extension"), False, None),
    f"{A1}/nested/dump6.txt": (hit(77777, "B3-one-extra-level"), False, None),
    "data/2026-11-14T19-45-49_SRRG/render_service/xxmin/hidumper/txt/dump7.txt":
        (hit(77777, "B4-non-numeric-min"), False, None),
    "data/2026-11-14T19-45-49_SRRG/render_service/30min/debug/dump8.txt":
        (hit(77777, "B5-debug-instead-of-hidumper"), False, None),
    "data/junk_SRRG/render_service/30min/hidumper/txt/dump9.txt":
        (hit(77777, "B6-no-timestamp"), False, None),
    "data/2026-02-30T10-00-00_SRRG/render_service/30min/hidumper/txt/dump10.txt":
        (hit(77777, "B7-impossible-date"), False, None),
    "data/2026-11-14T19-45-49_SRRG/render_service/-5min/hidumper/txt/dump11.txt":
        (hit(77777, "B8-negative-min"), False, None),
}


def main():
    for rel, (content, _, _) in FILES.items():
        path = (BASE / rel).resolve()
        if not path.is_relative_to(BASE.resolve()):
            raise SystemExit(f"refusing to write outside fixture dir: {path}")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content.encode("utf-8"))

    written = sorted(p.relative_to(DATA).as_posix() for p in DATA.rglob("*") if p.is_file())
    print(f"generated {len(FILES)} logical entries -> {len(written)} files under {DATA}")
    for rel in written:
        print("  " + rel)


if __name__ == "__main__":
    main()
