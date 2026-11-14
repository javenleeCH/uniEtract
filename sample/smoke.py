#!/usr/bin/env python3
"""Assert the CLI's behaviour against the committed fixtures in sample/.

Usage (from anywhere):
    python sample/smoke.py                     # run every case
    python sample/smoke.py --only encoding    # run cases whose name matches
    python sample/smoke.py --ux path/to/ux    # use a specific binary

The script resolves the repo root from its own location, so paths inside
`args` are written as `sample/...`. It invokes ux directly through
subprocess (no shell), which also avoids MSYS2 rewriting `//a` into a UNC
path on Windows Git Bash -- see README's known-limits section.

Fixture layout:
    basic/      one canonical file per input format
    edge/       encodings, BOM, size limits, malformed input, HTML quirks
    discover/   deterministic ordering, default-ignore pruning, glob, fuzzy
    generate.py rebuilds every fixture (non-UTF-8 and binary ones included)

Exit code: 0 when all cases pass, 1 on any mismatch, 2 on setup failure.

Known issues deliberately NOT asserted here (see the design doc's open
items and the project work log):
  * xpath("//x:book") with an unregistered prefix panics inside
    sxd-xpath 0.4.2 (rc=101, "No namespace for prefix"). It should be a
    Query(2) error instead; do not encode a panic as expected behaviour.
  * The design doc / --ns help text claim namespace URIs are not matched.
    The `default-ns-with-uri` and `prefixed-same-uri` cases below show the
    opposite: a correctly bound URI matches default-namespaced and
    differently-prefixed documents, while a wrong URI does not. The docs
    need correcting in a separate change.
"""

import argparse
import subprocess
import sys
from pathlib import Path

SAMPLE = Path(__file__).resolve().parent
REPO = SAMPLE.parent
S = "sample/"


def case(name, args, **kw):
    item = {"name": name, "args": args, "rc": 0, "stdout": None}
    item.update(kw)
    return item


CASES = [
    # ---------------- basic: one per input format ----------------
    case("basic/json-regex-jq",
         ["-e", r'jq(.users[].msg) | re(r"user=(?<id>\d+)") | jq({id})', S + "basic/users.json"],
         stdout='{"id":"42"}\n{"id":"7"}\n{"id":"99"}\n'),
    case("basic/jsonl-with-source",
         ["--with-source", "-e", "jq(.n)", S + "basic/events.jsonl"],
         stdout_contains=['"line":1', '"record":1', '"value":1', '"value":3']),
    case("basic/yaml",
         ["-O", "text", "-e", "jq(.database.host)", S + "basic/config.yaml"],
         stdout="db.internal\n"),
    case("basic/toml",
         ["-O", "text", "-e", "jq(.server.port)", S + "basic/config.toml"],
         stdout="8080\n"),
    case("basic/xml-ns-attr",
         ["--ns", "b=urn:books", "-e", 'xpath("//b:book") | attr("sku")', S + "basic/books.xml"],
         stdout='"ABC-123"\n"DEF-456"\n'),
    case("basic/html-attr",
         ["-O", "text", "-e", 'xpath("//a") | attr("href")', S + "basic/page.html"],
         stdout="https://example.com/x\nhttps://example.org/y\n"),
    case("basic/csv-select",
         ["-e", 'jq(select(.status == "failed") | {id, error})', S + "basic/jobs.csv"],
         stdout='{"error":"disk full","id":"2"}\n{"error":"disk, full","id":"3"}\n'),
    case("basic/tsv",
         ["-O", "text", "-e", "jq(.name)", S + "basic/jobs.tsv"],
         stdout="alice\nbob\n"),
    case("basic/text-log-regex",
         ["-f", "text", "-O", "text", "-e", r're(r"ERROR: (?<m>.*)") | jq(.m)', S + "basic/app.log"],
         stdout="disk full\nconnection reset\n"),
    case("basic/text-lines",
         ["-f", "text", "-O", "text", "-e", "text()", S + "basic/notes.txt"],
         stdout="第一行：中文\nsecond line\nthird line\n"),

    # ---------------- edge: encodings and byte-level hygiene ----------------
    case("edge/text-raw-split",
         ["-f", "text", "--text-mode", "raw", "-e",
          r'split(r"\r?\n----------------\r?\n") | re(r"id=(?<id>\d{6})")',
          S + "edge/split.txt"],
         stdout='{"0":"id=123456","1":"123456","id":"123456"}\n'
                '{"0":"id=654321","1":"654321","id":"654321"}\n'),
    case("edge/bom-utf8",
         ["-O", "text", "-e", "jq(.n)", S + "edge/bom-utf8.jsonl"],
         stdout="1\n2\n"),
    case("edge/gbk-auto-fallback",
         ["--encoding", "auto", "-f", "text", "-O", "text", "-e", "text()", S + "edge/gbk.txt"],
         stdout="中文 GBK 样本\n错误: 磁盘满\n", stderr_contains="assumed GB18030"),
    case("edge/utf16le",
         ["--encoding", "utf-16le", "-f", "text", "-O", "text", "-e", "text()",
          S + "edge/utf16le.txt"],
         stdout="UTF-16LE 文本\n第二行\n"),
    case("edge/utf16be-xml-declaration",
         ["--encoding", "utf-16be", "-O", "text", "-e",
          'xpath("//item") | attr("id")', S + "edge/utf16be.xml"],
         stdout="i1\n"),
    case("edge/crlf-stripped",
         ["-f", "text", "-O", "text", "-e", "text()", S + "edge/crlf.log"],
         stdout="INFO ok\nERROR: crlf\n"),
    case("edge/empty-jsonl",
         ["-e", "jq(.)", S + "edge/empty.jsonl"], stdout=""),
    case("edge/blank-lines-origin",
         ["--with-source", "-e", "jq(.n)", S + "edge/blank-lines.jsonl"],
         stdout_contains=['"line":1', '"record":1', '"line":3', '"record":2']),
    case("edge/big-integer",
         ["-e", "jq(.n)", S + "edge/big-integer.json"],
         stdout="123456789012345678901234567890\n"),
    case("edge/multi-document-yaml",
         ["-O", "text", "-e", "jq(.name)", S + "edge/multi-doc.yaml"],
         stdout="alice\nbob\n"),
    case("edge/csv-quoted-fields",
         ["-O", "text", "-e", "jq(.note)", S + "edge/csv-quotes.csv"],
         stdout='contains, comma\nsay "hi"\n'),
    case("edge/malformed-xml",
         ["-e", "text()", S + "edge/malformed.xml"],
         rc=1, stdout="", stderr_contains="invalid XML"),
    case("edge/invalid-utf8",
         ["-e", "text()", S + "edge/binary.bin"],
         rc=1, stdout="", stderr_contains="not valid UTF-8"),
    case("edge/binary-stdin-skip",
         ["-e", "text()"], stdin=b"plain\x00binary\n",
         stdout="", stderr_contains="binary input skipped"),
    case("edge/max-record-size",
         ["--max-record-size", "1KiB", "-f", "text", "-e", "text()", S + "edge/long-line.txt"],
         rc=1, stdout="", stderr_contains="line exceeds --max-record-size"),
    case("edge/no-extension-auto-misdetects",
         ["-e", "jq(.)", S + "edge/no-extension-jsonl"],
         rc=1, stdout="", stderr_contains="invalid JSON"),
    case("edge/no-extension-explicit-jsonl",
         ["-f", "jsonl", "-O", "text", "-e", "jq(.n)", S + "edge/no-extension-jsonl"],
         stdout="1\n2\n"),

    # ---------------- edge: HTML parser tree shape ----------------
    case("edge/implicit-tbody-not-visible",
         ["-f", "html", "-O", "text", "-e", 'xpath("//table/tr") | text()',
          S + "edge/html-tbody.html"], stdout=""),
    case("edge/explicit-tbody-visible",
         ["-f", "html", "-O", "text", "-e", 'xpath("//table/tbody/tr") | text()',
          S + "edge/html-tbody.html"], stdout="x\n"),
    case("edge/template-contents-in-dom",
         ["-f", "html", "-O", "text", "-e", 'xpath("//p") | attr("id")',
          S + "edge/html-template.html"], stdout="t\no\n"),
    case("edge/noscript-is-raw-text",
         ["-f", "html", "-O", "text", "-e", 'xpath("//p") | attr("id")',
          S + "edge/html-noscript.html"], stdout="o\n"),
    case("edge/html-case-folding",
         ["-f", "html", "-O", "text", "-e", 'xpath("//a") | attr("href")',
          S + "edge/html-uppercase.html"], stdout="/u\n"),

    # ---------------- edge: XML namespace reality ----------------
    case("edge/default-ns-with-uri",
         ["--ns", "b=urn:books", "-f", "xml", "-O", "text", "-e",
          'xpath("//b:book") | attr("sku")', S + "edge/default-ns.xml"],
         stdout="NS-1\n"),
    case("edge/default-ns-wrong-uri",
         ["--ns", "b=urn:other", "-f", "xml", "-O", "text", "-e",
          'xpath("//b:book") | attr("sku")', S + "edge/default-ns.xml"],
         stdout=""),
    case("edge/prefixed-same-uri",
         ["--ns", "b=urn:books", "-f", "xml", "-O", "text", "-e",
          'xpath("//b:book") | attr("sku")', S + "edge/prefixed.xml"],
         stdout="PRE-1\n"),

    # ---------------- aggregation, output modes, CLI contract ----------------
    case("cli/slurp-across-files",
         ["-f", "jsonl", "-O", "text", "-e", "slurp() | jq(length)",
          S + "basic/events.jsonl", S + "edge/blank-lines.jsonl"],
         stdout="5\n"),
    case("cli/json-pretty",
         ["-O", "json", "-e", "jq(.)", S + "basic/events.jsonl"],
         stdout='[\n  {\n    "level": "INFO",\n    "n": 1\n  },\n'
                '  {\n    "level": "ERROR",\n    "n": 2\n  },\n'
                '  {\n    "level": "INFO",\n    "n": 3\n  }\n]\n'),
    case("cli/json-compact",
         ["-O", "json", "--compact", "-e", "jq(.)", S + "basic/events.jsonl"],
         stdout='[{"level":"INFO","n":1},{"level":"ERROR","n":2},{"level":"INFO","n":3}]\n'),
    case("cli/max-results-is-not-silent",
         ["--max-results", "2", "-e", "jq(.n)", S + "basic/events.jsonl"],
         rc=1, stdout="1\n2\n", stderr_contains="--max-results"),
    case("cli/with-source-text-conflict",
         ["-O", "text", "--with-source", "-e", "text()", S + "basic/notes.txt"],
         rc=2, stdout="", stderr_contains="mutually exclusive"),
    case("cli/nesting-guard",
         ["-f", "json", "-e", "jq(" + "(" * 300 + ".a" + ")" * 300 + ")"],
         stdin=b"null\n", rc=2, stdout="", stderr_contains="nesting exceeds 128"),

    # ---------------- file discovery ----------------
    case("discover/default-ignore-prunes",
         ["--fuzzy", "noise", "-e", "text()", S + "discover"], stdout=""),
    case("discover/fuzzy",
         ["--fuzzy", "prodconfig", "-O", "text", "-e", "jq(.database.host)", S + "discover"],
         stdout="db.prod\n"),
    case("discover/glob-order",
         ["-f", "csv", "-O", "text", "-e", "jq(.total)", S + "discover/reports/*.csv"],
         stdout="10\n20\n"),
    case("discover/skip-incompatible",
         ["--skip-incompatible", "-O", "text", "-e", "jq(.n)",
          S + "discover/alpha.json", S + "basic/page.html"],
         stdout="1\n"),
]


def find_ux(explicit):
    if explicit:
        return Path(explicit).resolve()
    names = ["ux.exe", "ux"] if sys.platform == "win32" else ["ux", "ux.exe"]
    for profile in ("release", "debug"):
        for name in names:
            candidate = REPO / "target" / profile / name
            if candidate.is_file():
                return candidate
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ux", help="path to the ux binary")
    parser.add_argument("--only", help="run only cases whose name contains this text")
    options = parser.parse_args()

    ux = find_ux(options.ux)
    if ux is None or not ux.is_file():
        print("ux binary not found; run `cargo build --release` or pass --ux", file=sys.stderr)
        return 2

    selected = [c for c in CASES if not options.only or options.only in c["name"]]
    if not selected:
        print(f"no case matches {options.only!r}", file=sys.stderr)
        return 2

    print(f"ux: {ux}")
    print(f"cases: {len(selected)}\n")
    failures = []

    for item in selected:
        stdin = item.get("stdin")
        if isinstance(stdin, str):
            stdin = stdin.encode("utf-8")
        result = subprocess.run(
            [str(ux)] + item["args"],
            input=stdin,
            capture_output=True,
            cwd=str(REPO),
        )
        out = result.stdout.decode("utf-8", "replace")
        err = result.stderr.decode("utf-8", "replace")
        problems = []

        if result.returncode != item["rc"]:
            problems.append(f"rc: expected {item['rc']}, got {result.returncode}")
        expected_out = item.get("stdout")
        if expected_out is not None and out != expected_out:
            problems.append(f"stdout: expected {expected_out!r}, got {out!r}")
        for needle in item.get("stdout_contains", []):
            if needle not in out:
                problems.append(f"stdout missing {needle!r} (got {out!r})")
        stderr_needle = item.get("stderr_contains")
        if stderr_needle and stderr_needle not in err:
            problems.append(f"stderr missing {stderr_needle!r} (got {err!r})")

        if problems:
            failures.append(item["name"])
            print(f"FAIL  {item['name']}")
            for problem in problems:
                print(f"        {problem}")
        else:
            print(f"PASS  {item['name']}")

    print(f"\n{len(selected) - len(failures)}/{len(selected)} passed")
    if failures:
        print("failed: " + ", ".join(failures))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
