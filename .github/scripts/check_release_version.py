#!/usr/bin/env python3
"""CI gate: tag, manifests, docs and binary must agree on one release version.

The tag is read from RELEASE_TAG / GITHUB_REF so no shell-specific quoting is
needed under bash or pwsh. Branch and pull-request refs are local checks with
the tag derived from Cargo.toml. With --write-github-output, `version` and
`tag` are appended to the Actions step output file.
"""

from pathlib import Path
import argparse
import os
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$")
LOCK_FIELD = re.compile(r'^(\w+)\s*=\s*"([^"]+)"', re.MULTILINE)


def cargo_version():
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    if "[package]" not in text:
        raise ValueError("Cargo.toml has no [package] table")
    block = text.split("[package]", 1)[1]
    match = re.search(r'^version\s*=\s*"([^"]+)"', block, re.MULTILINE)
    if not match:
        raise ValueError("Cargo.toml [package] has no version")
    return match.group(1)


def lock_version(relative):
    text = (ROOT / relative).read_text(encoding="utf-8")
    for block in text.split("[[package]]")[1:]:
        fields = dict(LOCK_FIELD.findall(block))
        if fields.get("name") == "uniextract":
            return fields.get("version")
    return None


def resolved_tag():
    raw = (os.environ.get("RELEASE_TAG") or os.environ.get("GITHUB_REF") or "").strip()
    if raw.startswith("refs/tags/"):
        return raw[len("refs/tags/"):]
    if raw.startswith("refs/"):
        # Branch or pull-request ref: derive the tag from Cargo.toml locally.
        return None
    return raw or None


def changelog_section(version, text):
    lines = text.splitlines()
    header = re.compile(rf"^## {re.escape(version)}(?![\d.])")
    start = next((i for i, line in enumerate(lines) if header.match(line)), None)
    if start is None:
        return None
    end = next(
        (i for i in range(start + 1, len(lines)) if lines[i].startswith("## ")),
        len(lines),
    )
    return "\n".join(lines[start + 1:end]).strip()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", help="built ux executable to check with --version")
    parser.add_argument("--write-github-output", action="store_true")
    parser.add_argument("--notes", help="write release notes for this version to PATH")
    args = parser.parse_args()

    try:
        version = cargo_version()
    except (OSError, ValueError) as exc:
        print(f"cannot read Cargo.toml version: {exc}")
        return 1
    if not SEMVER.match(version):
        print(f"invalid Cargo.toml version: {version}")
        return 1

    tag = f"v{version}"
    errors = []
    observed = resolved_tag()
    if observed is not None and observed != tag:
        errors.append(f"tag {observed!r} does not match Cargo.toml version {version!r}")

    for relative in ("Cargo.lock", "fuzz/Cargo.lock"):
        found = lock_version(relative)
        if found != version:
            errors.append(f"{relative}: uniextract {found!r} != {version!r}")

    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    section = changelog_section(version, changelog)
    if section is None:
        errors.append(f"CHANGELOG.md has no ## {version} section")

    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    if f"ux {version}" not in readme:
        errors.append(f"README.md does not document ux {version}")
    if f"`{tag}`" not in readme:
        errors.append(f"README.md does not reference {tag}")

    design = (ROOT / "docs/cli-design.md").read_text(encoding="utf-8")
    if tag not in design:
        errors.append(f"docs/cli-design.md does not reference {tag}")

    if args.binary:
        binary = Path(args.binary)
        if not binary.is_file():
            errors.append(f"missing binary: {binary}")
        else:
            run = subprocess.run(
                [str(binary), "--version"],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            expected = f"ux {version}"
            if run.returncode != 0 or run.stdout.strip() != expected:
                errors.append(
                    f"{binary} --version returned {run.stdout.strip()!r} "
                    f"(rc={run.returncode}), expected {expected!r}"
                )

    if errors:
        for error in errors:
            print("release version mismatch:", error)
        return 1

    if args.write_github_output:
        output_file = os.environ.get("GITHUB_OUTPUT")
        if output_file:
            with open(output_file, "a", encoding="utf-8") as handle:
                handle.write(f"version={version}\ntag={tag}\n")

    if args.notes:
        notes = (
            f"# ux {version}\n\n"
            f"{section}\n\n"
            "## 校验\n\n"
            "下载对应平台的压缩包与 `SHA256SUMS`，对照其中同名条目校验哈希后再解压使用"
            "（Linux `sha256sum <file>`、macOS `shasum -a 256 <file>`、"
            "Windows `Get-FileHash -Algorithm SHA256 <file>`）。\n"
        )
        notes_path = Path(args.notes)
        notes_path.parent.mkdir(parents=True, exist_ok=True)
        notes_path.write_text(notes, encoding="utf-8")

    print(f"release version OK: {version} ({tag})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
