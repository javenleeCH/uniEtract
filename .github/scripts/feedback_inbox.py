#!/usr/bin/env python3
"""Incremental feedback inbox: new issues and discussions since the last pass.

Agents file problems (issues) and questions (Q&A discussions) from the shipped
ux-extract skill. The maintainer agent runs this script between sessions to see
only what changed since it last looked, so triage does not require re-reading
the whole tracker:

  python .github/scripts/feedback_inbox.py              # new since last run
  python .github/scripts/feedback_inbox.py --mark-seen  # advance the watermark
  python .github/scripts/feedback_inbox.py --all --limit 20
  python .github/scripts/feedback_inbox.py --format json
  python .github/scripts/feedback_inbox.py --no-discussions
  python .github/scripts/feedback_inbox.py --selftest   # offline

State lives in .ux-feedback/state.json (git-ignored) and stores one updated-at
watermark per kind. `comments`/reopened items bump updated_at, so a reply on an
old report is surfaced too. Read-only unless --mark-seen is passed. Exit codes:
0 success, 1 gh/network failure, 2 usage error.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_REPO = "javenleeCH/uniEtract"
DEFAULT_STATE = ROOT / ".ux-feedback" / "state.json"
AGENT_LABEL = "agent-report"
QUESTION_CATEGORY_SLUG = "q-a"
GH_TIMEOUT = 120


class InboxError(RuntimeError):
    pass


# ---------------------------------------------------------------------------
# Timestamps and state
# ---------------------------------------------------------------------------


def parse_ts(text):
    if not text:
        return None
    value = text.strip()
    if value.endswith("Z"):
        value = value[:-1] + "+00:00"
    return datetime.fromisoformat(value)


def latest_ts(items, field="updated_at"):
    stamps = [parse_ts(item.get(field)) for item in items]
    stamps = [stamp for stamp in stamps if stamp is not None]
    return max(stamps) if stamps else None


def load_state(path):
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {"schema": 1, "issues_watermark": None, "discussions_watermark": None}
    if not isinstance(data, dict):
        return {"schema": 1, "issues_watermark": None, "discussions_watermark": None}
    data.setdefault("schema", 1)
    data.setdefault("issues_watermark", None)
    data.setdefault("discussions_watermark", None)
    return data


def save_state(path, state):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(state, ensure_ascii=False, indent=2) + "\n"
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=str(path.parent), delete=False
    ) as handle:
        handle.write(payload)
        tmp = Path(handle.name)
    tmp.replace(path)


# ---------------------------------------------------------------------------
# GitHub transport
# ---------------------------------------------------------------------------


def gh(args, payload=None, timeout=GH_TIMEOUT):
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
        raise InboxError(stderr or stdout or f"gh exited {completed.returncode}")
    return stdout


def gh_json(args, payload=None, timeout=GH_TIMEOUT):
    text = gh(args, payload, timeout)
    if not text:
        # An empty body with exit 0 is ambiguous (network hiccup, throttled
        # proxy); reporting "no new feedback" here would hide real reports.
        raise InboxError(f"gh returned an empty body for `gh {' '.join(args)}`")
    try:
        return json.loads(text)
    except ValueError as exc:
        raise InboxError(f"gh returned non-JSON output: {exc}") from exc


def repo_parts(repo):
    if repo.count("/") != 1:
        raise InboxError(f"invalid --repo `{repo}`; expected OWNER/NAME")
    return repo.split("/", 1)


def fetch_issues(repo, limit):
    data = gh_json([
        "api",
        f"repos/{repo}/issues?state=all&sort=updated&direction=desc&per_page={limit}",
    ])
    if not isinstance(data, list):
        raise InboxError("unexpected issues payload from gh api")
    return [item for item in (normalize_issue(raw) for raw in data) if item]


def graphql(query, variables):
    payload = json.dumps({"query": query, "variables": variables}).encode("utf-8")
    data = gh_json(["api", "graphql", "--input", "-"], payload)
    if isinstance(data, dict) and data.get("errors"):
        raise InboxError(json.dumps(data["errors"], ensure_ascii=False))
    return data.get("data") or {}


DISCUSSION_QUERY = (
    "query($owner:String!,$name:String!,$first:Int!){repository(owner:$owner,name:$name){"
    "discussions(first:$first,orderBy:{field:UPDATED_AT,direction:DESC}){"
    "nodes{number title url updatedAt category{name slug} author{login} "
    "comments{totalCount} answer{id}}}}}"
)


def fetch_discussions(repo, limit):
    owner, name = repo_parts(repo)
    data = graphql(DISCUSSION_QUERY, {"owner": owner, "name": name, "first": limit})
    nodes = ((data.get("repository") or {}).get("discussions") or {}).get("nodes") or []
    return [normalize_discussion(node) for node in nodes if node]


# ---------------------------------------------------------------------------
# Normalisation
# ---------------------------------------------------------------------------


def normalize_issue(raw):
    if not isinstance(raw, dict) or "pull_request" in raw:
        return None
    return {
        "kind": "issue",
        "number": raw.get("number"),
        "title": raw.get("title") or "",
        "url": raw.get("html_url") or "",
        "state": raw.get("state") or "unknown",
        "labels": sorted(
            label.get("name", "") for label in raw.get("labels") or [] if label
        ),
        "author": (raw.get("user") or {}).get("login") or "",
        "updated_at": raw.get("updated_at") or "",
        "comments": raw.get("comments") or 0,
        "category": None,
        "category_slug": None,
        "answered": None,
    }


def normalize_discussion(raw):
    if not isinstance(raw, dict):
        return None
    return {
        "kind": "discussion",
        "number": raw.get("number"),
        "title": raw.get("title") or "",
        "url": raw.get("url") or "",
        "state": "open",
        "labels": [],
        "author": (raw.get("author") or {}).get("login") or "",
        "updated_at": raw.get("updatedAt") or "",
        "comments": (raw.get("comments") or {}).get("totalCount") or 0,
        "category": (raw.get("category") or {}).get("name"),
        "category_slug": (raw.get("category") or {}).get("slug"),
        "answered": raw.get("answer") is not None,
    }


# ---------------------------------------------------------------------------
# Selection and rendering
# ---------------------------------------------------------------------------


def newer_than(item, watermark):
    if watermark is None:
        return True
    stamp = parse_ts(item.get("updated_at"))
    return stamp is not None and stamp > watermark


def select_new(items, watermark, show_all, since):
    if show_all:
        return list(items)
    threshold = since or watermark
    return [item for item in items if newer_than(item, threshold)]


def describe(item):
    flags = []
    if item["kind"] == "issue":
        flags.append(item["state"])
        if item["labels"]:
            flags.append(",".join(item["labels"]))
    else:
        if item.get("category"):
            flags.append(item["category"])
        flags.append("answered" if item.get("answered") else "unanswered")
    flags.append(f"updated={item['updated_at']}")
    flags.append(f"comments={item['comments']}")
    if item.get("author"):
        flags.append(f"by={item['author']}")
    return f"[{item['kind']:<10} #{item['number']}] " + " ".join(flags)


def render_text(repo, state_path, items, issues, discussions, watermarks, marked):
    lines = [f"repo: {repo}", f"state: {state_path}"]
    for kind in ("issues", "discussions"):
        stamp = watermarks.get(kind)
        lines.append(f"{kind} watermark: {stamp.isoformat() if stamp else '(none)'}")
    lines.append(f"new items: {len(items)}")
    lines.append("")
    for item in items:
        lines.append(describe(item))
        lines.append(f"  {item['title']}")
        lines.append(f"  {item['url']}")
        lines.append("")

    open_issues = [item for item in issues if item["state"] == "open"]
    agent_reports = [item for item in issues if AGENT_LABEL in item["labels"]]
    unanswered = [
        item for item in discussions
        if not item.get("answered") and item.get("category_slug") == QUESTION_CATEGORY_SLUG
    ]
    lines.append(
        f"fetched: {len(issues)} issues ({len(open_issues)} open, "
        f"{len(agent_reports)} {AGENT_LABEL}), {len(discussions)} discussions "
        f"({len(unanswered)} unanswered)"
    )
    if items:
        lines.append("next:")
        lines.append(f"  gh issue view <number> --repo {repo} --comments")
        lines.append(f"  gh issue comment <number> --repo {repo} --body-file reply.md")
        lines.append(f"  gh issue close <number> --repo {repo}")
        if not marked:
            lines.append("  re-run with --mark-seen once triage is recorded")
    else:
        lines.append("no new feedback; nothing to triage")
    if marked:
        lines.append("watermark advanced")
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------


def selftest(_args):
    checks = []

    def check(name, condition):
        checks.append((name, bool(condition)))

    issue = normalize_issue({
        "number": 7,
        "title": "t",
        "html_url": "https://example/7",
        "state": "open",
        "labels": [{"name": "bug"}, {"name": AGENT_LABEL}],
        "user": {"login": "agent"},
        "updated_at": "2026-11-18T10:00:00Z",
        "comments": 2,
    })
    check("issue normalises", issue and issue["labels"] == [AGENT_LABEL, "bug"])
    check("pull requests are skipped", normalize_issue({"pull_request": {}}) is None)

    discussion = normalize_discussion({
        "number": 3,
        "title": "q",
        "url": "https://example/3",
        "updatedAt": "2026-11-18T11:00:00Z",
        "category": {"name": "Q&A"},
        "author": {"login": "agent"},
        "comments": {"totalCount": 1},
        "answer": None,
    })
    check("discussion normalises", discussion and discussion["kind"] == "discussion")
    check("unanswered flag", discussion and discussion["answered"] is False)
    answered = normalize_discussion({"number": 4, "answer": {"id": "c"}})
    check("answered flag", answered and answered["answered"] is True)

    watermark = parse_ts("2026-11-18T10:30:00Z")
    check("stale item filtered", newer_than(issue, watermark) is False)
    newer = dict(issue, updated_at="2026-11-18T10:40:00Z")
    check("newer item selected", newer_than(newer, watermark) is True)

    selected = select_new([newer, issue], watermark, show_all=False, since=None)
    check("selection keeps only new", [item["number"] for item in selected] == [7])

    text = render_text(
        "owner/repo", "state.json", [newer], [newer], [],
        {"issues": watermark, "discussions": None}, False,
    )
    check("text names the repo", "owner/repo" in text)
    check("text lists the URL", "https://example/7" in text)

    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "nested" / "state.json"
        state = load_state(path)
        check("missing state starts empty", state["issues_watermark"] is None)
        state["issues_watermark"] = "2026-11-18T10:00:00Z"
        save_state(path, state)
        check("state round-trips", load_state(path)["issues_watermark"] == "2026-11-18T10:00:00Z")

    failed = [name for name, ok in checks if not ok]
    if failed:
        for name in failed:
            print(f"selftest FAILED: {name}")
        return 1
    print(f"selftest OK ({len(checks)} checks)")
    return 0


def run(args):
    repo = args.repo
    state_path = Path(args.state)
    state = load_state(state_path)
    issues, discussions = [], []
    if not args.no_issues:
        issues = fetch_issues(repo, args.limit)
    if not args.no_discussions:
        discussions = fetch_discussions(repo, args.limit)

    since = parse_ts(args.since) if args.since else None
    issue_wm = since or parse_ts(state.get("issues_watermark"))
    discussion_wm = since or parse_ts(state.get("discussions_watermark"))
    new_issues = select_new(issues, issue_wm, args.all, since)
    new_discussions = select_new(discussions, discussion_wm, args.all, since)
    items = sorted(new_issues + new_discussions, key=lambda item: item["updated_at"])

    marked = False
    if args.mark_seen:
        state["issues_watermark"] = _advance(state.get("issues_watermark"), latest_ts(issues))
        state["discussions_watermark"] = _advance(
            state.get("discussions_watermark"), latest_ts(discussions)
        )
        state["repo"] = repo
        state["last_run"] = datetime.now(timezone.utc).isoformat()
        save_state(state_path, state)
        marked = True

    if args.format == "json":
        payload = {
            "repo": repo,
            "state": str(state_path),
            "since": {
                "issues": issue_wm.isoformat() if issue_wm else None,
                "discussions": discussion_wm.isoformat() if discussion_wm else None,
            },
            "new": items,
            "fetched": {"issues": len(issues), "discussions": len(discussions)},
            "marked": marked,
        }
        print(json.dumps(payload, ensure_ascii=True, indent=2))
    else:
        print(render_text(
            repo, state_path, items, issues, discussions,
            {"issues": issue_wm, "discussions": discussion_wm}, marked,
        ))
    return 0


def _advance(previous, fetched_latest):
    if fetched_latest is None:
        return previous
    old = parse_ts(previous)
    if old is None or fetched_latest > old:
        return fetched_latest.isoformat()
    return previous


def build_parser():
    parser = argparse.ArgumentParser(
        prog="feedback_inbox",
        description="Show issues and discussions updated since the last triage pass.",
    )
    parser.add_argument(
        "--repo",
        default=os.environ.get("UX_FEEDBACK_REPO", DEFAULT_REPO),
        help=f"GitHub repository OWNER/NAME (default: {DEFAULT_REPO})",
    )
    parser.add_argument(
        "--state",
        default=os.environ.get("UX_FEEDBACK_STATE", str(DEFAULT_STATE)),
        help=f"watermark file (default: {DEFAULT_STATE})",
    )
    parser.add_argument("--limit", type=int, default=50, help="items fetched per kind (default: 50)")
    parser.add_argument("--all", action="store_true", help="ignore watermarks")
    parser.add_argument("--since", help="override watermarks with an ISO-8601 instant")
    parser.add_argument("--mark-seen", action="store_true", help="advance watermarks after printing")
    parser.add_argument("--format", choices=("text", "json"), default="text")
    parser.add_argument("--no-issues", action="store_true")
    parser.add_argument("--no-discussions", action="store_true")
    parser.add_argument("--selftest", action="store_true", help="run offline checks and exit")
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    if args.selftest:
        return selftest(args)
    try:
        return run(args)
    except (InboxError, OSError, subprocess.TimeoutExpired) as exc:
        print(f"feedback_inbox: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
