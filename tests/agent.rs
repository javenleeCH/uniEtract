//! Executable contract for developers and AI agents.
//!
//! These tests pin the parts of the CLI an automation caller depends on:
//! stdout purity, structured stderr, stable exit codes, input inventory,
//! help/completions, and hard-limit behaviour.

use std::fs;

use assert_cmd::Command;
use serde_json::Value as JsonValue;
use tempfile::tempdir;

fn ux() -> Command {
    Command::cargo_bin("ux").unwrap()
}

fn json_lines(bytes: &[u8]) -> Vec<JsonValue> {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("each diagnostic line must be JSON"))
        .collect()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn error_format_json_reports_usage_errors_as_one_object() {
    let assert = ux()
        .args([
            "-e",
            "text()",
            "-O",
            "text",
            "--with-source",
            "--error-format",
            "json",
        ])
        .assert()
        .code(2);
    let output = assert.get_output();
    assert!(output.stdout.is_empty(), "stdout must carry data only");
    let diagnostics = json_lines(&output.stderr);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["code"], "usage");
    assert_eq!(diagnostics[0]["exit_code"], 2);
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .unwrap()
            .contains("mutually exclusive")
    );
}

#[test]
fn error_format_json_reports_compile_and_runtime_errors() {
    let compile = ux()
        .args(["-e", "jq(. |", "-f", "json", "--error-format", "json"])
        .write_stdin("{}")
        .assert()
        .code(2)
        .get_output()
        .clone();
    let diagnostic = json_lines(&compile.stderr);
    assert_eq!(diagnostic[0]["code"], "query");
    assert_eq!(diagnostic[0]["exit_code"], 2);

    let runtime = ux()
        .args(["-f", "xml", "-e", "text()", "--error-format", "json"])
        .write_stdin("<root><broken></root>")
        .assert()
        .code(1)
        .get_output()
        .clone();
    let diagnostic = json_lines(&runtime.stderr);
    assert_eq!(diagnostic[0]["code"], "runtime");
    assert_eq!(diagnostic[0]["exit_code"], 1);
    assert!(
        diagnostic[0]["message"]
            .as_str()
            .unwrap()
            .contains("invalid XML")
    );
}

#[test]
fn eval_diagnostics_carry_stage_and_origin() {
    let output = ux()
        .args([
            "-f",
            "json",
            "-e",
            r#"jq(error("boom"))"#,
            "--error-format",
            "json",
        ])
        .write_stdin("null")
        .assert()
        .code(1)
        .get_output()
        .clone();
    let diagnostic = json_lines(&output.stderr);
    assert_eq!(diagnostic[0]["code"], "eval");
    assert_eq!(diagnostic[0]["stage"], "jq");
    assert_eq!(diagnostic[0]["path"], JsonValue::Null);
    assert!(diagnostic[0].get("line").is_some());
    assert!(diagnostic[0].get("record").is_some());
    assert!(diagnostic[0]["message"].as_str().unwrap().contains("boom"));
}

#[test]
fn json_error_format_keeps_stdout_parseable() {
    let output = ux()
        .args([
            "-f",
            "jsonl",
            "-e",
            "jq(.)",
            "--compact",
            "--error-format",
            "json",
        ])
        .write_stdin("1\n2\n")
        .assert()
        .success()
        .get_output()
        .clone();
    let values = json_lines(&output.stdout);
    assert_eq!(values.len(), 2);
    assert_eq!(values[0], 1);
    assert_eq!(values[1], 2);
    let diagnostics = json_lines(&output.stderr);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["code"], "warning");
    assert_eq!(diagnostics[0]["exit_code"], 0);
}

#[test]
fn quiet_suppresses_warnings_but_never_errors() {
    let dir = tempdir().unwrap();
    let pattern = format!("{}/*.none", dir.path().to_str().unwrap());
    let quiet = ux()
        .args(["-e", "text()", "--quiet", &pattern])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(quiet.stderr.is_empty());
    assert!(quiet.stdout.is_empty());

    let failure = ux()
        .args([
            "-e",
            "text()",
            "--exclude",
            "[",
            "--quiet",
            "--error-format",
            "json",
        ])
        .assert()
        .code(2)
        .get_output()
        .clone();
    let diagnostics = json_lines(&failure.stderr);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["code"], "usage");
}

#[test]
fn warnings_are_structured_when_error_format_is_json() {
    let dir = tempdir().unwrap();
    let pattern = format!("{}/*.none", dir.path().to_str().unwrap());
    let output = ux()
        .args(["-e", "text()", "--error-format", "json", &pattern])
        .assert()
        .success()
        .get_output()
        .clone();
    let diagnostics = json_lines(&output.stderr);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["code"], "warning");
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .unwrap()
            .contains("no input files matched")
    );
}

#[test]
fn list_outputs_a_stable_inventory_without_a_pipeline() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("b.json"), "not json at all").unwrap();
    fs::write(dir.path().join("a.json"), "also not json").unwrap();
    let output = ux()
        .args(["--list", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .clone();
    let entries = json_lines(&output.stdout);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["type"], "file");
    assert_eq!(entries[1]["type"], "file");
    assert!(entries[0]["path"].as_str().unwrap().ends_with("a.json"));
    assert!(entries[1]["path"].as_str().unwrap().ends_with("b.json"));
    assert!(!entries[0]["path"].as_str().unwrap().contains('\\'));
    assert!(output.stderr.is_empty());
}

#[test]
fn list_marks_stdin_and_does_not_open_files() {
    let output = ux()
        .args(["--list", "-"])
        .assert()
        .success()
        .get_output()
        .clone();
    let entries = json_lines(&output.stdout);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["type"], "stdin");
    assert_eq!(entries[0]["path"], JsonValue::Null);
}

#[test]
fn list_appends_an_extension_derived_format_hint() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.json"), "not json").unwrap();
    fs::write(dir.path().join("b.csv"), "not csv").unwrap();
    fs::write(dir.path().join("extensionless"), "not jsonl").unwrap();

    let output = ux()
        .args(["--list", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .clone();
    let entries = json_lines(&output.stdout);
    assert_eq!(entries.len(), 3);
    // Extension-derived hints (content-free: --list never opens files).
    let a = entries
        .iter()
        .find(|e| e["path"].as_str().unwrap().ends_with("a.json"))
        .unwrap();
    assert_eq!(a["format"], "json");
    let b = entries
        .iter()
        .find(|e| e["path"].as_str().unwrap().ends_with("b.csv"))
        .unwrap();
    assert_eq!(b["format"], "csv");
    // Unknown extension: field is omitted rather than guessed.
    let c = entries
        .iter()
        .find(|e| e["path"].as_str().unwrap().ends_with("extensionless"))
        .unwrap();
    assert!(c.get("format").is_none(), "{c}");

    // Explicit -f overrides the hint for every entry.
    let forced = ux()
        .args(["--list", "-f", "text", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .clone();
    for entry in json_lines(&forced.stdout) {
        assert_eq!(entry["format"], "text");
    }
}

#[test]
fn completions_script_is_generated_without_other_arguments() {
    let bash = ux()
        .args(["--completions", "bash"])
        .assert()
        .success()
        .get_output()
        .clone();
    let script = text(&bash.stdout);
    assert!(script.contains("_ux"));
    assert!(script.contains("complete"));
    assert!(bash.stderr.is_empty());

    let powershell = ux()
        .args(["--completions", "powershell"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(text(&powershell.stdout).contains("Register-ArgumentCompleter"));
}

#[test]
fn help_documents_the_agent_contract() {
    let long = ux()
        .args(["--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let help = text(&long.stdout);
    for expected in [
        "AGENT CONTRACT",
        "STAGES",
        "EXAMPLES",
        "--error-format",
        "--quiet",
        "--list",
        "--completions",
        "stdin",
    ] {
        assert!(help.contains(expected), "--help is missing {expected}");
    }
    assert!(
        help.contains("Usage: ux "),
        "help must use a stable bin name on every platform"
    );
    let short = ux().args(["-h"]).assert().success().get_output().clone();
    assert!(text(&short.stdout).contains("Examples:"));
}

#[test]
fn unbound_xpath_prefix_is_a_query_error_not_a_panic() {
    let output = ux()
        .args([
            "-f",
            "xml",
            "-e",
            r#"xpath("//x:book")"#,
            "--error-format",
            "json",
        ])
        .write_stdin(r#"<root xmlns:x="urn:books"><x:book/></root>"#)
        .assert()
        .code(2)
        .get_output()
        .clone();
    let diagnostic = json_lines(&output.stderr);
    assert_eq!(diagnostic[0]["code"], "query");
    assert!(
        diagnostic[0]["message"]
            .as_str()
            .unwrap()
            .contains("not bound")
    );
}

#[test]
fn namespace_binding_matches_by_uri() {
    ux().args([
        "--ns",
        "b=urn:books",
        "-f",
        "xml",
        "-O",
        "text",
        "-e",
        r#"xpath("//b:book") | attr("sku")"#,
    ])
    .write_stdin(r#"<root xmlns="urn:books"><book sku="NS-1"/></root>"#)
    .assert()
    .success()
    .stdout("NS-1\n");
}

#[test]
fn max_results_stops_immediately_across_files() {
    let dir = tempdir().unwrap();
    let first = dir.path().join("a.jsonl");
    let second = dir.path().join("b.jsonl");
    fs::write(&first, "1\n2\n").unwrap();
    fs::write(&second, "3\n4\n").unwrap();
    let output = ux()
        .args([
            "-f",
            "jsonl",
            "-O",
            "text",
            "--max-results",
            "1",
            "-e",
            "jq(.)",
            first.to_str().unwrap(),
            second.to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert_eq!(text(&output.stdout), "1\n");
    assert_eq!(
        text(&output.stderr).matches("--max-results").count(),
        1,
        "the limit diagnostic must be emitted exactly once"
    );
}

#[test]
fn max_results_never_emits_a_partial_json_aggregate() {
    let output = ux()
        .args([
            "-f",
            "jsonl",
            "-O",
            "json",
            "--max-results",
            "1",
            "-e",
            "jq(.)",
        ])
        .write_stdin("1\n2\n")
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(output.stdout.is_empty());
    let limited = ux()
        .args([
            "-f",
            "jsonl",
            "--max-results",
            "1",
            "-e",
            "jq(.)",
            "--error-format",
            "json",
        ])
        .write_stdin("1\n2\n")
        .assert()
        .code(1)
        .get_output()
        .clone();
    let diagnostic = json_lines(&limited.stderr);
    assert_eq!(diagnostic[0]["code"], "limit_exceeded");
    assert_eq!(diagnostic[0]["exit_code"], 1);
}

#[test]
fn transcoded_input_respects_max_input_size_before_decoding() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("utf16.txt");
    let mut bytes = vec![0xFF, 0xFE];
    for unit in "hello world".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&path, bytes).unwrap();
    let output = ux()
        .args([
            "--encoding",
            "utf-16le",
            "--max-input-size",
            "4",
            "-f",
            "text",
            "-e",
            "text()",
            path.to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(output.stdout.is_empty());
    assert!(text(&output.stderr).contains("--max-input-size"));
}

#[test]
fn zero_matches_is_success_with_clean_stderr() {
    let output = ux()
        .args(["-f", "text", "-O", "text", "-e", r#"re(r"ZZZ")"#])
        .write_stdin("abc\n")
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn file_zero_matches_emits_a_suppressible_summary() {
    // A genuine zero match over *file* input is the highest-frequency silent
    // failure for agents; it must be observable on stderr (still exit 0).
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "abc\n").unwrap();

    let output = ux()
        .args([
            "-f",
            "text",
            "-O",
            "text",
            "-e",
            r#"re(r"ZZZ")"#,
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stdout.is_empty());
    assert!(
        text(&output.stderr).contains("0 results"),
        "{:?}",
        text(&output.stderr)
    );

    // --quiet suppresses the summary (still exit 0, still data-only stdout).
    let quiet = ux()
        .args([
            "-f",
            "text",
            "-O",
            "text",
            "--quiet",
            "-e",
            r#"re(r"ZZZ")"#,
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(quiet.stdout.is_empty());
    assert!(quiet.stderr.is_empty(), "{:?}", text(&quiet.stderr));
}

#[test]
fn version_is_discoverable() {
    ux().args(["--version"])
        .assert()
        .success()
        .stdout(predicates::str::starts_with("ux "));
}

#[test]
fn no_stdin_rejects_implicit_stdin() {
    let output = ux()
        .args(["-e", "text()", "--no-stdin", "--error-format", "json"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    let diagnostic = json_lines(&output.stderr);
    assert_eq!(diagnostic[0]["code"], "usage");
    assert!(diagnostic[0]["message"].as_str().unwrap().contains("stdin"));
}

#[test]
fn dry_run_alias_lists_inputs() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.json"), "{}").unwrap();
    let output = ux()
        .args(["--dry-run", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(json_lines(&output.stdout).len(), 1);
}

#[test]
fn failed_json_run_does_not_emit_partial_array() {
    let dir = tempdir().unwrap();
    let good = dir.path().join("good.json");
    let bad = dir.path().join("bad.json");
    fs::write(&good, "{\"a\":1}").unwrap();
    fs::write(&bad, "{oops").unwrap();
    let output = ux()
        .args([
            "-f",
            "json",
            "-O",
            "json",
            "-e",
            "jq(.)",
            good.to_str().unwrap(),
            bad.to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(
        output.stdout.is_empty(),
        "a failed run must discard its partial aggregate"
    );
    assert!(text(&output.stderr).contains("invalid JSON"));
}

#[test]
fn list_is_byte_stable_and_error_codes_are_stable() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.json"), "{}").unwrap();
    fs::write(dir.path().join("b.json"), "{}").unwrap();
    let mut snapshots = Vec::new();
    for _ in 0..3 {
        snapshots.push(
            ux().args(["--list", dir.path().to_str().unwrap()])
                .assert()
                .success()
                .get_output()
                .stdout
                .clone(),
        );
    }
    assert_eq!(snapshots[0], snapshots[1]);
    assert_eq!(snapshots[1], snapshots[2]);

    let mut codes = Vec::new();
    for _ in 0..3 {
        let output = ux()
            .args([
                "-e",
                "text()",
                "-O",
                "text",
                "--with-source",
                "--error-format",
                "json",
            ])
            .assert()
            .code(2)
            .get_output()
            .clone();
        codes.push(json_lines(&output.stderr)[0]["code"].clone());
    }
    assert!(codes.iter().all(|code| code == "usage"));
}
