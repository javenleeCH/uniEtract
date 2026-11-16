#!/usr/bin/env python3
"""Install the ux-extract skill into an agent's skills directory.

Run from the unpacked skill folder, or install directly from the zip:

  python ux-extract/scripts/install.py                    # user scope (default)
  python ux-extract/scripts/install.py --project         # current project
  python ux-extract/scripts/install.py --target DIR      # any agent
  python ux-extract/scripts/install.py --from-zip FILE   # install from a zip
  python ux-extract/scripts/install.py --force           # replace an existing copy
  python ux-extract/scripts/install.py --uninstall       # remove the installed copy

No third-party dependencies, no network access, no config file edits.
Requires Python 3.8+. Exit 0 on success, 1 on failure.
"""

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

SKILL_NAME = "ux-extract"


def emit(message=""):
    """Stay quiet when a downstream pipe (e.g. Select-Object -First N) closes."""
    try:
        print(message)
    except (BrokenPipeError, OSError):
        pass


def skill_source():
    """The skill root that contains this script's scripts/ directory."""
    return Path(__file__).resolve().parents[1]


def validate(src):
    skill_md = src / "SKILL.md"
    if not skill_md.is_file():
        return False, f"SKILL.md not found in {src}"
    text = skill_md.read_text(encoding="utf-8", errors="replace")
    match = re.search(r"^name:\s*(\S+)\s*$", text, re.MULTILINE)
    if not match or match.group(1) != SKILL_NAME:
        return False, f"SKILL.md name does not match {SKILL_NAME}"
    if not (src / "scripts" / "ux_probe.py").is_file():
        return False, "scripts/ux_probe.py is missing from the package"
    return True, "ok"


def safe_extract(zip_path, dest):
    """Reject path traversal before extracting anything."""
    with zipfile.ZipFile(zip_path) as archive:
        for name in archive.namelist():
            parts = PurePosixPath(name.replace("\\", "/")).parts
            if (
                ".." in parts
                or name.startswith(("/", "\\"))
                or (parts and ":" in parts[0])
            ):
                return f"unsafe path in zip: {name}"
        archive.extractall(dest)
    return None


def source_from_args(args, temp_dir):
    if not args.from_zip:
        return skill_source(), None
    zip_path = Path(args.from_zip).expanduser().resolve()
    if not zip_path.is_file():
        return None, f"zip not found: {zip_path}"
    dest = Path(temp_dir) / "unpacked"
    error = safe_extract(zip_path, dest)
    if error:
        return None, error
    candidate = dest / SKILL_NAME
    if candidate.is_dir():
        return candidate, None
    manuals = list(dest.rglob("SKILL.md"))
    if not manuals:
        return None, "no SKILL.md inside the zip"
    return manuals[0].parent, None


def resolve_target(args):
    if args.target:
        return Path(args.target).expanduser().resolve()
    if args.project:
        return Path.cwd() / ".agents" / "skills"
    return Path.home() / ".agents" / "skills"


def main():
    parser = argparse.ArgumentParser(
        description=f"Install the {SKILL_NAME} skill into an agent's skills directory."
    )
    scope = parser.add_mutually_exclusive_group()
    scope.add_argument(
        "--user",
        action="store_true",
        help="install under ~/.agents/skills (default, cross-agent convention)",
    )
    scope.add_argument(
        "--project",
        action="store_true",
        help="install under <cwd>/.agents/skills",
    )
    scope.add_argument(
        "--target",
        metavar="DIR",
        help="install under an arbitrary skills root (any SKILL.md-compatible agent)",
    )
    parser.add_argument(
        "--from-zip",
        metavar="FILE",
        help="install from a distributable zip instead of the local skill folder",
    )
    parser.add_argument(
        "--force", action="store_true", help="replace an already installed copy"
    )
    parser.add_argument(
        "--uninstall", action="store_true", help="remove the installed copy"
    )
    args = parser.parse_args()

    target = resolve_target(args)
    destination = target / SKILL_NAME
    if destination.name != SKILL_NAME or destination.parent != target:
        emit(f"ERROR: refusing to operate on unexpected path: {destination}")
        return 1

    if args.uninstall:
        if not destination.exists():
            emit(f"NOT INSTALLED: {destination}")
            return 0
        shutil.rmtree(destination)
        emit(f"REMOVED: {destination}")
        return 0

    with tempfile.TemporaryDirectory() as temp_dir:
        source, error = source_from_args(args, temp_dir)
        if error:
            emit(f"ERROR: {error}")
            return 1
        ok, message = validate(source)
        if not ok:
            emit(f"ERROR: invalid skill package: {message}")
            return 1

        if destination.exists():
            if not args.force:
                emit(f"ALREADY INSTALLED: {destination}")
                emit("Re-run with --force to replace it.")
                return 1
            shutil.rmtree(destination)

        target.mkdir(parents=True, exist_ok=True)
        shutil.copytree(source, destination)
        files = sorted(
            str(path.relative_to(destination))
            for path in destination.rglob("*")
            if path.is_file()
        )
        emit(f"INSTALLED: {destination} ({len(files)} files)")
        for name in files:
            emit(f"  {name}")

        probe = destination / "scripts" / "ux_probe.py"
        if probe.is_file():
            result = subprocess.run(
                [sys.executable, str(probe)],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            emit(f"PREFLIGHT: {result.stdout.strip()}")
            if result.returncode != 0:
                emit(
                    "NOTE: skill installed, but ux is not available yet. "
                    "Install or build ux before running extraction tasks."
                )

    emit(f"NEXT: restart the agent, then ask it to extract data with {SKILL_NAME}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
