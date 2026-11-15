#!/usr/bin/env python3
"""CI gate: shipped docs must match the 0.3.1 CLI contract.

Invoked as a file (not `python -c '...'`) so the same command works under bash
and pwsh with Chinese paths. Missing required files are a hard failure instead
of a silently skipped scan, and the NativeHeap fixture count is checked here
too.
"""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]

DOCS = [
    "README.md",
    "CHANGELOG.md",
    "src/cli.rs",
    ".trae/documents/统一信息抽取器-cli-设计.md",
    ".trae/documents/路径匹配模块-设计.md",
]

REQUIRED = DOCS + [
    "Cargo.lock",
    "fuzz/Cargo.lock",
    "fuzz/Cargo.toml",
    ".github/workflows/ci.yml",
    ".github/scripts/check_test_count.py",
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
    # Rejected wording from earlier rounds; keep these so the direction-based
    # BrokenPipe promise and the blanket panic promise cannot creep back in.
    "输出侧 BrokenPipe 为 0，输入侧读失败为 1",
    "输入侧读失败仍为",
    "仅输出侧",
    "检测必须覆盖",
    "任何意外 panic 转为",
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
