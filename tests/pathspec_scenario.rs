//! Reproducible NativeHeap path-matching scenario.
//!
//! These 8 assertions replace the retired Python prototype. The fixture tree
//! under `tests/fixtures/pathmatch/data` plus the Rust tables below are the
//! single source of truth: no Python interpreter, no shell globbing, no CWD
//! drift.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value as JsonValue;

const FIXTURE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pathmatch/data");
const RUN_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pathmatch");

const A1: &str = "data/2026-11-14T19-45-49_SRRG/render_service/30min/hidumper/txt";
const A2: &str = "data/2026-11-15T08-12-03_SRRG/render_service/45min/hidumper/txt";
const A3: &str = "data/2026-11-16T23-59-59_SRRG/render_service/5min/hidumper/txt";

/// Explicit `re:` encodes the date shape; 0.3.1 has no calendar validation.
const SPEC: &str = "data/{ts:re:(?:\\d{4}-(?:0[13578]|1[02])-(?:0[1-9]|[12]\\d|3[01])|\\d{4}-(?:0[469]|11)-(?:0[1-9]|[12]\\d|30)|\\d{4}-02-(?:0[1-9]|1\\d|2\\d))T\\d{2}-\\d{2}-\\d{2}}_SRRG/render_service/{mins:re:\\d+}min/hidumper/txt/{file:glob:*.txt}";
const EXPR: &str = r#"re(r"NativeHeap pss:\s*(?<kb>\d+)\s*kB") | jq({kb})"#;

fn ux() -> Command {
    Command::cargo_bin("ux").unwrap()
}

fn hit(kb: &str, tag: &str) -> String {
    format!(
        concat!(
            "----- render_service hidumper meminfo -----\n",
            "                   PSS   Shared  Private   Heap\n",
            "                  (KB)   Dirty    Dirty   Size\n",
            "                ------  ------   ------  ------\n",
            "  NativeHeap pss: {kb} kB\n",
            "  Dalvik Heap pss: 2048 kB\n",
            "  .so mmap pss: 4096 kB\n",
            "TOTAL: {kb} kB  tag={tag}\n"
        ),
        kb = kb,
        tag = tag
    )
}

const MISS: &str = concat!(
    "----- render_service hidumper meminfo -----\n",
    "nativeheap pss: 999 kB\n",
    "NativeHeap: 111 kB\n",
    "NativeHeap pss 888 kB\n",
    "NativeHeap pss: 777 MB\n",
    "TOTAL: 0 kB\n"
);

/// (relative path, exact bytes, official SPEC should match, extracted kB)
fn fixtures() -> Vec<(String, String, bool, Option<&'static str>)> {
    vec![
        (
            format!("{A1}/dump1.txt"),
            hit("12345", "A1"),
            true,
            Some("12345"),
        ),
        (
            format!("{A2}/dump2.txt"),
            hit("67890", "A2"),
            true,
            Some("67890"),
        ),
        (
            format!("{A3}/dump3.txt"),
            hit("321", "A3"),
            true,
            Some("321"),
        ),
        (format!("{A1}/dump4.txt"), MISS.to_owned(), true, None),
        (
            "data/2026-11-14T19-45-49_SRRG/render_service/30min/hidumper/raw/dump5.txt".to_owned(),
            hit("77777", "B1-raw-instead-of-txt"),
            false,
            None,
        ),
        (
            format!("{A1}/summary.csv"),
            hit("77777", "B2-not-txt-extension"),
            false,
            None,
        ),
        (
            format!("{A1}/nested/dump6.txt"),
            hit("77777", "B3-one-extra-level"),
            false,
            None,
        ),
        (
            "data/2026-11-14T19-45-49_SRRG/render_service/xxmin/hidumper/txt/dump7.txt".to_owned(),
            hit("77777", "B4-non-numeric-min"),
            false,
            None,
        ),
        (
            "data/2026-11-14T19-45-49_SRRG/render_service/30min/debug/dump8.txt".to_owned(),
            hit("77777", "B5-debug-instead-of-hidumper"),
            false,
            None,
        ),
        (
            "data/junk_SRRG/render_service/30min/hidumper/txt/dump9.txt".to_owned(),
            hit("77777", "B6-no-timestamp"),
            false,
            None,
        ),
        (
            "data/2026-02-30T10-00-00_SRRG/render_service/30min/hidumper/txt/dump10.txt".to_owned(),
            hit("77777", "B7-impossible-date"),
            false,
            None,
        ),
        (
            "data/2026-11-14T19-45-49_SRRG/render_service/-5min/hidumper/txt/dump11.txt".to_owned(),
            hit("77777", "B8-negative-min"),
            false,
            None,
        ),
    ]
}

fn collect_tree(root: &Path, base: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let relative = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, base, &mut out);
    out
}

fn list(args: &[&str]) -> (u8, String, String) {
    let output = ux().current_dir(RUN_ROOT).args(args).output().unwrap();
    (
        output.status.code().unwrap_or(255) as u8,
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn inventory_lines(stdout: &str) -> Vec<JsonValue> {
    stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn scenario_1_fixture_tree_is_exactly_twelve_files_with_expected_bytes() {
    let expected: BTreeMap<String, Vec<u8>> = fixtures()
        .into_iter()
        .map(|(path, bytes, _, _)| (path, bytes.into_bytes()))
        .collect();
    let actual = collect_tree(Path::new(FIXTURE_ROOT), Path::new(RUN_ROOT));
    assert_eq!(actual.len(), 12, "fixture tree drifted");
    assert_eq!(actual, expected, "fixture bytes drifted");
}

#[test]
fn scenario_2_legacy_time_lit_and_shorthand_specs_are_usage_errors() {
    for spec in [
        "data/{ts:time:%Y-%m-%dT%H-%M-%S}_SRRG/render_service/{mins:re:\\d+}min/hidumper/txt/{file:glob:*.txt}",
        "data/{n:lit:foo}/{file:glob:*.txt}",
        r"data/{%Y-%m-%dT%H-%M-%S}_SRRG/{*.txt}",
    ] {
        let (code, stdout, stderr) = list(&["--path-spec", spec, "--list"]);
        assert_eq!(code, 2, "spec={spec} stdout={stdout} stderr={stderr}");
        assert!(stdout.is_empty(), "spec={spec} stdout={stdout}");
        assert!(stderr.contains("invalid --path-spec"), "{stderr}");
    }
}

#[test]
fn scenario_3_directory_ending_spec_does_not_append_an_implicit_globstar() {
    let dir_spec = SPEC.strip_suffix("/{file:glob:*.txt}").unwrap();
    let (code, stdout, stderr) = list(&["--path-spec", dir_spec, "--list"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.is_empty(),
        "directory tail matched files below: {stdout}"
    );
}

#[test]
fn scenario_4_official_spec_matches_exactly_four_of_twelve_files() {
    let (code, stdout, stderr) = list(&["--path-spec", SPEC, "--list"]);
    assert_eq!(code, 0, "{stderr}");
    let rows = inventory_lines(&stdout);
    assert_eq!(rows.len(), 4, "{stdout}");
    let mut names: Vec<String> = rows
        .iter()
        .map(|row| {
            PathBuf::from(row["path"].as_str().unwrap())
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    // Ordering is by the full normalized relative path, so dump1/dump4 (same
    // directory) come before dump2/dump3. Compare as a set here.
    names.sort();
    assert_eq!(
        names,
        ["dump1.txt", "dump2.txt", "dump3.txt", "dump4.txt"],
        "{stdout}"
    );
    for (path, _, _, _) in fixtures().into_iter().filter(|(_, _, hit, _)| !hit) {
        let name = Path::new(&path).file_name().unwrap().to_string_lossy();
        assert!(!stdout.contains(name.as_ref()), "decoy leaked: {path}");
    }
}

#[test]
fn scenario_5_explicit_regex_rejects_the_impossible_date_decoy() {
    let (code, stdout, stderr) = list(&["--path-spec", SPEC, "--list"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(!stdout.contains("2026-02-30"), "{stdout}");
    assert!(!stdout.contains("dump10.txt"), "{stdout}");
    // The rejection is the caller's regex, not a built-in calendar check.
    let permissive = "data/{ts:re:\\d{4}-\\d{2}-\\d{2}T\\d{2}-\\d{2}-\\d{2}}_SRRG/render_service/{mins:re:\\d+}min/hidumper/txt/{file:glob:*.txt}";
    let (code, stdout, stderr) = list(&["--path-spec", permissive, "--list"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("2026-02-30"), "{stdout}");
}

#[test]
fn scenario_6_segment_pruning_never_enters_unmatched_directories() {
    let dir = tempfile::tempdir().unwrap();
    for relative in [
        "data/2026-11-14T19-45-49_SRRG/render_service/30min/hidumper/txt/keep.txt",
        "data/junk_SRRG/render_service/30min/hidumper/txt/leak1.txt",
        "data/2026-11-14T19-45-49_SRRG/not_render_service/30min/hidumper/txt/leak2.txt",
        "data/2026-11-14T19-45-49_SRRG/render_service/30min/hidumper/raw/leak3.txt",
    ] {
        let path = dir.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "NativeHeap pss: 1 kB\n").unwrap();
    }
    let output = ux()
        .current_dir(dir.path())
        .args(["--path-spec", SPEC, "--list"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(inventory_lines(&stdout).len(), 1, "{stdout}");
    assert!(stdout.contains("keep.txt"), "{stdout}");
    assert!(!stdout.contains("leak"), "{stdout}");
}

#[test]
fn scenario_7_three_native_heap_pss_values_carry_source_metadata() {
    let output = ux()
        .current_dir(RUN_ROOT)
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
    let rows: Vec<JsonValue> = inventory_lines(&stdout);
    assert_eq!(rows.len(), 3, "{stdout}");
    let values: Vec<&str> = rows
        .iter()
        .map(|row| row["value"]["kb"].as_str().unwrap())
        .collect();
    assert_eq!(values, ["12345", "67890", "321"], "{stdout}");
    for row in &rows {
        assert!(row["source"]["line"].is_number(), "{row}");
        assert!(row["source"]["record"].is_number(), "{row}");
        assert!(row["source"]["captures"].is_object(), "{row}");
    }
}

#[test]
fn scenario_8_captures_are_read_only_and_quiet_does_not_change_stdout() {
    let (code, plain, stderr) = list(&["--path-spec", SPEC, "--list"]);
    assert_eq!(code, 0, "{stderr}");
    let (quiet_code, quiet, quiet_stderr) = list(&["--path-spec", SPEC, "--list", "--quiet"]);
    assert_eq!(quiet_code, 0, "{quiet_stderr}");
    assert_eq!(plain, quiet, "--quiet changed --list output");

    let first = &inventory_lines(&plain)[0];
    let captures = &first["captures"];
    assert_eq!(captures["ts"], "2026-11-14T19-45-49", "{captures}");
    assert_eq!(captures["mins"], "30", "{captures}");
    assert_eq!(captures["file"], "dump1.txt", "{captures}");
    assert_eq!(captures.as_object().unwrap().len(), 3, "{captures}");

    // Captures do not feed the pipeline: the DSL cannot reference them, and a
    // fuzzy post-filter leaves the inventory shape unchanged.
    let (code, filtered, stderr) = list(&["--path-spec", SPEC, "--list", "--fuzzy", "dump1.txt"]);
    assert_eq!(code, 0, "{stderr}");
    let rows = inventory_lines(&filtered);
    assert!(!rows.is_empty(), "{filtered}");
    for row in rows {
        assert_eq!(row["captures"]["file"], "dump1.txt", "{filtered}");
    }
}
