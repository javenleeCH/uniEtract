#!/usr/bin/env python3
"""CI gate: shipped docs must match the 0.3.1 CLI contract.

Invoked as a standalone script (no shell quoting, no rg) so it behaves the
same under bash and pwsh. Missing required files are a hard failure instead of
a silently skipped scan, and the NativeHeap fixture count is checked here too.
"""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]

DOCS = [
    "README.md",
    "CHANGELOG.md",
    "src/cli.rs",
    "docs/cli-design.md",
    "docs/path-spec-design.md",
]

REQUIRED = DOCS + [
    "Cargo.lock",
    "fuzz/Cargo.lock",
    "fuzz/Cargo.toml",
    "LICENSE",
    ".github/workflows/ci.yml",
    ".github/workflows/release.yml",
    ".github/scripts/check_test_count.py",
    ".github/scripts/check_fuzz_manifest.py",
    ".github/scripts/check_release_version.py",
    ".github/scripts/package_release.py",
    "tests/lib_run.rs",
    "tests/pathspec.rs",
    "tests/pathspec_scenario.rs",
    "tests/agent.rs",
    "tests/cli.rs",
    "tests/jq_compat.rs",
]

# Only authoritative docs and the fixture generator may not contain the old
# prototype wording or grammar. Negative test fixtures legitimately contain the
# legacy syntax, so they are checked with the narrower BANNED_ALL set.
BANNED_DOCS = (
    "does not exist in ux",
    "path-spec 未实现",
    "未实现提案",
    "当前不可用",
    "尚未落地",
    "未实施",
    "verify.py",
    "{ts:time:",
    "{n:lit:",
    # Rejected wording: 0.4.0 fixes BrokenPipe by I/O direction, so the old
    # variant-based wording must not come back (and the blanket panic promise
    # must stay out).
    "AppError::Io(BrokenPipe) 一律",
    "同样返回 0",
    "不证明输入被完整读取",
    "检测必须覆盖",
    "任何意外 panic 转为",
    # Design docs describe the finalized implementation; open-issue and
    # deferred-work wording must not creep back in.
    "遗留问题",
    "已知缺口",
    "后续关注",
    "待后续",
    "待实施",
    "候选",
)

BANNED_ALL = (
    "does not exist in ux",
    "未实现提案",
    "当前不可用",
    "尚未落地",
    "未实施",
    "verify.py",
)


def scan(paths, banned):
    hits = []
    for path in paths:
        text = path.read_text(encoding="utf-8", errors="replace")
        for phrase in banned:
            if phrase in text:
                hits.append((path.relative_to(ROOT).as_posix(), phrase))
    return hits


def main():
    missing = [p for p in REQUIRED if not (ROOT / p).is_file()]
    if missing:
        print("missing required files:", missing)
        return 1
    if not (ROOT / "scenarios").is_dir():
        print("missing scenarios/")
        return 1

    extra = list((ROOT / "scenarios").rglob("*.py"))
    extra += list((ROOT / "tests").glob("*.rs"))
    extra += list((ROOT / "sample").glob("*.md"))
    hits = scan([ROOT / p for p in DOCS], BANNED_DOCS) + scan(extra, BANNED_ALL)

    data = ROOT / "tests" / "fixtures" / "pathmatch" / "data"
    count = len([p for p in data.rglob("*") if p.is_file()]) if data.is_dir() else 0
    if count != 12:
        print(f"fixture count: {count} (expected 12)")
        return 1

    if hits:
        print("stale contract wording:", hits)
        return 1
    print(f"contract docs OK; {count} pathmatch fixtures")
    return 0


if __name__ == "__main__":
    sys.exit(main())
