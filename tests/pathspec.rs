//! End-to-end coverage for `--path-spec` (design §4 / §5).
//!
//! Since 0.3.1 the spec language accepts only explicit `{name:re:...}` and
//! `{name:glob:...}` placeholders. Calendar/time validity is expressed by the
//! caller's regex, never inferred by `ux`.
use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn ux() -> Command {
    Command::cargo_bin("ux").unwrap()
}

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// The shape regex encodes the date constraint that `time:` used to own. It
/// deliberately does NOT validate real calendar dates or clock ranges: 02-29
/// and `T99-99-99` are accepted, 02-30 is rejected by the caller's fragment.
const SPEC: &str = "data/{ts:re:(?:\\d{4}-(?:0[13578]|1[02])-(?:0[1-9]|[12]\\d|3[01])|\\d{4}-(?:0[469]|11)-(?:0[1-9]|[12]\\d|30)|\\d{4}-02-(?:0[1-9]|1\\d|2\\d))T\\d{2}-\\d{2}-\\d{2}}_SRRG/render_service/{mins:re:\\d+}min/hidumper/txt/{file:glob:*.txt}";
const EXPR: &str = r#"re(r"NativeHeap pss:\s*(?<kb>\d+)\s*kB") | jq({kb})"#;

fn scenario(root: &Path) {
    write(
        root,
        "data/2026-11-14T19-45-49_SRRG/render_service/30min/hidumper/txt/a.txt",
        "header\nNativeHeap pss: 123 kB\n",
    );
    write(
        root,
        "data/2026-11-15T08-12-03_SRRG/render_service/5min/hidumper/txt/b.txt",
        "NativeHeap pss: 456 kB\n",
    );
    // Decoys: every one contains a matching line, but the path must not match.
    write(
        root,
        "data/junk_SRRG/render_service/30min/hidumper/txt/c.txt",
        "NativeHeap pss: 777 kB\n",
    );
    write(
        root,
        "data/2026-11-14T19-45-49_SRRG/render_service/xxmin/hidumper/txt/d.txt",
        "NativeHeap pss: 777 kB\n",
    );
    write(
        root,
        "data/2026-02-30T10-00-00_SRRG/render_service/30min/hidumper/txt/e.txt",
        "NativeHeap pss: 777 kB\n",
    );
    write(
        root,
        "data/2026-11-14T19-45-49_SRRG/render_service/30min/hidumper/raw/f.txt",
        "NativeHeap pss: 777 kB\n",
    );
}

#[test]
fn path_spec_selects_exactly_the_conforming_files_and_exposes_captures() {
    let dir = tempdir().unwrap();
    scenario(dir.path());
    let output = ux()
        .current_dir(dir.path())
        .args([
            "--path-spec",
            SPEC,
            "-f",
            "text",
            "--with-source",
            "-e",
            EXPR,
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "unexpected output: {stdout}");
    assert!(lines[0].contains("\"kb\":\"123\""), "{stdout}");
    assert!(lines[1].contains("\"kb\":\"456\""), "{stdout}");
    assert!(lines[0].contains("\"captures\""), "{stdout}");
    assert!(
        lines[0].contains("\"ts\":\"2026-11-14T19-45-49\""),
        "{stdout}"
    );
    assert!(lines[0].contains("\"mins\":\"30\""), "{stdout}");
    assert!(lines[0].contains("\"file\":\"a.txt\""), "{stdout}");
    // Captures are read-only string metadata: no derived `ts_fields` array.
    assert!(!stdout.contains("ts_fields"), "{stdout}");
    assert!(
        !stdout.contains("777"),
        "decoy paths leaked into the result"
    );
}

#[test]
fn list_shows_the_matched_inventory_with_captures() {
    let dir = tempdir().unwrap();
    scenario(dir.path());
    let output = ux()
        .current_dir(dir.path())
        .args(["--path-spec", SPEC, "--list"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), 2, "{stdout}");
    assert!(stdout.contains("\"captures\""), "{stdout}");
    assert!(stdout.contains("\"ts\""), "{stdout}");
    assert!(!stdout.contains("ts_fields"), "{stdout}");
    assert!(!stdout.contains("2026-02-30"), "{stdout}");
    assert!(!stdout.contains("junk_SRRG"), "{stdout}");
}

#[test]
fn bad_specs_fail_before_any_file_is_opened() {
    let dir = tempdir().unwrap();
    scenario(dir.path());
    for (spec, needle) in [
        (
            "data/{ts:time:%Y-%m-%dT%H-%M-%S}_SRRG/{f:glob:*.txt}",
            "unknown placeholder kind `time`",
        ),
        (
            "data/{n:lit:foo}/{f:glob:*.txt}",
            "unknown placeholder kind `lit`",
        ),
        (
            r"data/{%Y-%m-%d}/{*.txt}",
            "must be {name:re:...} or {name:glob:...}",
        ),
        ("data//{f:glob:*.txt}", "empty segment"),
        (r"data/{a:re:\d+}/{a:re:\d+}.txt", "duplicate capture name"),
    ] {
        ux().current_dir(dir.path())
            .args(["--path-spec", spec, "-e", "text()"])
            .assert()
            .code(2)
            .stdout("")
            .stderr(predicate::str::contains("invalid --path-spec"))
            .stderr(predicate::str::contains(needle));
    }
}

#[test]
fn path_spec_and_positional_paths_are_mutually_exclusive() {
    let dir = tempdir().unwrap();
    scenario(dir.path());
    ux().current_dir(dir.path())
        .args(["--path-spec", SPEC, "-e", "text()", "data"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn directory_ending_spec_does_not_silently_append_a_globstar() {
    let dir = tempdir().unwrap();
    scenario(dir.path());
    let dir_spec = SPEC.strip_suffix("/{file:glob:*.txt}").unwrap();
    ux().current_dir(dir.path())
        .args(["--path-spec", dir_spec, "-f", "text", "-e", "text()"])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn shorthand_spec_is_a_usage_error() {
    let dir = tempdir().unwrap();
    write(dir.path(), "data/2026-11-14/a.txt", "hit\n");
    ux().current_dir(dir.path())
        .args([
            "--path-spec",
            r"data/{%Y-%m-%d}/{*.txt}",
            "-f",
            "text",
            "-e",
            r#"re(r"hit")"#,
        ])
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicate::str::contains("invalid --path-spec"));
}

#[test]
fn captures_stage_injects_read_only_captures_on_demand() {
    let dir = tempdir().unwrap();
    scenario(dir.path());
    // captures() is the opt-in path from path-spec metadata into the pipeline.
    ux().current_dir(dir.path())
        .args([
            "--path-spec",
            SPEC,
            "-f",
            "text",
            "-O",
            "text",
            "-e",
            r#"jq(select(. != null)) | captures() | jq(.ts)"#,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("2026-11-14T19-45-49"))
        .stdout(predicate::str::contains("2026-11-15T08-12-03"));
}

#[test]
fn calendar_and_clock_validity_are_delegated_to_the_regex() {
    let dir = tempdir().unwrap();
    write(
        dir.path(),
        "data/2026-02-29T99-99-99_SRRG/render_service/30min/hidumper/txt/a.txt",
        "NativeHeap pss: 1 kB\n",
    );
    // 0.3.1 performs no calendar/time validation: 02-29 and T99-99-99 are
    // accepted. 02-30 is rejected only because SPEC's shape regex excludes it.
    ux().current_dir(dir.path())
        .args(["--path-spec", SPEC, "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("2026-02-29T99-99-99_SRRG"));
}

#[test]
fn sort_by_capture_numeric_fixes_dictionary_order() {
    let dir = tempdir().unwrap();
    // mins=2 and mins=10: lexicographic order is 10 < 2, numeric order is 2 < 10.
    let spec =
        "data/{ts:glob:*}_SRRG/render_service/{mins:re:\\d+}min/hidumper/txt/{file:glob:*.txt}";
    for (mins, file) in [("10", "a.txt"), ("2", "b.txt")] {
        write(
            dir.path(),
            &format!("data/2026-11-14T00-00-00_SRRG/render_service/{mins}min/hidumper/txt/{file}"),
            "NativeHeap pss: 1 kB\n",
        );
    }

    // Default: lexicographic → 10min before 2min.
    let default = ux()
        .current_dir(dir.path())
        .args(["--path-spec", spec, "--list"])
        .output()
        .unwrap();
    let default_stdout = String::from_utf8_lossy(&default.stdout);
    let pos10 = default_stdout.find("10min").unwrap();
    let pos2 = default_stdout.find("2min").unwrap();
    assert!(pos10 < pos2, "lexicographic default: {default_stdout}");

    // --sort-by mins:num → 2min before 10min.
    let sorted = ux()
        .current_dir(dir.path())
        .args(["--path-spec", spec, "--list", "--sort-by", "mins:num"])
        .output()
        .unwrap();
    let sorted_stdout = String::from_utf8_lossy(&sorted.stdout);
    let pos10 = sorted_stdout.find("10min").unwrap();
    let pos2 = sorted_stdout.find("2min").unwrap();
    assert!(pos2 < pos10, "numeric sort: {sorted_stdout}");
}
