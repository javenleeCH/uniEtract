#!/usr/bin/env python3
"""CI gate: dist/ux-extract.zip must equal .agents/skills/ux-extract/.

The zip is currently a hand-packaged release asset; a contributor who edits the
skill source but forgets to repackage would otherwise ship stale skill content
under the new release's SHA256SUMS. This extracts the zip in memory and compares
file-for-file (names and sha256) against the source tree, so drift is a hard
failure.

Run standalone (no shell quoting, no third-party deps) under bash and pwsh.
"""

from pathlib import Path
import hashlib
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / ".agents" / "skills" / "ux-extract"
ZIP = ROOT / "dist" / "ux-extract.zip"
# The zip stores entries under this prefix.
PREFIX = "ux-extract/"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def source_files() -> dict:
    files = {}
    for path in SOURCE.rglob("*"):
        if not path.is_file():
            continue
        rel = path.relative_to(SOURCE).as_posix()
        files[rel] = sha256(path.read_bytes())
    return files


def zip_files() -> dict:
    files = {}
    with zipfile.ZipFile(ZIP) as archive:
        for name in archive.namelist():
            # Skip directory entries (trailing `/`).
            if name.endswith("/"):
                continue
            if not name.startswith(PREFIX):
                return {"__error__": f"unexpected zip entry `{name}`"}
            rel = name[len(PREFIX):]
            files[rel] = sha256(archive.read(name))
    return files


def main():
    if not ZIP.is_file():
        print("missing dist/ux-extract.zip")
        return 1
    if not SOURCE.is_dir():
        print("missing .agents/skills/ux-extract/")
        return 1

    sources = source_files()
    zipped = zip_files()
    if "__error__" in zipped:
        print(f"skill zip check failed: {zipped['__error__']}")
        return 1

    errors = []
    if sources.keys() != zipped.keys():
        errors.append(
            f"file set mismatch: zip has {sorted(zipped)} vs "
            f"source {sorted(sources)}"
        )
    for rel in sources.keys() & zipped.keys():
        if sources[rel] != zipped[rel]:
            errors.append(f"content drift in `{rel}`")

    if errors:
        for error in errors:
            print(f"skill zip drift: {error}")
        print("hint: repackage with the skill packaging script and recommit")
        return 1

    print(f"skill zip OK: {len(sources)} files match .agents/skills/ux-extract/")
    return 0


if __name__ == "__main__":
    sys.exit(main())