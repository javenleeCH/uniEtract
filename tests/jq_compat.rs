//! Executable compatibility set for the locked jaq trio (3.1.2 / 3.0.4 / 2.0.3).
//!
//! v3 §8.5 asked for a generated `COMPAT.md`; an executable test is used instead
//! because it cannot drift from the code and fails CI on a dependency upgrade.

use std::io::Write;
use std::process::{Command, Stdio};

fn run(filter: &str, input: &str) -> (String, String, i32) {
    let pipeline = format!("jq({filter})");
    let mut child = Command::new(env!("CARGO_BIN_EXE_ux"))
        .args(["-f", "json", "-O", "text", "-e", &pipeline])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn supported_core_filters() {
    let cases: &[(&str, &str, &str, &str)] = &[
        ("field access", ".a", r#"{"a":1}"#, "1\n"),
        ("array iteration", ".[]", "[1,2]", "1\n2\n"),
        ("select", "select(. > 1)", "2", "2\n"),
        (
            "object construction",
            "{id: .a}",
            r#"{"a":7}"#,
            "{\"id\":7}\n",
        ),
        ("inner pipe", ".a | .b", r#"{"a":{"b":3}}"#, "3\n"),
        ("map", "map(.a)", r#"[{"a":1},{"a":2}]"#, "[1,2]\n"),
        ("length", "length", "[1,2,3]", "3\n"),
        ("keys", "keys", r#"{"b":1,"a":2}"#, "[\"a\",\"b\"]\n"),
        ("tonumber", "tonumber", r#""12""#, "12\n"),
        ("add", "add", "[1,2,3]", "6\n"),
        ("sort", "sort", "[3,1,2]", "[1,2,3]\n"),
        ("unique", "unique", "[1,1,2]", "[1,2]\n"),
        ("nan becomes null", "nan", "null", "null\n"),
        ("infinite becomes null", "infinite", "null", "null\n"),
        (
            "arbitrary precision integer",
            ".",
            r#"123456789012345678901234567890"#,
            "123456789012345678901234567890\n",
        ),
    ];
    for (name, filter, input, expected) in cases {
        let (stdout, stderr, code) = run(filter, input);
        assert_eq!(code, 0, "{name}: stderr={stderr}");
        assert_eq!(stdout, *expected, "{name}");
    }
}

#[test]
fn runtime_errors_are_reported_not_swallowed() {
    let (stdout, stderr, code) = run("error(\"boom\")", "null");
    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("boom"));
}

#[test]
fn unknown_jq_syntax_fails_at_compile_time() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ux"))
        .args(["-e", "jq(. |", "-f", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("query error"));
}
