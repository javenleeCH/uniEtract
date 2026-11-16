#!/usr/bin/env python3
"""Create one release archive per platform and aggregate their checksums.

Pure Python stdlib: the same script runs unchanged on Linux, Windows and macOS
runners. `--collect` recomputes every archive hash and writes the SHA256SUMS
asset published with the GitHub Release.
"""

from pathlib import Path
import argparse
import hashlib
import os
import shutil
import sys
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
PAYLOAD = ("README.md", "LICENSE", "CHANGELOG.md")
DOC_FILES = ("cli-design.md", "path-spec-design.md")


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def output_dir(raw):
    path = Path(raw)
    return path if path.is_absolute() else ROOT / path


def files_under(root):
    return sorted(path for path in root.rglob("*") if path.is_file())


def package(args):
    if not all((args.version, args.target, args.binary, args.format)):
        print("package mode needs --version, --target, --binary and --format")
        return 1

    binary = Path(args.binary)
    if not binary.is_file():
        print(f"missing binary: {binary}")
        return 1

    stem = f"ux-{args.version}-{args.target}"
    dest = output_dir(args.output_dir)
    dest.mkdir(parents=True, exist_ok=True)
    stage = dest / f".{stem}-stage"
    shutil.rmtree(stage, ignore_errors=True)
    stage.mkdir()

    shutil.copy2(binary, stage / binary.name)
    for relative in PAYLOAD:
        shutil.copy2(ROOT / relative, stage / relative)
    docs = stage / "docs"
    docs.mkdir()
    for name in DOC_FILES:
        shutil.copy2(ROOT / "docs" / name, docs / name)

    suffix = "zip" if args.format == "zip" else "tar.gz"
    archive = dest / f"{stem}.{suffix}"
    if archive.exists():
        archive.unlink()

    if args.format == "zip":
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
            for path in files_under(stage):
                name = (Path(stem) / path.relative_to(stage)).as_posix()
                bundle.write(path, name)
    else:
        with tarfile.open(archive, "w:gz") as bundle:
            for path in files_under(stage):
                bundle.add(
                    path,
                    arcname=(Path(stem) / path.relative_to(stage)).as_posix(),
                    recursive=False,
                )

    shutil.rmtree(stage)
    digest = sha256(archive)
    (dest / f"{archive.name}.sha256").write_text(
        f"{digest}  {archive.name}\n", encoding="utf-8"
    )
    print(f"packaged {archive.name}: {digest}")
    return 0


def collect(args):
    dest = output_dir(args.output_dir)
    sums = []
    for checksum_file in sorted(dest.glob("*.sha256")):
        expected_name = checksum_file.name[: -len(".sha256")]
        for line in checksum_file.read_text(encoding="utf-8").splitlines():
            fields = line.split()
            if not fields:
                continue
            if len(fields) != 2:
                print(f"malformed checksum line in {checksum_file.name}: {line!r}")
                return 1
            digest, name = fields
            if name != expected_name:
                print(f"{checksum_file.name} points at {name!r}, expected {expected_name!r}")
                return 1
            archive = dest / name
            if not archive.is_file():
                print(f"missing archive: {name}")
                return 1
            actual = sha256(archive)
            if actual != digest:
                print(f"checksum mismatch for {name}: {actual} != {digest}")
                return 1
            sums.append((name, digest))

    if not sums:
        print("no *.sha256 files found; nothing to publish")
        return 1

    staged = set()
    for raw in args.extra_file or []:
        source = Path(raw)
        if not source.is_file():
            print(f"missing extra file: {raw}")
            return 1
        if source.name in staged or any(name == source.name for name, _ in sums):
            print(f"extra file already checksummed: {source.name}")
            return 1
        staged_file = dest / source.name
        shutil.copy2(source, staged_file)
        sums.append((source.name, sha256(staged_file)))
        staged.add(source.name)

    targets = [
        item.strip()
        for item in os.environ.get("RELEASE_TARGETS", "").split(",")
        if item.strip()
    ]
    if args.version and targets:
        names = {name for name, _ in sums}
        allowed = {
            f"ux-{args.version}-{target}.{suffix}"
            for target in targets
            for suffix in ("zip", "tar.gz")
        }
        for target in targets:
            matches = [
                name for name in names if name.startswith(f"ux-{args.version}-{target}.")
            ]
            if len(matches) != 1:
                print(f"expected exactly one archive for {target}, got {matches}")
                return 1
        unexpected = names - allowed - staged
        if unexpected:
            print(f"unexpected archives: {sorted(unexpected)}")
            return 1
        expected_count = len(targets) + len(staged)
        if len(sums) != expected_count:
            print(f"expected {expected_count} archives, got {len(sums)}")
            return 1

    (dest / "SHA256SUMS").write_text(
        "".join(f"{digest}  {name}\n" for name, digest in sorted(sums)),
        encoding="utf-8",
    )
    print(f"SHA256SUMS OK: {len(sums)} archives")
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--version")
    parser.add_argument("--target")
    parser.add_argument("--binary")
    parser.add_argument("--format", choices=("zip", "tar.gz"))
    parser.add_argument("--output-dir", default="dist/release")
    parser.add_argument("--extra-file", action="append", help="stage an extra file (e.g. dist/ux-extract.zip) into the release with its checksum")
    parser.add_argument("--collect", action="store_true")
    args = parser.parse_args()
    return collect(args) if args.collect else package(args)


if __name__ == "__main__":
    sys.exit(main())
