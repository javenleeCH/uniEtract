use std::fs;
use std::io::{Read, Write};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn ux() -> Command {
    Command::cargo_bin("ux").unwrap()
}

#[test]
fn help_documents_path_spec_contract_and_roadmap() {
    let output = ux()
        .args(["--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let help = String::from_utf8_lossy(&output.stdout).into_owned();
    for expected in [
        "PATH SPEC",
        "ROADMAP",
        "{name:re:...}",
        "{name:glob:...}",
        "Usage(2) before any directory is opened",
        "read-only string metadata",
        "--max-depth is ignored here",
        "no stable public API is promised",
        "Input-side BrokenPipe becomes runtime(1) with a diagnostic in 0.4.0",
    ] {
        assert!(help.contains(expected), "--help is missing {expected}");
    }
}

#[test]
fn json_to_regex_to_jq_pipeline() {
    ux().args([
        "-e",
        r#"jq(.items[].msg) | re(r"user=(?<id>\d+)") | jq(.id)"#,
        "-O",
        "text",
    ])
    .write_stdin(r#"{"items":[{"msg":"user=42"},{"msg":"user=7"}]}"#)
    .assert()
    .success()
    .stdout("42\n7\n");
}

#[test]
fn jsonl_records_keep_line_and_record_numbers() {
    ux().args(["-f", "jsonl", "-e", "jq(.)", "--with-source"])
        .write_stdin("{\"a\":1}\n\n{\"a\":2}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"record\":2"))
        .stdout(predicate::str::contains("\"line\":3"));
}

#[test]
fn yaml_toml_csv_tsv_are_normalized_to_data() {
    ux().args(["-f", "yaml", "-O", "text", "-e", "jq(.name)"])
        .write_stdin("---\nname: alice\n---\nname: bob\n")
        .assert()
        .success()
        .stdout("alice\nbob\n");

    ux().args(["-f", "toml", "-O", "text", "-e", "jq(.name)"])
        .write_stdin("name = \"carol\"\n")
        .assert()
        .success()
        .stdout("carol\n");

    ux().args(["-f", "csv", "-O", "text", "-e", "jq(.id)"])
        .write_stdin("id,name\n1,a\n2,b\n")
        .assert()
        .success()
        .stdout("1\n2\n");

    ux().args(["-f", "tsv", "-O", "text", "-e", "jq(.id)"])
        .write_stdin("id\tname\n3\tc\n")
        .assert()
        .success()
        .stdout("3\n");
}

#[test]
fn html_xpath_and_attribute_nodes() {
    ux().args([
        "-f",
        "html",
        "-O",
        "text",
        "-e",
        r#"xpath("//@href") | attr("href")"#,
    ])
    .write_stdin(r#"<html><body><a href="/one">x</a><a href="/two">y</a></body></html>"#)
    .assert()
    .success()
    .stdout("/one\n/two\n");
}

#[test]
fn malformed_xml_is_an_error() {
    ux().args(["-f", "xml", "-e", "text()"])
        .write_stdin("<root><broken></root>")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid XML"));
}

#[test]
fn slurp_spans_files_and_empty_input_yields_empty_array() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.jsonl"), "{\"n\":1}\n{\"n\":2}\n").unwrap();
    fs::write(dir.path().join("b.jsonl"), "{\"n\":3}\n").unwrap();
    ux().args([
        "-f",
        "jsonl",
        "-O",
        "text",
        "-e",
        "slurp() | jq(length)",
        dir.path().join("a.jsonl").to_str().unwrap(),
        dir.path().join("b.jsonl").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout("3\n");

    ux().args(["-f", "jsonl", "-e", "slurp()"])
        .write_stdin("")
        .assert()
        .success()
        .stdout("[]\n");
}

#[test]
fn slurp_with_json_output_produces_nested_array() {
    ux().args(["-f", "jsonl", "-O", "json", "-e", "slurp() | jq(map(.n))"])
        .write_stdin("{\"n\":1}\n{\"n\":2}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("[\n"));
}

#[test]
fn split_drops_empty_fragments() {
    ux().args([
        "-f",
        "text",
        "--text-mode",
        "raw",
        "-O",
        "text",
        "-e",
        r#"split(r"\|")"#,
    ])
    .write_stdin("a|b||c")
    .assert()
    .success()
    .stdout("a\nb\nc\n");
}

#[test]
fn compile_time_type_checks_fail_before_reading_input() {
    ux().args(["-f", "jsonl", "-e", "slurp() | xpath(\"//a\")"])
        .write_stdin("{\"a\":1}\n")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("requires Node"));

    ux().args(["-f", "json", "-e", "xpath(\"//a\")"])
        .write_stdin("{}")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("produces Data"));
}

#[test]
fn parameter_contradictions_exit_two() {
    ux().args(["-e", "text()", "-O", "text", "--with-source"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("mutually exclusive"));

    ux().args(["-e", "text()", "-", "-"]).assert().code(2);

    ux().args(["-e", "text()", "--exclude", "["])
        .assert()
        .code(2);
}

#[test]
fn max_results_errors_instead_of_truncating() {
    ux().args([
        "-f",
        "jsonl",
        "-O",
        "text",
        "--max-results",
        "1",
        "-e",
        "jq(.)",
    ])
    .write_stdin("1\n2\n")
    .assert()
    .code(1)
    .stdout("1\n")
    .stderr(predicate::str::contains("--max-results"));
}

#[test]
fn utf8_bom_is_stripped_from_first_record() {
    let mut input = vec![0xEF, 0xBB, 0xBF];
    input.extend_from_slice(b"{\"a\":1}\n");
    let mut command = ux();
    command.args(["-f", "jsonl", "-O", "text", "-e", "jq(.a)"]);
    command.write_stdin(input);
    command.assert().success().stdout("1\n");
}

#[test]
fn gbk_fallback_is_decoded() {
    let mut command = ux();
    command.args([
        "-f",
        "text",
        "--text-mode",
        "raw",
        "--encoding",
        "auto",
        "-O",
        "text",
        "-e",
        "text()",
    ]);
    command.write_stdin(vec![0xD6, 0xD0, 0xCE, 0xC4]);
    command
        .assert()
        .success()
        .stdout("中文\n")
        .stderr(predicate::str::contains("assumed GB18030"));
}

#[test]
fn binary_input_is_skipped_with_warning() {
    let mut command = ux();
    command.args(["-f", "text", "--text-mode", "raw", "-e", "text()"]);
    command.write_stdin(b"a\0b".to_vec());
    command
        .assert()
        .success()
        .stderr(predicate::str::contains("binary input skipped"));
}

#[test]
fn size_limits_report_clear_errors() {
    ux().args(["-f", "json", "--max-input-size", "3", "-e", "text()"])
        .write_stdin("{\"a\":1}")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--max-input-size"));

    ux().args(["-f", "jsonl", "--max-record-size", "3", "-e", "text()"])
        .write_stdin("{\"a\":1}\n")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--max-record-size"));
}

#[test]
fn file_discovery_is_deterministic_and_prunes_ignored_dirs() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("b.json"), "{\"n\":2}\n").unwrap();
    fs::write(dir.path().join("a.json"), "{\"n\":1}\n").unwrap();
    fs::create_dir_all(dir.path().join("target")).unwrap();
    fs::write(dir.path().join("target/generated.json"), "{\"n\":9}\n").unwrap();

    ux().args([
        "-f",
        "jsonl",
        "-O",
        "text",
        "-e",
        "jq(.n)",
        dir.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout("1\n2\n");
}

#[test]
fn fuzzy_filters_discovered_files() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("prodconfig.yaml"), "hit\n").unwrap();
    fs::write(dir.path().join("unrelated.txt"), "miss\n").unwrap();
    ux().args([
        "-f",
        "text",
        "--fuzzy",
        "prodconfig",
        "-O",
        "text",
        "-e",
        "text()",
        dir.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout("hit\n");
}

#[test]
fn empty_glob_succeeds_with_warning() {
    let dir = tempdir().unwrap();
    let pattern = format!("{}/*.none", dir.path().to_str().unwrap());
    ux().args(["-e", "text()", &pattern])
        .assert()
        .success()
        .stderr(predicate::str::contains("no input files matched"));
}

#[test]
fn skip_incompatible_only_handles_the_first_stage() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("data.json"), "{\"a\":1}\n").unwrap();
    fs::write(dir.path().join("page.html"), "<a href=\"/x\">x</a>").unwrap();
    ux().args([
        "--skip-incompatible",
        "-O",
        "text",
        "-e",
        r#"xpath("//a") | attr("href")"#,
        dir.path().join("data.json").to_str().unwrap(),
        dir.path().join("page.html").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout("/x\n")
    .stderr(predicate::str::contains("incompatible"));
}

#[test]
fn json_output_is_pretty_and_compact_flag_works() {
    ux().args(["-f", "jsonl", "-O", "json", "-e", "jq(.)"])
        .write_stdin("1\n2\n")
        .assert()
        .success()
        .stdout("[\n  1,\n  2\n]\n");

    ux().args(["-f", "jsonl", "-O", "json", "--compact", "-e", "jq(.)"])
        .write_stdin("1\n2\n")
        .assert()
        .success()
        .stdout("[1,2]\n");
}

#[test]
fn line_buffered_emits_before_stdin_is_closed() {
    let mut child = ProcessCommand::new(env!("CARGO_BIN_EXE_ux"))
        .args([
            "--line-buffered",
            "-f",
            "text",
            "-O",
            "text",
            "-e",
            "text()",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"hello\n").unwrap();
    stdin.flush().unwrap();

    let mut stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut buffer = [0_u8; 64];
        let read = stdout.read(&mut buffer).unwrap();
        let _ = sender.send(String::from_utf8_lossy(&buffer[..read]).into_owned());
    });

    let received = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("--line-buffered produced no output before EOF");
    assert_eq!(received, "hello\n");
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success());
}

#[test]
fn nan_and_large_integers_follow_the_jq_contract() {
    ux().args(["-f", "json", "-e", "jq(nan)"])
        .write_stdin("null")
        .assert()
        .success()
        .stdout("null\n");

    ux().args(["-f", "json", "-e", "jq(.)"])
        .write_stdin(r#"{"n":123456789012345678901234567890}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("123456789012345678901234567890"));
}

// §6.4 HTML 容错专项：这些用例固定 sxd_html/html5ever 的实际行为，
// 升级 DOM 后端或 html5ever 版本时必须重跑，避免静默改变查询结果。

fn html_stdout(html: &str, pipeline: &str) -> String {
    let output = ux()
        .args(["-f", "html", "-O", "text", "-e", pipeline])
        .write_stdin(html)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).unwrap()
}

#[test]
fn html_implicit_tbody_breaks_direct_table_tr_xpath() {
    let html = "<table><tr><td>x</td></tr></table>";
    // html5ever inserts <tbody>, so //table/tr is a zero-match, not an error.
    assert_eq!(html_stdout(html, r#"xpath("//table/tr") | text()"#), "");
    assert_eq!(
        html_stdout(html, r#"xpath("//table/tbody/tr") | text()"#),
        "x\n"
    );
}

#[test]
fn html_template_contents_are_visible_to_xpath() {
    let html = r#"<template><p id="t">inside</p></template><p id="o">outside</p>"#;
    assert_eq!(html_stdout(html, r#"xpath("//p") | attr("id")"#), "t\no\n");
}

#[test]
fn html_noscript_markup_stays_raw_text() {
    let html = r#"<noscript><p id="n">inside</p></noscript><p id="o">outside</p>"#;
    // With scripting enabled, noscript body is raw text: no <p> element inside it.
    assert_eq!(html_stdout(html, r#"xpath("//p") | attr("id")"#), "o\n");
    assert_eq!(
        html_stdout(html, r#"xpath("//noscript") | text()"#),
        "<p id=\"n\">inside</p>\n"
    );
}

#[test]
fn html_folds_element_and_attribute_names_to_lowercase() {
    let html = r#"<DIV ID="upper"><A HREF="/u">x</A></DIV>"#;
    assert_eq!(html_stdout(html, r#"xpath("//a") | attr("href")"#), "/u\n");
    assert_eq!(html_stdout(html, r#"xpath("//A") | attr("HREF")"#), "");
}
