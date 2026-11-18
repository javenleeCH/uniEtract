#!/usr/bin/env python3
"""Report a ux problem or usage question to the project's public tracker.

An agent that is blocked by the CLI, or cannot tell whether observed behavior
is intended, should file one report instead of silently working around it. This
script composes a reproducible report (environment, command, expectation,
actual output, optional preflight) and submits it:

  - `--kind bug`      -> GitHub issue, labels `agent-report` + `bug`
  - `--kind question` -> GitHub discussion in the Q&A category

Submission uses the `gh` CLI and needs network access. When `gh` is missing or
the request fails, the report is written to a file under `.ux-feedback/outbox/`
and the script prints the exact command (or web URL) a human can use to submit
it. The report contains only what the caller passes in plus environment data;
never pass credentials, personal data or proprietary file contents.

Usage:
  python ux_feedback.py report --kind bug --title "..." --summary "..." \
      --command "ux -e ..." --expected "..." --actual "..." [--probe] \
      [--include-output FILE] [--body-file FILE|-] [--dry-run] [--json]
  python ux_feedback.py report --kind bug --title "..." --raw-body - < report.md
  python ux_feedback.py check                  # tracker reachable?
  python ux_feedback.py search --query TEXT    # look for duplicates first
  python ux_feedback.py selftest               # offline self-check

No third-party dependencies; requires Python 3.8+ and works on Windows too.
Exit codes: 0 submitted (or dry run), 1 submission error, 2 usage error,
3 report written to disk (submit by hand).
"""

import argparse
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

DEFAULT_REPO = "javenleeCH/uniEtract"
LABEL = "agent-report"
QUESTION_CATEGORY = "q-a"
MAX_OUTPUT = 6000
GH_TIMEOUT = 90
TITLE_LIMIT = 256
OUTBOX_NAME = Path(".ux-feedback") / "outbox"


class FeedbackError(RuntimeError):
    """A submission attempt failed in a way the caller can act on."""


# ---------------------------------------------------------------------------
# Environment facts
# ---------------------------------------------------------------------------


def _probe_module():
    """Load the bundled preflight module without requiring it to be importable."""
    scripts = str(Path(__file__).resolve().parent)
    if scripts not in sys.path:
        sys.path.insert(0, scripts)
    # Do not leave a __pycache__ inside the skill tree: dist/ux-extract.zip is
    # compared file-for-file against it, and bytecode is not shipped.
    sys.dont_write_bytecode = True
    try:
        import ux_probe  # type: ignore

        return ux_probe
    except ImportError:
        return None


def ux_facts():
    """Return (version line, executable or None); never raises."""
    probe = _probe_module()
    exe = None
    try:
        if probe is not None:
            exe = probe.resolve_ux()
        else:
            exe = shutil.which("ux")
            if exe is None:
                for rel in ("target/release/ux.exe", "target/release/ux",
                            "target/debug/ux.exe", "target/debug/ux"):
                    candidate = Path.cwd() / rel
                    if candidate.is_file():
                        exe = str(candidate.resolve())
                        break
    except Exception:  # pragma: no cover - defensive only
        exe = None
    if exe is None:
        return "unavailable (run `ux --version` manually)", None
    code, stdout, stderr = probe.run(exe, ["--version"]) if probe is not None else _run_version(exe)
    if code != 0:
        return f"`--version` failed (rc={code}): {stderr.strip()}", exe
    return stdout.strip() or "empty `--version` output", exe


def _run_version(exe):
    completed = subprocess.run(
        [exe, "--version"], stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    return (
        completed.returncode,
        completed.stdout.decode("utf-8", "replace"),
        completed.stderr.decode("utf-8", "replace"),
    )


def probe_report():
    """Run the bundled preflight and return its stdout, or a short error text."""
    probe = _probe_module()
    if probe is None:
        return "preflight unavailable (scripts/ux_probe.py not importable)"
    try:
        completed = subprocess.run(
            [sys.executable, str(Path(__file__).resolve().parent / "ux_probe.py")],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=GH_TIMEOUT,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        return f"preflight failed to run: {exc}"
    text = completed.stdout.decode("utf-8", "replace").strip()
    return text or f"preflight produced no output (rc={completed.returncode})"


# ---------------------------------------------------------------------------
# Report composition
# ---------------------------------------------------------------------------


def use_utf8_streams():
    """Write UTF-8 when stdio is redirected.

    On Windows Python picks the ANSI code page for pipes while many hosts
    (CI logs, agent runners) decode UTF-8; without this, non-ASCII titles show
    as mojibake. A real console keeps its native encoding.
    """
    for stream in (sys.stdout, sys.stderr):
        if stream is None or stream.isatty():
            continue
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except (AttributeError, ValueError, OSError):
            pass


def slugify(title, limit=40):
    slug = re.sub(r"[^0-9a-zA-Z\u4e00-\u9fff]+", "-", title.strip().lower())
    slug = slug.strip("-") or "report"
    return slug[:limit].strip("-") or "report"


def _fence(text):
    return "```\n" + text.strip("\n") + "\n```"


def environment_lines():
    version, exe = ux_facts()
    return [
        "### Environment",
        "",
        f"- ux: {version}",
        f"- executable: `{exe}`" if exe else "- executable: none resolved",
        f"- OS: {platform.platform()}",
        f"- Python: {platform.python_version()}",
        "",
    ]


def compose_report(title, kind, summary, command, expected, actual,
                   output_text=None, probe_text=None, raw_body=None,
                   repo=DEFAULT_REPO):
    """Build the issue/discussion body plus a self-contained markdown file."""
    marker = "<!-- ux-feedback: generated by .agents/skills/ux-extract/scripts/ux_feedback.py -->"
    if raw_body is not None:
        lines = [marker, "", raw_body.strip(), ""] + environment_lines()
    else:
        lines = [
            marker,
            "",
            "### Summary",
            "",
            summary.strip() if summary and summary.strip() else "_(not provided)_",
            "",
        ] + environment_lines()
    if command and command.strip():
        lines += ["### Command", "", _fence(command), ""]
    if expected and expected.strip():
        lines += ["### Expected", "", expected.strip(), ""]
    if actual and actual.strip():
        lines += ["### Actual", "", actual.strip(), ""]
    if output_text and output_text.strip():
        text = output_text.strip()
        if len(text) > MAX_OUTPUT:
            text = text[:MAX_OUTPUT] + "\n... (truncated by ux_feedback.py)"
        lines += ["### Output", "", _fence(text), ""]
    if probe_text:
        lines += ["### Preflight (ux_probe.py)", "", _fence(probe_text), ""]

    body = "\n".join(lines).rstrip() + "\n"
    hint = (
        f"<!-- submit: gh issue create --repo {repo} "
        f"--title {json.dumps(title, ensure_ascii=False)} --body-file <this file> -->"
        if kind == "bug"
        else f"<!-- submit: paste into https://github.com/{repo}/discussions/new?category={QUESTION_CATEGORY} -->"
    )
    markdown = (
        f"# {title}\n\n"
        f"{body}\n"
        f"---\n\n{hint}\n"
    )
    return {"title": title, "kind": kind, "body": body, "markdown": markdown}


# ---------------------------------------------------------------------------
# GitHub transport
# ---------------------------------------------------------------------------


def _gh(args, payload=None, timeout=GH_TIMEOUT):
    if shutil.which("gh") is None:
        raise FeedbackError("gh CLI not found on PATH")
    completed = subprocess.run(
        ["gh", *args],
        input=payload,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )
    stdout = completed.stdout.decode("utf-8", "replace").strip()
    stderr = completed.stderr.decode("utf-8", "replace").strip()
    if completed.returncode != 0:
        raise FeedbackError(stderr or stdout or f"gh exited {completed.returncode}")
    return stdout


def _gh_json(args, payload=None, timeout=GH_TIMEOUT):
    text = _gh(args, payload, timeout)
    try:
        return json.loads(text)
    except ValueError as exc:
        raise FeedbackError(f"gh returned non-JSON output: {exc}") from exc


def _graphql(query, variables, timeout=GH_TIMEOUT):
    payload = {"query": query, "variables": variables}
    data = _gh_json(
        ["api", "graphql", "--input", "-"],
        json.dumps(payload, ensure_ascii=False).encode("utf-8"),
        timeout=timeout,
    )
    if isinstance(data, dict) and data.get("errors"):
        raise FeedbackError(json.dumps(data["errors"], ensure_ascii=False))
    return data.get("data") or {}


def repo_parts(repo):
    if repo.count("/") != 1:
        raise FeedbackError(f"invalid --repo `{repo}`; expected OWNER/NAME")
    return repo.split("/", 1)


def submit_issue(repo, report, timeout=GH_TIMEOUT):
    payload = {
        "title": report["title"],
        "body": report["body"],
        "labels": [LABEL] + (["bug"] if report["kind"] == "bug" else []),
    }
    try:
        data = _gh_json(
            ["api", f"repos/{repo}/issues", "--input", "-"],
            json.dumps(payload, ensure_ascii=False).encode("utf-8"),
            timeout=timeout,
        )
    except FeedbackError as first:
        # A repository without the reporting label must not swallow the report.
        retry = {"title": report["title"], "body": report["body"]}
        try:
            data = _gh_json(
                ["api", f"repos/{repo}/issues", "--input", "-"],
                json.dumps(retry, ensure_ascii=False).encode("utf-8"),
                timeout=timeout,
            )
        except FeedbackError:
            raise first
    return data.get("html_url") or data.get("url") or ""


def submit_discussion(repo, report, timeout=GH_TIMEOUT):
    owner, name = repo_parts(repo)
    meta = _graphql(
        "query($owner:String!,$name:String!){repository(owner:$owner,name:$name){"
        "id discussionCategories(first:50){nodes{id slug}}}}",
        {"owner": owner, "name": name},
        timeout=timeout,
    )
    repository = meta.get("repository") or {}
    repo_id = repository.get("id")
    categories = (repository.get("discussionCategories") or {}).get("nodes") or []
    category_id = next(
        (node["id"] for node in categories if node.get("slug") == QUESTION_CATEGORY),
        None,
    )
    if not repo_id or not category_id:
        raise FeedbackError(
            f"discussions (category `{QUESTION_CATEGORY}`) are unavailable on {repo}"
        )
    data = _graphql(
        "mutation($repo:ID!,$category:ID!,$title:String!,$body:String!){"
        "createDiscussion(input:{repositoryId:$repo,categoryId:$category,"
        "title:$title,body:$body}){discussion{number url}}}",
        {
            "repo": repo_id,
            "category": category_id,
            "title": report["title"],
            "body": report["body"],
        },
        timeout=timeout,
    )
    discussion = (data.get("createDiscussion") or {}).get("discussion") or {}
    return discussion.get("url") or ""


# ---------------------------------------------------------------------------
# Offline fallback
# ---------------------------------------------------------------------------


def write_pending(report, outbox):
    outbox = Path(outbox)
    outbox.mkdir(parents=True, exist_ok=True)
    path = outbox / f"{time.strftime('%Y%m%d-%H%M%S')}-{report['kind']}-{slugify(report['title'])}.md"
    path.write_text(report["markdown"], encoding="utf-8")
    return path


def submission_hint(report, repo, path):
    if report["kind"] == "bug":
        return [
            f"submit the issue with:\n  gh issue create --repo {repo} "
            f"--title {json.dumps(report['title'], ensure_ascii=False)} "
            f"--body-file {path}",
            f"or open https://github.com/{repo}/issues/new and paste the file content",
        ]
    return [
        f"open https://github.com/{repo}/discussions/new?category={QUESTION_CATEGORY}",
        f"paste {path} as the discussion body",
    ]


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------


def read_stdin_text():
    """Read piped markdown as UTF-8 first, then as the host encoding.

    Windows pipes carry the ANSI code page, while agents and CI almost always
    write UTF-8; preferring UTF-8 keeps Chinese/emoji bodies intact.
    """
    if hasattr(sys.stdin, "buffer"):
        data = sys.stdin.buffer.read()
    else:
        return sys.stdin.read()
    for encoding in ("utf-8", sys.stdin.encoding or "utf-8"):
        try:
            return data.decode(encoding)
        except (UnicodeDecodeError, LookupError):
            continue
    return data.decode("utf-8", "replace")


def read_text_source(source):
    if source == "-":
        return read_stdin_text()
    return Path(source).read_text(encoding="utf-8", errors="replace")


def cmd_report(args):
    repo = args.repo
    title = " ".join(args.title.split())
    if not title:
        raise SystemExit("--title must not be empty")
    if len(title) > TITLE_LIMIT:
        raise SystemExit(
            f"--title is {len(title)} characters; GitHub allows at most "
            f"{TITLE_LIMIT}. Shorten it and put the detail in the body."
        )
    if args.timeout <= 0:
        raise SystemExit("--timeout must be a positive number of seconds")

    if args.raw_body and any((args.summary, args.ux_command, args.expected,
                              args.actual, args.body_file,
                              args.include_output, args.probe)):
        raise SystemExit(
            "--raw-body replaces the structured sections; drop "
            "--summary/--command/--expected/--actual/--body-file/"
            "--include-output/--probe"
        )

    output_text = None
    if args.include_output:
        try:
            output_text = read_text_source(args.include_output)
        except OSError as exc:
            raise SystemExit(f"cannot read --include-output: {exc}")
    if args.body_file:
        try:
            extra = read_text_source(args.body_file)
        except OSError as exc:
            raise SystemExit(f"cannot read --body-file: {exc}")
        args.actual = "\n\n".join(part for part in (args.actual, extra) if part)
    raw_body = None
    if args.raw_body:
        try:
            raw_body = read_text_source(args.raw_body)
        except OSError as exc:
            raise SystemExit(f"cannot read --raw-body: {exc}")
        if not raw_body.strip():
            raise SystemExit("--raw-body source is empty")

    if (args.kind == "bug" and raw_body is None
            and not any((args.ux_command, args.expected, args.actual))):
        print(
            "warning: no --command/--expected/--actual; maintainers will "
            "have to ask for details",
            file=sys.stderr,
        )

    report = compose_report(
        title=title,
        kind=args.kind,
        summary=args.summary,
        command=args.ux_command,
        expected=args.expected,
        actual=args.actual,
        output_text=output_text,
        probe_text=probe_report() if args.probe else None,
        raw_body=raw_body,
        repo=repo,
    )
    if args.dry_run:
        print(report["markdown"])
        return 0

    result = {"submitted": False, "kind": args.kind, "title": title,
              "repo": repo, "url": None, "path": None, "reason": None,
              "channel": None}
    try:
        if args.kind == "bug":
            result["url"] = submit_issue(repo, report, timeout=args.timeout)
            result["channel"] = "issue"
        else:
            result["url"] = submit_discussion(repo, report, timeout=args.timeout)
            result["channel"] = "discussion"
        result["submitted"] = True
    except (FeedbackError, subprocess.TimeoutExpired, OSError) as exc:
        result["reason"] = str(exc)
        try:
            path = write_pending(report, args.outbox)
            result["path"] = str(path)
        except OSError as write_exc:
            result["reason"] += f"; outbox write failed: {write_exc}"
            print(
                "could not write the outbox report either; copy the markdown "
                "below and submit it by hand",
                file=sys.stderr,
            )
            print(report["markdown"], file=sys.stderr)

    if args.json:
        print(json.dumps(result, ensure_ascii=True, indent=2))
    elif result["submitted"]:
        print(f"submitted {result['channel']}: {result['url']}")
        print("next: keep that URL for the human, and continue with the "
              "documented workaround")
    else:
        print(f"not submitted ({result['reason']})")
        if result["path"]:
            print(f"report written to: {result['path']}")
            for line in submission_hint(report, repo, result["path"]):
                print(line)
    return 0 if result["submitted"] else 3


def cmd_check(args):
    repo = args.repo
    result = {
        "repo": repo,
        "gh": shutil.which("gh") is not None,
        "repo_reachable": False,
        "discussions": False,
        "label": False,
        "error": None,
    }
    try:
        _gh_json(["api", f"repos/{repo}"])
        result["repo_reachable"] = True
        try:
            _gh_json(["api", f"repos/{repo}/labels/{LABEL}"])
            result["label"] = True
        except FeedbackError:
            result["label"] = False
        try:
            owner, name = repo_parts(repo)
            meta = _graphql(
                "query($owner:String!,$name:String!){repository(owner:$owner,name:$name){"
                "discussionCategories(first:50){nodes{slug}}}}",
                {"owner": owner, "name": name},
            )
            categories = (meta.get("repository") or {}).get("discussionCategories") or {}
            result["discussions"] = any(
                node.get("slug") == QUESTION_CATEGORY
                for node in categories.get("nodes") or []
            )
        except FeedbackError:
            result["discussions"] = False
    except (FeedbackError, OSError, subprocess.TimeoutExpired) as exc:
        result["error"] = str(exc)
    print(json.dumps(result, ensure_ascii=True, indent=2))
    return 0 if all((result["gh"], result["repo_reachable"], result["discussions"], result["label"])) else 3


def cmd_search(args):
    repo = args.repo
    matches = []
    errors = []
    query = args.query.strip()
    if shutil.which("gh") is None:
        errors.append("gh CLI not found on PATH; duplicate search was skipped")
    else:
        try:
            data = _gh_json(
                ["api", "-X", "GET", "search/issues",
                 "-f", f"q=repo:{repo} type:issue {query}", "-f", "per_page=10"]
            )
            for item in data.get("items") or []:
                matches.append({
                    "kind": "issue",
                    "number": item.get("number"),
                    "title": item.get("title"),
                    "state": item.get("state"),
                    "url": item.get("html_url"),
                })
        except (FeedbackError, OSError, subprocess.TimeoutExpired) as exc:
            errors.append(f"issue search failed: {exc}")
        try:
            data = _graphql(
                "query($q:String!,$first:Int!){search(query:$q,type:DISCUSSION,first:$first){"
                "nodes{... on Discussion{number title url updatedAt}}}}",
                {"q": f"repo:{repo} {query}", "first": 10},
            )
            for node in (data.get("search") or {}).get("nodes") or []:
                if node:
                    matches.append({
                        "kind": "discussion",
                        "number": node.get("number"),
                        "title": node.get("title"),
                        "state": "discussion",
                        "url": node.get("url"),
                    })
        except (FeedbackError, OSError, subprocess.TimeoutExpired) as exc:
            errors.append(f"discussion search failed: {exc}")
    print(json.dumps(
        {"query": query, "matches": matches, "errors": errors},
        ensure_ascii=True, indent=2,
    ))
    return 0


def cmd_selftest(_args):
    checks = []

    def check(name, condition):
        checks.append((name, bool(condition)))

    report = compose_report(
        title="selftest title",
        kind="bug",
        summary="summary line",
        command="ux -e 'jq(.id)' sample.json",
        expected="1",
        actual="0",
        output_text="stdout\n",
    )
    for section in ("### Summary", "### Environment", "### Command",
                    "### Expected", "### Actual", "### Output"):
        check(f"body has {section}", section in report["body"])
    check("markdown starts with title", report["markdown"].startswith("# selftest title\n"))
    check("markdown carries submit hint", "<!-- submit:" in report["markdown"])
    check("slug drops punctuation", slugify("fix: jq() fails!") == "fix-jq-fails")
    check("slug survives empty title", slugify("   ") == "report")

    question = compose_report("q", "question", "why?", None, None, None)
    check("question points at discussions",
          "discussions/new?category=q-a" in question["markdown"])

    raw = compose_report("t", "bug", None, None, None, None,
                         raw_body="## What\n\nbroken output", repo="owner/name")
    check("raw body is preserved", "## What" in raw["body"])
    check("raw body appends environment", "### Environment" in raw["body"])
    check("raw body hint names the repo", "owner/name" in raw["markdown"])
    check("timeout defaults to GH_TIMEOUT",
          build_parser().parse_args(["report", "--title", "t"]).timeout == GH_TIMEOUT)

    parsed = build_parser().parse_args(
        ["report", "--repo", "owner/name", "--title", "t", "--dry-run"]
    )
    check("--repo is accepted after the subcommand", parsed.repo == "owner/name")
    check("subcommand survives parsing", parsed.command == "report")
    check("--command does not clobber the subcommand",
          build_parser().parse_args(
              ["report", "--title", "t", "--command", "ux -e jq(.)"]
          ).ux_command == "ux -e jq(.)")

    with tempfile.TemporaryDirectory() as tmp:
        path = write_pending(report, Path(tmp) / "outbox")
        check("pending file written", path.is_file())
        check("pending file round-trips", "# selftest title" in path.read_text(encoding="utf-8"))

    failed = [name for name, ok in checks if not ok]
    if failed:
        for name in failed:
            print(f"selftest FAILED: {name}")
        return 1
    print(f"selftest OK ({len(checks)} checks)")
    return 0


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def build_parser():
    parser = argparse.ArgumentParser(
        prog="ux_feedback",
        description="Report a ux problem or usage question to the public tracker.",
    )
    sub = parser.add_subparsers(dest="command")

    def add_repo(target):
        target.add_argument(
            "--repo",
            default=os.environ.get("UX_FEEDBACK_REPO", DEFAULT_REPO),
            help=f"GitHub repository OWNER/NAME (default: {DEFAULT_REPO})",
        )

    report = sub.add_parser("report", help="compose and submit a report")
    add_repo(report)
    report.add_argument("--kind", choices=("bug", "question"), default="bug")
    report.add_argument("--title", required=True, help="one-line, searchable title")
    report.add_argument("--summary", help="what is wrong, in one or two sentences")
    report.add_argument(
        "--command", dest="ux_command",
        help="the exact ux invocation that exposes it",
    )
    report.add_argument("--expected", help="what you expected")
    report.add_argument("--actual", help="what happened instead")
    report.add_argument("--body-file", help="append extra markdown context from this file (`-` for stdin)")
    report.add_argument(
        "--raw-body",
        help="send this markdown as the whole body instead of the structured "
             "sections (`-` for stdin); env facts are appended",
    )
    report.add_argument("--include-output", help="attach a (truncated) file, e.g. captured stderr")
    report.add_argument("--probe", action="store_true", help="run ux_probe.py and attach its JSON")
    report.add_argument(
        "--timeout", type=int, default=GH_TIMEOUT,
        help=f"seconds before a gh call is abandoned as a network failure "
             f"(default: {GH_TIMEOUT})",
    )
    report.add_argument("--outbox", default=str(OUTBOX_NAME),
                        help=f"where an offline report is written (default: {OUTBOX_NAME})")
    report.add_argument("--dry-run", action="store_true", help="print the report, submit nothing")
    report.add_argument("--json", action="store_true", help="print the result as JSON")
    report.set_defaults(func=cmd_report)

    check = sub.add_parser("check", help="verify the tracker is reachable and labelled")
    add_repo(check)
    check.set_defaults(func=cmd_check)

    search = sub.add_parser("search", help="search existing issues/discussions for duplicates")
    add_repo(search)
    search.add_argument("--query", required=True)
    search.set_defaults(func=cmd_search)

    selftest = sub.add_parser("selftest", help="offline self-check")
    selftest.set_defaults(func=cmd_selftest)
    return parser


def main(argv=None):
    use_utf8_streams()
    parser = build_parser()
    args = parser.parse_args(argv)
    if not getattr(args, "command", None):
        parser.print_help()
        print("\nrun one of: report, check, search, selftest", file=sys.stderr)
        return 2
    try:
        return args.func(args)
    except FeedbackError as exc:
        print(f"ux_feedback: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
