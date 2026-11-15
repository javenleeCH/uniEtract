//! End-to-end coverage for `--path-spec` (design §4 / §5).
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

const SPEC: &str = "data/{ts:time:%Y-%m-%dT%H-%M-%S}_SRRG/render_service/{mins:re:\\d+}min/hidumper/txt/{file:glob:*.txt}";
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
    assert_eq!(output.status.code(), Some(0), "{:?}", output);
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
    assert!(
        lines[0].contains("\"ts_fields\":[2026,11,14,19,45,49]"),
        "{stdout}"
    );
    assert!(lines[0].contains("\"mins\":\"30\""), "{stdout}");
    assert!(lines[0].contains("\"file\":\"a.txt\""), "{stdout}");
    assert!(
        !stdout.contains("777"),
        "decoy paths leaked into the result"
    );
}

#[test]
fn list_shows_the_matched_inventory_with_captures() {
    let dir = tempdir().unwrap();
    scenario(dir.path());
    ux().current_dir(dir.path())
        .args(["--path-spec", SPEC, "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"captures\""))
        .stdout(predicate::str::contains("\"ts_fields\""))
        .stdout(predicate::str::contains("2026-02-30").not())
        .stdout(predicate::str::contains("junk_SRRG").not());
}

#[test]
fn bad_specs_fail_before_any_file_is_opened() {
    let dir = tempdir().unwrap();
    scenario(dir.path());
    for (spec, needle) in [
        (
            "data/{ts:time:%Y-%m-%dT%H:%M:%S}_SRRG/{f:glob:*.txt}",
            "cannot contain a colon",
        ),
        (
            "data/{ts:time:%Y-%q}_SRRG/{f:glob:*.txt}",
            "unknown time directive",
        ),
        ("data//{f:glob:*.txt}", "empty segment"),
        ("data/{a:re:\\d+}/{a:re:\\d+}.txt", "duplicate capture name"),
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
    ux().current_dir(dir.path())
        .args([
            "--path-spec",
            "data/{ts:time:%Y-%m-%dT%H-%M-%S}_SRRG/render_service/{mins:re:\\d+}min/hidumper/txt",
            "-f",
            "text",
            "-e",
            "text()",
        ])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn shorthand_spec_is_accepted() {
    let dir = tempdir().unwrap();
    write(dir.path(), "data/2026-11-14/a.txt", "hit\n");
    write(dir.path(), "data/junk/a.txt", "miss\n");
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
        .success()
        .stdout(predicate::str::contains("hit"))
        .stdout(predicate::str::contains("miss").not());
}
