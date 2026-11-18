---
name: ux-extract
description: This skill should be used when an agent needs to extract structured data from local files, directories, logs, JSON/JSONL/YAML/TOML/XML/HTML/CSV/TSV or stdin using the ux (UniExtract) CLI, or when a task would otherwise be solved with ad-hoc jq, grep, regex or Python parsing scripts. Covers composing the 9-stage pipeline, previewing inputs with --list, selecting files with --path-spec, and interpreting stable JSON diagnostics and 0/1/2 exit codes.
---

# ux-extract

## Overview

`ux` is a single-process, serial Rust CLI that composes jq, XPath and regex stages over one record stream. It writes data only to stdout and one diagnostic per line to stderr, which makes it safe for scripts, CI and agents. Treat the CLI as the only stable interface; do not rely on the Rust library API.

## When to use

- Extract or reshape fields from JSON, JSONL, YAML, TOML, CSV/TSV, XML, HTML or text/log files.
- Pull matches from logs with regex, or select XML/HTML nodes with XPath.
- Select files by structured path before reading them (`--path-spec`), e.g. timestamped dump directories.
- Aggregate across many files, attach provenance with `--with-source`, or preview an input set without opening files.
- Replace a hand-written jq/grep/regex/Python one-liner with a deterministic, diagnosable command.

## Prerequisites

1. Locate the binary: prefer `ux` on PATH; otherwise `target/release/ux` (`ux.exe` on Windows) after `cargo build --release --locked`.
2. Run `python <skill-directory>/scripts/ux_probe.py` from the working directory that contains the target files. Read its JSON report; stop and fix the environment when `ok` is false. The script resolves `ux` on PATH or a local `target/release|debug` build.
3. Read `ux --help` for the stage reference when the pipeline syntax is uncertain.

## Workflow

### 1. Preflight

Run the probe script. Never assume a version from `--version`: the working tree can implement the next version's behavior while still reporting the old number. Branch on `--help` and observed behavior instead.

### 2. Establish the input set first

Run `ux --list <paths>` or `ux --list --path-spec <spec>` and inspect the JSONL inventory. This opens no file contents and needs no `-e`. Add `--no-stdin` in CI or agent loops so a missing path can never block on a terminal.

Choose `--path-spec` when paths themselves carry semantics; read `references/path-spec.md` before writing a spec.

### 3. Compose the pipeline

Pick stages from `references/cli-contract.md` and start from a recipe in `references/recipes.md`. Always pass `-e` as one quoted argument: the `|` inside it separates ux stages and must never be interpreted by the shell.

### 4. Execute with machine-readable diagnostics

Add `--error-format json` so every stderr line is a parseable object. Add `--quiet` to silence non-result-affecting warnings (encoding fallback, duplicate `--ns`, etc.); result-affecting warnings — skipped/empty/missing inputs and zero matches — always print with `"category":"result_set"`. Keep stdout and stderr separate; never merge them with `2>&1`.

### 5. Interpret the result

Read `code` and `exit_code` from stderr, not human wording. Use the decision table in `references/cli-contract.md` when stdout is empty. On `usage` or `query`, fix arguments or pipeline syntax; on `runtime`, `io` or `eval`, inspect the data; on `limit_exceeded`, raise `--max-results` or narrow the input.

## Hard rules

- Keep stdout as pure data. Do not combine `--with-source` with `-O text` (mutually exclusive, `Usage(2)`).
- Specify `-f` explicitly for extension-less JSONL, YAML, TOML, CSV/TSV; `-f auto` misdetects extension-less JSONL.
- Use `-O jsonl` for large inputs; `-O json` and `slurp()` buffer everything before the first byte.
- Add `--line-buffered` when a long-lived pipe needs output before EOF.
- Treat `--max-results` as a hard error (exit 1), not a silent truncation.
- Bind XPath prefixes with `--ns PREFIX=URI`; unbound prefixes fail at compile time with `Query(2)`.
- On Windows Git Bash / MSYS2, set `MSYS2_ARG_CONV_EXCL='*'` or use PowerShell; otherwise `//a` inside `-e` becomes a UNC path and XPath silently matches nothing.
- Do not treat an input-side broken pipe as silence: since 0.4.0 it is a `runtime(1)` diagnostic with `rc=1`. `rc=0` with empty stdout and empty stderr means a genuine zero match over stdin; over file inputs a zero match prints a `processed N file(s), 0 results` warning (result-affecting, survives `--quiet`).
- Do not invent new flags or stages beyond the shipped CLI; the shipped selection surface is `--path-spec`, `--path-regex`/`--root`, and the shipped stages include `captures()` (explicit capture injection) and `check(FILTER)` (per-record assertion).
- Do not silently work around a tool defect or an ambiguous semantic. Report it once per distinct problem (see below), then continue with the documented workaround.

## Reporting problems and questions

`ux` is maintained from public reports: every reproducible bug and every unclear semantic that is filed becomes a fix, a contract-doc sentence, or a release note. When the preflight, `--help`, `references/cli-contract.md` and `--error-format json` still cannot explain what you see, file one report instead of guessing or silently bypassing:

```powershell
python <skill-directory>/scripts/ux_feedback.py report --kind bug --title "..." `
  --summary "..." --command "ux -e ..." --expected "..." --actual "..." --probe
```

- `--kind bug` files a GitHub **issue** (reproducible defect: wrong output, wrong exit code, silent drop, contract contradiction).
- `--kind question` files a **Q&A discussion** (semantics, boundaries, "is this intended?").
- Run `ux_feedback.py search --query TEXT` first and comment on an existing report instead of duplicating it.
- The script submits through `gh`; without network or `gh` it writes the full report to `.ux-feedback/outbox/` and prints the exact submit command. Exit code `3` means "prepared, not submitted": hand the file to the human.
- Never include credentials, personal data or proprietary file contents. Use a minimal synthetic sample.
- Details and the four things a report must state: `references/feedback.md`. Maintainer-side triage: repository `docs/feedback-loop.md`.

## Bundled resources

| Resource | When to load |
|---|---|
| `scripts/ux_probe.py` | Run at the start of every task; deterministic preflight with a JSON report |
| `scripts/ux_feedback.py` | File a problem report (issue) or usage question (discussion); `search` for duplicates first |
| `scripts/install.py` | Human-only: installs this skill into another agent. Do not run during extraction tasks |
| `INSTALL.md` | Human-only installation guide bundled in the zip. Do not load for extraction tasks |
| `references/feedback.md` | How to write, dedupe and submit a report or question; privacy rules |
| `references/cli-contract.md` | Flags, stages, formats, diagnostics, exit codes, decision table, traps |
| `references/path-spec.md` | Writing or debugging `--path-spec`, captures, pruning, NativeHeap example |
| `references/recipes.md` | Copy-paste commands by input type and task |

## Troubleshooting

- Zero matches with exit 0: re-run `ux --list`, check `-f`, and check HTML implicit `tbody` and lowercase tag names. A `empty file (no records)` warning means a 0-byte file was opened (reconcile as empty, not missing); nested named groups in `re:` fragments join captures since 0.4.0.
- `Usage(2)` before any file opens: validate `--path-spec` placeholders and argument combinations.
- `query` / `Query(2)`: check stage names, raw-string delimiters and XPath prefix bindings.
- Warning `assumed GB18030`: pass an explicit `--encoding` for deterministic transcoding.
- `rc=1` with partial jsonl output: non-strict mode continues after a file error; inspect the last diagnostic and add `--strict` if partial results are unacceptable.
