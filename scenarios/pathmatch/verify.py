#!/usr/bin/env python3
"""Verify the NativeHeap path-matching scenario end to end.

Run from anywhere::

    python scenarios/pathmatch/verify.py
    python scenarios/pathmatch/verify.py --ux path/to/ux

This is an EXECUTABLE CHECK of the design proposal in
`.trae/documents/路径匹配模块-设计.md` §4/§5, not the Rust implementation:
`--path-spec` does not exist in ux yet. The Python compiler below implements
the subset of PathSpec needed for this scenario (segment/fragment grammar,
explicit re:/glob:/time:/lit: prefixes plus shorthand, strict calendar
validation, segment-level pruning). Once L1-L3 land, these same fixtures and
assertions can be replayed against the real CLI.

What is checked:
  1. fixtures on disk exactly match generate.FILES (no drift, no leftovers)
  2. the compiler rejects bad specs (colon in time format, unknown directive,
     duplicate capture name) and refuses to auto-append `**` to a directory
  3. the original colon-shaped template works on logical (non-Windows) paths
  4. the official spec matches exactly the 4 intended files, with captures
  5. strict time validation rejects the impossible 2026-02-30 directory
  6. segment-level pruning visits fewer entries than a full recursive walk
  7. ux extracts exactly 3 `NativeHeap pss` values from those 4 files
  8. ux's current glob / fuzzy discovery demonstrably over-matches

Exit code: 0 all assertions pass, 1 assertion failure, 2 setup failure.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

import generate

ROOT = generate.ROOT
REPO = ROOT.parents[1]
DATA = generate.DATA

EXPR = r're(r"NativeHeap pss:\s*(?<kb>\d+)\s*kB") | jq({kb})'
GLOB = "scenarios/pathmatch/data/*_SRRG/render_service/*min/hidumper/txt/*.txt"
DIR = "scenarios/pathmatch/data"

TIME_SHAPES = {
    "%Y": r"\d{4}", "%m": r"\d{2}", "%d": r"\d{2}",
    "%H": r"\d{2}", "%M": r"\d{2}", "%S": r"\d{2}",
}
EXPLICIT = re.compile(r"^([A-Za-z_]\w*):(re|glob|time|lit):(.*)$", re.S)
NAMED_SHORTHAND = re.compile(r"^([A-Za-z_]\w*):(.*)$", re.S)


class SpecError(ValueError):
    """A PathSpec compile error; in the real CLI this must be Usage(2)."""


@dataclass
class Segment:
    pattern: re.Pattern
    names: list[str]
    time_specs: dict[str, str] = field(default_factory=dict)


@dataclass
class Compiled:
    segments: list[Segment]
    capture_names: list[str]

    def anchored(self):
        return "^" + "/".join(s.pattern.pattern for s in self.segments) + "$"


def split_segments(spec: str) -> list[str]:
    out, buf, depth, escaped = [], [], 0, False
    for ch in spec:
        if escaped:
            buf.append(ch)
            escaped = False
            continue
        if ch == "\\":
            buf.append(ch)
            escaped = True
            continue
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth < 0:
                raise SpecError("unmatched }")
        if ch == "/" and depth == 0:
            out.append("".join(buf))
            buf = []
        else:
            buf.append(ch)
    if depth:
        raise SpecError("unmatched {")
    out.append("".join(buf))
    if any(not seg for seg in out):
        raise SpecError("empty segment (// is not allowed)")
    return out


def tokenize(segment: str) -> list[tuple[str, str]]:
    tokens, i = [], 0
    while i < len(segment):
        if segment[i] == "\\" and i + 1 < len(segment) and segment[i + 1] in "{}\\":
            tokens.append(("lit", segment[i + 1]))
            i += 2
            continue
        if segment[i] == "{":
            end = segment.find("}", i)
            if end < 0:
                raise SpecError(f"unterminated placeholder in {segment!r}")
            tokens.append(("ph", segment[i + 1:end]))
            i = end + 1
            continue
        end = segment.find("{", i)
        if end < 0:
            end = len(segment)
        tokens.append(("lit", segment[i:end]))
        i = end
    return tokens


def infer_kind(body: str) -> str:
    if body.startswith("%"):
        return "time"
    if ("*" in body or "?" in body) and "\\" not in body:
        return "glob"
    return "re"


def parse_placeholder(text: str, auto: int) -> tuple[str, str, str]:
    m = EXPLICIT.match(text)
    if m:
        return m.group(1), m.group(2), m.group(3)
    m = NAMED_SHORTHAND.match(text)
    if m:
        body = m.group(2)
        return m.group(1), infer_kind(body), body
    return f"cap{auto}", infer_kind(text), text


def glob_body(body: str) -> str:
    out = []
    for ch in body:
        if ch == "*":
            out.append("[^/]*")
        elif ch == "?":
            out.append("[^/]")
        elif ch in "[]":
            raise SpecError("character classes are outside this scenario's glob subset")
        else:
            out.append(re.escape(ch))
    return "".join(out)


def time_shape(fmt: str, allow_colon: bool) -> str:
    out, i = [], 0
    while i < len(fmt):
        code = fmt[i:i + 2]
        if code in TIME_SHAPES:
            out.append(TIME_SHAPES[code])
            i += 2
            continue
        ch = fmt[i]
        if ch == "%":
            raise SpecError(
                f"unknown time directive {fmt[i:i + 2]!r} "
                "(supported: %Y %m %d %H %M %S)")
        if ch == ":" and not allow_colon:
            raise SpecError(
                "time format contains ':': Windows directory names cannot contain "
                "a colon; use %H-%M-%S or an explicit {name:re:...} placeholder")
        out.append(re.escape(ch))
        i += 1
    return "".join(out)


def time_field_regex(fmt: str) -> tuple[re.Pattern, list[str]]:
    """Shape regex whose groups map to the directive letters in order."""
    parts, fields, i = [], [], 0
    while i < len(fmt):
        code = fmt[i:i + 2]
        if code in TIME_SHAPES:
            parts.append("(" + TIME_SHAPES[code] + ")")
            fields.append(code[1])
            i += 2
        else:
            parts.append(re.escape(fmt[i]))
            i += 1
    return re.compile("^" + "".join(parts) + "$"), fields


def compile_spec(spec: str, *, allow_colon: bool = False) -> Compiled:
    segments: list[Segment] = []
    names: list[str] = []
    auto = 0
    for seg_text in split_segments(spec):
        parts, seg_names, time_specs = [], [], {}
        for kind, text in tokenize(seg_text):
            if kind == "lit":
                parts.append(re.escape(text))
                continue
            auto += 1
            name, ph_kind, body = parse_placeholder(text, auto)
            if not re.fullmatch(r"[A-Za-z_]\w*", name):
                raise SpecError(f"illegal capture name {name!r}")
            if name in names:
                raise SpecError(f"duplicate capture name {name!r}")
            names.append(name)
            seg_names.append(name)
            if ph_kind == "re":
                fragment = body
            elif ph_kind == "glob":
                fragment = glob_body(body)
            elif ph_kind == "time":
                fragment = time_shape(body, allow_colon)
                time_specs[name] = body
            else:
                fragment = re.escape(body)
            parts.append(f"(?P<{name}>{fragment})")
        try:
            pattern = re.compile("".join(parts))
        except re.error as exc:
            raise SpecError(f"bad regex fragment: {exc}") from exc
        segments.append(Segment(pattern, seg_names, time_specs))
    # NOTE: a trailing pure-literal segment is treated as a FILE segment.
    # Nothing appends `**` implicitly (§4.2): a spec ending in a directory
    # simply cannot match a deeper path, and `--list` is how a user notices.
    return Compiled(segments, names)


def validate_times(seg: Segment, caps: dict, strict: bool) -> bool:
    if not strict or not seg.time_specs:
        return True
    for name, fmt in seg.time_specs.items():
        raw = caps.get(name)
        if raw is None:
            return False
        rx, fields = time_field_regex(fmt)
        m = rx.fullmatch(raw)
        if not m:
            return False
        v = {"Y": 1970, "m": 1, "d": 1, "H": 0, "M": 0, "S": 0}
        for letter, digits in zip(fields, m.groups()):
            v[letter] = int(digits)
        try:
            datetime.datetime(v["Y"], v["m"], v["d"], v["H"], v["M"], v["S"])
        except ValueError:
            return False
    return True


def match_rel(cp: Compiled, rel: str, strict: bool = True):
    parts = rel.replace("\\", "/").split("/")
    if len(parts) != len(cp.segments):
        return None
    caps = {}
    for seg, text in zip(cp.segments, parts):
        m = seg.pattern.fullmatch(text)
        if not m:
            return None
        caps.update({k: v for k, v in m.groupdict().items() if v is not None})
        if not validate_times(seg, caps, strict):
            return None
    return caps


def locate(cp: Compiled, base: Path, strict: bool = True):
    """§5.1 segment-level traversal: unmatched directories are never entered."""
    stats = {"read_dir": 0, "entries": 0, "pruned_dirs": 0}
    frontier: list[tuple[Path, dict]] = [(base, {})]
    for depth, seg in enumerate(cp.segments):
        last = depth == len(cp.segments) - 1
        nxt = []
        for current, caps in frontier:
            if not current.is_dir():
                continue
            stats["read_dir"] += 1
            with os.scandir(current) as it:
                entries = sorted(it, key=lambda e: e.name)
            for entry in entries:
                stats["entries"] += 1
                is_dir = entry.is_dir(follow_symlinks=False)
                if (last and is_dir) or (not last and not is_dir):
                    continue  # want a file on the last segment, a dir elsewhere
                m = seg.pattern.fullmatch(entry.name)
                merged = dict(caps)
                if m:
                    merged.update({k: v for k, v in m.groupdict().items() if v is not None})
                    merged_ok = validate_times(seg, merged, strict)
                else:
                    merged_ok = False
                if not merged_ok:
                    if not last:
                        stats["pruned_dirs"] += 1
                    continue
                nxt.append((Path(entry.path), merged))
        frontier = nxt
    return (sorted((p.relative_to(base).as_posix(), c) for p, c in frontier), stats)


def full_walk_stats(root: Path):
    stats = {"read_dir": 0, "entries": 0}
    for dirpath, dirnames, filenames in os.walk(root):
        if "__pycache__" in Path(dirpath).parts:
            continue
        stats["read_dir"] += 1
        stats["entries"] += len(dirnames) + len(filenames)
    return stats


def normalize_source_path(path: str) -> str:
    norm = str(path).replace("\\", "/")
    marker = "scenarios/pathmatch/"
    if marker in norm:
        norm = norm.split(marker, 1)[1]
    return norm


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


def run_ux(ux, args):
    result = subprocess.run(
        [str(ux), *args], capture_output=True, cwd=str(REPO),
    )
    return (result.returncode,
            result.stdout.decode("utf-8", "replace"),
            result.stderr.decode("utf-8", "replace"))


def parse_rows(stdout):
    rows = []
    for line in stdout.splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows


FAILURES = []


def check(label, ok, detail=""):
    print(f"  {'PASS' if ok else 'FAIL'}  {label}" + (f"  {detail}" if detail else ""))
    if not ok:
        FAILURES.append(label)


def verify_fixtures():
    print("\n[1] fixtures on disk")
    actual = {p.relative_to(ROOT).as_posix() for p in DATA.rglob("*") if p.is_file()}
    expected = set(generate.FILES)
    check("file set matches generate.FILES", actual == expected,
          f"{len(actual)} files, {len(expected)} expected")
    bad = [rel for rel, (content, _, _) in generate.FILES.items()
           if not (ROOT / rel).is_file()
           or (ROOT / rel).read_bytes() != content.encode("utf-8")]
    check("every file's bytes are unchanged", not bad, repr(bad))


def verify_compiler():
    print("\n[2] PathSpec compiler (design §4)")
    official = compile_spec(generate.OFFICIAL_SPEC)
    check("capture names", official.capture_names == ["ts", "mins", "file"],
          repr(official.capture_names))
    print("  anchored regex: " + official.anchored())

    for label, spec, kw, needle in [
        ("colon in time format rejected",
         "data/{ts:time:%Y-%m-%dT%H:%M:%S}_SRRG/x.txt", {}, "colon"),
        ("unknown time directive rejected",
         "data/{ts:time:%Y-%q}_SRRG/x.txt", {}, "unknown time directive"),
        ("duplicate capture rejected",
         "data/{a:re:\\d+}/{a:re:\\d+}.txt", {}, "duplicate capture name"),
    ]:
        try:
            compile_spec(spec, **kw)
            check(label, False, "no error raised")
        except SpecError as exc:
            check(label, needle in str(exc), repr(str(exc)))

    # §4.2: a spec ending in a directory must NOT silently become `**`.
    dir_only = compile_spec(generate.ORIGINAL_TEMPLATE, allow_colon=True)
    files = [p.relative_to(ROOT).as_posix() for p in DATA.rglob("*") if p.is_file()]
    check("directory-shaped spec does not silently match files underneath",
          all(match_rel(dir_only, rel) is None for rel in files),
          f"{len(files)} candidate files, 0 matches")


def verify_matching():
    print("\n[3] exact path matching on the 12 on-disk files")
    official = compile_spec(generate.OFFICIAL_SPEC)
    expected = sorted(rel for rel, (_, ok, _) in generate.FILES.items() if ok)
    all_files = sorted(p.relative_to(ROOT).as_posix() for p in DATA.rglob("*") if p.is_file())

    regex_hits = sorted(rel for rel in all_files
                        if match_rel(official, rel) is not None)
    check("anchored regex selects exactly the 4 intended paths",
          regex_hits == expected, f"{len(regex_hits)} hits")

    located, stats = locate(official, ROOT)
    check("segment-level locate agrees with the regex",
          [rel for rel, _ in located] == expected,
          f"{len(located)} hits")
    check("captures are populated", all(
        set(("ts", "mins", "file")) <= set(caps) for _, caps in located), "")
    for rel, caps in located[:3]:
        print(f"    {rel}\n      captures: {caps}")

    loose_hits = sorted(rel for rel in all_files
                        if match_rel(official, rel, strict=False) is not None)
    check("strict calendar check rejects 2026-02-30",
          len(loose_hits) == len(expected) + 1
          and any("2026-02-30" in r for r in loose_hits),
          f"strict=False -> {len(loose_hits)} hits")

    # Baseline from the previous round: hand-written external path regex,
    # piped into ux. PathSpec must agree with it on shape and add two things:
    # strict calendar validation and named captures.
    baseline = re.compile(
        r"data/\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}_SRRG/"
        r"render_service/\d+min/hidumper/txt/[^/]+\.txt")
    baseline_hits = sorted(rel for rel in all_files if baseline.fullmatch(rel))
    check("PathSpec agrees with the hand-written external regex on shape",
          baseline_hits == loose_hits,
          f"external regex -> {len(baseline_hits)} hits")
    check("strict PathSpec = external regex minus impossible date",
          expected == [r for r in baseline_hits if "2026-02-30" not in r])

    walk = full_walk_stats(ROOT)
    print(f"  pruning: locate visited {stats['read_dir']} dirs / {stats['entries']} entries "
          f"({stats['pruned_dirs']} dirs pruned); "
          f"full walk {walk['read_dir']} dirs / {walk['entries']} entries")
    check("segment pruning inspects fewer entries than a full walk",
          stats["entries"] < walk["entries"],
          f"{stats['entries']} < {walk['entries']}")
    return expected


def verify_colon_paths():
    print("\n[4] original colon-shaped template (logical paths, Windows cannot create them)")
    original = compile_spec(
        generate.ORIGINAL_TEMPLATE + "/{file:glob:*.txt}", allow_colon=True)
    hits = [p for p in generate.LOGICAL_COLON_PATHS if match_rel(original, p)]
    check("colon template matches the 3 conforming logical paths", len(hits) == 3,
          f"{len(hits)}/4")
    check("colon template rejects xxmin", all("xxmin" not in p for p in hits))
    official = compile_spec(generate.OFFICIAL_SPEC)
    check("official hyphen spec rejects colon paths (cross-platform semantics differ)",
          all(match_rel(official, p) is None for p in generate.LOGICAL_COLON_PATHS))


def verify_ux(ux, expected):
    print("\n[5] ux extraction on the precisely located files")
    targets = ["scenarios/pathmatch/" + rel for rel in expected]
    rc, out, err = run_ux(ux, ["-f", "text", "--with-source", "-e", EXPR, *targets])
    check("ux rc=0", rc == 0, f"rc={rc} stderr={err.strip()[:200]}")
    if rc != 0:
        return
    rows = parse_rows(out)
    got = {}
    provenance_ok = True
    for row in rows:
        src = row.get("source", {})
        rel = normalize_source_path(src.get("path", ""))
        got[rel] = str(row.get("value", {}).get("kb"))
        if src.get("line") != 5 or src.get("record") != 5:
            provenance_ok = False
    want = {rel: kb for rel, (_, ok, kb) in generate.FILES.items() if ok and kb}
    check("exactly 3 NativeHeap pss values", len(rows) == 3, f"{len(rows)} rows")
    check("values and source paths match", got == want, repr(got))
    check("--with-source line/record point at the hit line", provenance_ok)
    for rel, kb in sorted(got.items()):
        print(f"    kb={kb:>5}  {rel}")

    print("\n[6] control group: what ux does today without PathSpec")
    controls = {
        "glob  *_SRRG/*min/txt/*.txt": ["-f", "text", "--with-source", "-e", EXPR, GLOB],
        "fuzzy hidumper (whole tree)": ["-f", "text", "--with-source",
                                        "--fuzzy", "hidumper", "-e", EXPR, DIR],
    }
    for label, args in controls.items():
        rc2, out2, err2 = run_ux(ux, args)
        if rc2 != 0:
            print(f"    {label:<32} rc={rc2} stderr={err2.strip()[:120]}")
            continue
        paths = [normalize_source_path(r.get("source", {}).get("path", ""))
                 for r in parse_rows(out2)]
        extra = [p for p in paths if p not in expected]
        print(f"    {label:<32} {len(paths)} rows, {len(extra)} from decoy paths")
        check(f"{label} over-matches (proves the need for locate)", len(paths) > 3)

    rc3, out3, _ = run_ux(ux, ["--list", DIR])
    if rc3 == 0:
        try:
            listed = len(parse_rows(out3))
            print(f"    ux --list sees {listed} inputs, "
                  "but has no structural path include filter")
        except json.JSONDecodeError:
            print("    ux --list output is not JSONL; skipped inventory count")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ux", help="path to the ux binary")
    options = parser.parse_args()

    if not DATA.is_dir():
        print("fixtures missing; run `python scenarios/pathmatch/generate.py`", file=sys.stderr)
        return 2
    ux = find_ux(options.ux)
    print(f"ux binary: {ux or 'NOT FOUND (cargo build --release)'}")

    verify_fixtures()
    verify_compiler()
    expected = verify_matching()
    verify_colon_paths()
    if ux is None or not ux.is_file():
        print("\nux binary not found; sections [5]/[6] skipped", file=sys.stderr)
        return 1
    verify_ux(ux, expected)

    print(f"\n{'=' * 60}")
    if FAILURES:
        print(f"{len(FAILURES)} FAILED: " + "; ".join(FAILURES))
        return 1
    print("all checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
