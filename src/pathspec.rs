//! Structured path matching (`--path-spec`); see `docs/path-spec-design.md`.
//!
//! A spec is a `/`-separated sequence of segments. Each segment is a sequence
//! of literal fragments and explicit `{name:re:...}` / `{name:glob:...}`
//! placeholders. Since 0.3.1 there is no calendar/time validation and no
//! shorthand syntax: unknown kinds are rejected with `Usage(2)` before any
//! directory is opened. Callers express validity in the `re:` fragment.
//!
//! The spec is anchored against the normalized relative path from the current
//! directory. A directory-ending spec does NOT implicitly append `**`: the
//! final segment is treated as a file segment, so callers must spell out
//! `/{file:glob:*.txt}` explicitly.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use glob::Pattern as GlobPattern;
use nucleo_matcher::{Config, Matcher};
use regex::Regex;
use serde_json::{Map, Value as JsonValue};

use crate::cli::Cli;
use crate::diagnostic::Reporter;
use crate::discover::{DEFAULT_IGNORED_DIRS, Input, fuzzy_hit, is_excluded};
use crate::error::AppError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Re,
    Glob,
}

struct Segment {
    regex: Regex,
    captures: Vec<String>,
}

pub struct Compiled {
    segments: Vec<Segment>,
}

impl Compiled {
    /// Number of compiled segments; used by the fuzz entry point to keep a
    /// meaningful return shape.
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }
}

fn usage(message: impl Into<String>) -> AppError {
    AppError::Usage(format!("invalid --path-spec: {}", message.into()))
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Split on `/`, but never inside `{}` and never on an escaped character.
fn split_segments(spec: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut depth = 0_i32;
    let mut escaped = false;
    for ch in spec.chars() {
        if escaped {
            buf.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            buf.push(ch);
            escaped = true;
            continue;
        }
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return Err("unmatched `}`".to_owned());
                }
            }
            '/' if depth == 0 => {
                out.push(std::mem::take(&mut buf));
                continue;
            }
            _ => {}
        }
        buf.push(ch);
    }
    if depth != 0 {
        return Err("unmatched `{`".to_owned());
    }
    out.push(buf);
    if out.iter().any(String::is_empty) {
        return Err("empty segment (`//` is not allowed)".to_owned());
    }
    if out.len() > 64 {
        return Err("too many segments (limit 64)".to_owned());
    }
    Ok(out)
}

/// A segment is an alternating sequence of literal text and placeholders.
fn tokenize(segment: &str) -> Result<Vec<(bool, String)>, String> {
    let mut tokens = Vec::new();
    let mut literal = String::new();
    let mut chars = segment.chars().peekable();
    while let Some(ch) = chars.next() {
        if let ('\\', Some('{' | '}' | '\\')) = (ch, chars.peek()) {
            literal.push(chars.next().unwrap());
            continue;
        }
        if ch == '{' {
            if !literal.is_empty() {
                tokens.push((false, std::mem::take(&mut literal)));
            }
            let mut body = String::new();
            // Brace depth so a regex quantifier like `\d{4}` inside the
            // placeholder value does not prematurely close it.
            let mut depth = 1_usize;
            let mut closed = false;
            while let Some(inner) = chars.next() {
                if inner == '\\' {
                    body.push(inner);
                    if let Some(next) = chars.next() {
                        body.push(next);
                    }
                    continue;
                }
                if inner == '{' {
                    depth += 1;
                    body.push(inner);
                    continue;
                }
                if inner == '}' {
                    if depth == 1 {
                        closed = true;
                        break;
                    }
                    depth -= 1;
                    body.push(inner);
                    continue;
                }
                body.push(inner);
            }
            if !closed {
                return Err(format!("unterminated placeholder in `{segment}`"));
            }
            tokens.push((true, body));
        } else {
            literal.push(ch);
        }
    }
    if !literal.is_empty() {
        tokens.push((false, literal));
    }
    Ok(tokens)
}

/// Only explicit `{name:re:...}` and `{name:glob:...}` are accepted.
/// Shorthand (`{\d+}`, `{%Y-%m-%d}`), `time:` and `lit:` were removed in
/// 0.3.1; they now fail fast as `Usage(2)`.
fn parse_placeholder(body: &str) -> Result<(String, Kind, String), String> {
    let bad = || format!("placeholder `{{{body}}}` must be {{name:re:...}} or {{name:glob:...}}");
    let mut parts = body.splitn(3, ':');
    let name = parts.next().unwrap_or_default();
    let kind_text = parts.next().ok_or_else(bad)?;
    let value = parts.next().ok_or_else(bad)?.to_owned();
    if !is_identifier(name) {
        return Err(bad());
    }
    let kind = match kind_text {
        "re" => Kind::Re,
        "glob" => Kind::Glob,
        _ => {
            return Err(format!(
                "unknown placeholder kind `{kind_text}`; use re: or glob:"
            ));
        }
    };
    if value.is_empty() {
        return Err(bad());
    }
    Ok((name.to_owned(), kind, value))
}

fn glob_fragment(body: &str) -> Result<String, String> {
    let mut out = String::new();
    for ch in body.chars() {
        match ch {
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            '[' | ']' => {
                return Err(
                    "glob character classes are not supported; use {name:re:...}".to_owned(),
                );
            }
            _ => out.push_str(&regex::escape(&ch.to_string())),
        }
    }
    Ok(out)
}

/// A `re:` fragment is spliced into `(?P<name>...)`. Reject anything that can
/// escape that group: an unbalanced `)`, or a top-level `|` (which would turn
/// the segment into an alternation that matches every file name and silently
/// disables structural pruning).
fn validate_re_fragment(fragment: &str) -> Result<(), String> {
    let mut depth = 0_i32;
    let mut escaped = false;
    let mut in_class = false;
    let mut chars = fragment.chars().peekable();
    while let Some(ch) = chars.next() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if in_class {
            if ch == ']' {
                in_class = false;
            }
            continue;
        }
        match ch {
            '[' => {
                in_class = true;
                // A `]` right after `[` is a literal class member (`[]a]`),
                // not the closing bracket; consume it so the class stays open.
                if chars.peek() == Some(&']') {
                    chars.next();
                }
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return Err("unbalanced `)` escapes the capture group".to_owned());
                }
            }
            '|' if depth == 0 => {
                return Err(
                    "top-level `|` escapes the capture group; wrap it in `(?:...)`".to_owned(),
                );
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err("unbalanced `(` in regex fragment".to_owned());
    }
    Ok(())
}

pub fn compile(spec: &str) -> Result<Compiled, AppError> {
    let mut segments = Vec::new();
    let mut capture_names = Vec::new();
    let segment_texts = split_segments(spec).map_err(usage)?;
    for (index, text) in segment_texts.iter().enumerate() {
        let segment_number = index + 1;
        let mut body = String::new();
        let mut captures = Vec::new();
        let tokens =
            tokenize(text).map_err(|error| usage(format!("segment {segment_number}: {error}")))?;
        for (is_placeholder, token) in tokens {
            if !is_placeholder {
                body.push_str(&regex::escape(&token));
                continue;
            }
            let (name, kind, value) = parse_placeholder(&token)
                .map_err(|error| usage(format!("segment {segment_number}: {error}")))?;
            if capture_names.contains(&name) {
                return Err(usage(format!(
                    "segment {segment_number}: duplicate capture name `{name}`"
                )));
            }
            if capture_names.len() >= 32 {
                return Err(usage("too many captures (limit 32)"));
            }
            let fragment = match kind {
                Kind::Re => {
                    validate_re_fragment(&value)
                        .map_err(|error| usage(format!("segment {segment_number}: {error}")))?;
                    value
                }
                Kind::Glob => glob_fragment(&value)
                    .map_err(|error| usage(format!("segment {segment_number}: {error}")))?,
            };
            body.push_str(&format!("(?P<{name}>{fragment})"));
            captures.push(name.clone());
            capture_names.push(name);
        }
        let regex = Regex::new(&format!(r"\A(?:{body})\z")).map_err(|error| {
            usage(format!(
                "segment {segment_number}: bad regex fragment: {error}"
            ))
        })?;
        // Nested named groups inside an `re:` fragment are captures too.
        // Dropping them silently (0.3.1 behavior) hid file-embedded fields
        // from `--list` and `--with-source`. capture_names() yields every
        // named group, including the explicit placeholder itself, so skip
        // names already registered for this segment. The regex crate
        // rejects duplicates within one segment; only cross-segment
        // collisions and the 32-capture budget need checking here.
        for nested in regex.capture_names().flatten() {
            if captures.iter().any(|registered| registered == nested) {
                continue;
            }
            if capture_names.contains(&nested.to_owned()) {
                return Err(usage(format!(
                    "segment {segment_number}: duplicate capture name `{nested}`"
                )));
            }
            if capture_names.len() >= 32 {
                return Err(usage("too many captures (limit 32)"));
            }
            let nested = nested.to_owned();
            captures.push(nested.clone());
            capture_names.push(nested);
        }
        segments.push(Segment { regex, captures });
    }
    Ok(Compiled { segments })
}

fn capture_json(segment: &Segment, hit: &regex::Captures<'_>) -> BTreeMap<String, JsonValue> {
    segment
        .captures
        .iter()
        .map(|name| {
            let raw = hit
                .name(name)
                .map_or("", |matched| matched.as_str())
                .to_owned();
            (name.clone(), JsonValue::String(raw))
        })
        .collect()
}

/// Single funnel for the three discovery anomalies so tests exercise exactly
/// the formatting used in production. These drop entries, so they are
/// result-affecting and survive `--quiet`. `--error-format json` stays in
/// `Reporter`.
fn warn_io(
    kind: &str,
    path: &std::path::Path,
    error: &std::io::Error,
    reporter: &mut Reporter<'_>,
) {
    reporter.warn_result_affecting(&format!(
        "kind={kind} path={}: {error}",
        crate::model::slash_normalize(path)
    ));
}

/// Segment-level traversal: an unmatched directory is never entered (§5.1).
pub fn discover(
    spec: &str,
    cli: &Cli,
    reporter: &mut Reporter<'_>,
) -> Result<Vec<Input>, AppError> {
    if cli.max_depth.is_some() {
        reporter
            .warn("--max-depth is ignored with --path-spec; the segment count defines the depth");
    }
    let compiled = compile(spec)?;
    let base = std::env::current_dir()
        .map_err(|error| AppError::Runtime(format!("cannot resolve current directory: {error}")))?;
    let excludes = cli
        .exclude
        .iter()
        .map(|pattern| GlobPattern::new(pattern))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::Usage(format!("invalid --exclude glob: {error}")))?;
    let mut matcher = cli
        .fuzzy
        .as_ref()
        .map(|_| Matcher::new(Config::DEFAULT.match_paths()));

    type Frontier = (PathBuf, Vec<String>, BTreeMap<String, JsonValue>);
    let mut frontier: Vec<Frontier> = vec![(base, Vec::new(), BTreeMap::new())];

    for (index, segment) in compiled.segments.iter().enumerate() {
        let last = index + 1 == compiled.segments.len();
        let mut next: Vec<Frontier> = Vec::new();
        for (directory, parts, captures) in frontier {
            let raw_entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) => {
                    warn_io("read_dir", &directory, &error, reporter);
                    continue;
                }
            };
            // `ReadDir` yields an `io::Result<DirEntry>` per item: an error here
            // is a silently dropped entry, not a `read_dir` failure.
            let mut entries = Vec::new();
            for item in raw_entries {
                match item {
                    Ok(entry) => entries.push(entry),
                    Err(error) => warn_io("read_dir_entry", &directory, &error, reporter),
                }
            }
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let name = entry.file_name().to_string_lossy().into_owned();
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(error) => {
                        warn_io("file_type", &entry.path(), &error, reporter);
                        continue;
                    }
                };
                if last && !file_type.is_file() {
                    continue;
                }
                if !last && !file_type.is_dir() {
                    continue;
                }
                if !last && !cli.no_default_ignore && DEFAULT_IGNORED_DIRS.contains(&name.as_str())
                {
                    continue;
                }
                let Some(hit) = segment.regex.captures(&name) else {
                    continue;
                };
                let found = capture_json(segment, &hit);
                // One frontier node expands into many entries, so the prefix
                // belongs to the node and must be copied per child. Bounded by
                // 64 segments / 32 captures.
                let mut merged = captures.clone();
                merged.extend(found);
                let mut new_parts = parts.clone();
                new_parts.push(name);
                next.push((entry.path(), new_parts, merged));
            }
        }
        frontier = next;
    }

    let mut found: Vec<(PathBuf, String, Arc<JsonValue>)> = Vec::new();
    let mut seen = HashSet::new();
    for (path, parts, captures) in frontier {
        let relative = parts.join("/");
        if is_excluded(&relative, &excludes) {
            continue;
        }
        if let Some(matcher) = matcher.as_mut()
            && !fuzzy_hit(matcher, &relative, cli.fuzzy.as_deref().unwrap_or_default())
        {
            continue;
        }
        let key = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !seen.insert(key) {
            continue;
        }
        let mut object = Map::new();
        for (name, value) in captures {
            object.insert(name, value);
        }
        found.push((path, relative, Arc::new(JsonValue::Object(object))));
    }
    // Default lexicographic path order is the stable contract; --sort-by
    // overrides it using a named capture (numeric or string).
    if let Some(sort_by) = cli.sort_by.as_deref().and_then(crate::cli::parse_sort_by) {
        found.sort_by(|left, right| {
            compare_capture(&left.2, &right.2, &sort_by.capture, sort_by.numeric)
                .unwrap_or_else(|| left.1.cmp(&right.1))
        });
    } else {
        found.sort_by(|left, right| left.1.cmp(&right.1));
    }
    if found.is_empty() {
        reporter.warn_result_affecting("no input files matched");
    }
    Ok(found
        .into_iter()
        .map(|(path, _, captures)| Input::Matched { path, captures })
        .collect())
}

/// Compare two capture objects by a named key. Returns `None` when the key is
/// absent from either side (fall back to path order). With `numeric`, both
/// sides parsing as i64 yields numeric order; otherwise string order.
fn compare_capture(
    left: &JsonValue,
    right: &JsonValue,
    key: &str,
    numeric: bool,
) -> Option<std::cmp::Ordering> {
    let a = left.get(key)?.as_str()?;
    let b = right.get(key)?.as_str()?;
    if numeric {
        if let (Ok(an), Ok(bn)) = (a.parse::<i64>(), b.parse::<i64>()) {
            return Some(an.cmp(&bn));
        }
    }
    Some(a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Explicit `re:` fragment encodes the date shape. 0.3.1 performs no
    /// calendar validation; the alternate excludes 02-30 while accepting
    /// 02-29 regardless of leap year.
    const SPEC: &str = "data/{ts:re:(?:\\d{4}-(?:0[13578]|1[02])-(?:0[1-9]|[12]\\d|3[01])|\\d{4}-(?:0[469]|11)-(?:0[1-9]|[12]\\d|30)|\\d{4}-02-(?:0[1-9]|1\\d|2\\d))T\\d{2}-\\d{2}-\\d{2}}_SRRG/render_service/{mins:re:\\d+}min/hidumper/txt/{file:glob:*.txt}";

    #[test]
    fn compiles_the_user_scenario() {
        let compiled = compile(SPEC).unwrap();
        assert_eq!(compiled.segments.len(), 7);
        let segment = &compiled.segments[1];
        assert!(segment.regex.is_match("2026-11-14T19-45-49_SRRG"));
        assert!(!segment.regex.is_match("junk_SRRG"));
        let hit = segment.regex.captures("2026-11-14T19-45-49_SRRG").unwrap();
        let json = capture_json(segment, &hit);
        assert_eq!(
            json.get("ts"),
            Some(&JsonValue::String("2026-11-14T19-45-49".to_owned()))
        );
        assert!(!json.contains_key("ts_fields"));
    }

    #[test]
    fn shorthand_and_unknown_kinds_are_usage_errors() {
        for spec in [
            r"data/{\d+}.txt",
            r"data/{%Y-%m-%d}/{*.txt}",
            "data/{ts:time:%Y-%m-%d}/{f:glob:*.txt}",
            "data/{n:lit:foo}/{f:glob:*.txt}",
            "data/{name:body}.txt",
        ] {
            assert!(compile(spec).is_err(), "expected Usage(2) for {spec}");
        }
    }

    #[test]
    fn regex_quantifier_inside_placeholder_does_not_close_it() {
        let spec = r"data/{ts:re:\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}}_SRRG/{file:glob:*.txt}";
        let compiled = compile(spec).unwrap();
        assert!(
            compiled.segments[1]
                .regex
                .is_match("2026-11-14T19-45-49_SRRG")
        );
        assert!(compiled.segments[2].regex.is_match("dump1.txt"));
    }

    #[test]
    fn rejects_bad_specs_before_touching_the_filesystem() {
        for spec in [
            "data//x.txt",
            r"data/{a:re:\d+}/{a:re:\d+}.txt",
            "data/{f:glob:[ab].txt}",
            "data/{ts:time:%Y-%m-%d}/{f:glob:*.txt}",
            "data/{n:lit:foo}.txt",
        ] {
            assert!(compile(spec).is_err(), "expected an error for {spec}");
        }
    }

    #[test]
    fn no_calendar_or_time_validation() {
        let compiled = compile(SPEC).unwrap();
        let segment = &compiled.segments[1];
        let hit = segment.regex.captures("2026-02-29T99-99-99_SRRG").unwrap();
        let json = capture_json(segment, &hit);
        assert_eq!(json.len(), 1);
        assert!(!json.contains_key("ts_fields"));
        assert!(segment.regex.captures("2026-02-30T10-00-00_SRRG").is_none());
    }

    #[test]
    fn directory_ending_spec_does_not_match_files_below() {
        let compiled = compile(r"data/{ts:re:\d{4}-\d{2}-\d{2}}_SRRG/txt").unwrap();
        assert_eq!(compiled.segments.len(), 3);
        assert!(compiled.segments[2].regex.is_match("txt"));
        assert!(!compiled.segments[2].regex.is_match("dump1.txt"));
    }

    #[test]
    fn re_fragments_cannot_escape_the_capture_group() {
        for (spec, needle) in [
            (r"data/{x:re:a)|.*|(?:b}/{f:glob:*.txt}", "unbalanced `)`"),
            (r"data/{x:re:a|b}/{f:glob:*.txt}", "top-level `|`"),
            (r"data/{x:re:(?:a}/{f:glob:*.txt}", "unbalanced `(`"),
        ] {
            let error = match compile(spec) {
                Err(error) => error.to_string(),
                Ok(_) => panic!("expected a Usage(2) error for {spec}"),
            };
            assert!(error.contains(needle), "{spec} -> {error}");
        }
        // Alternation nested in a group is legitimate, and a `|` inside a
        // character class is not a top-level alternation at all.
        assert!(compile(r"data/{x:re:(?:ab|cd)\d{2}}/{f:glob:*.txt}").is_ok());
        assert!(compile(r"data/{x:re:[|]}/{f:glob:*.txt}").is_ok());
        assert!(compile(r"data/{x:re:[]a]bc}/{f:glob:*.txt}").is_ok());
    }

    #[test]
    fn escapes_and_colons_in_re_fragments_are_handled() {
        let compiled = compile(r"data/\{a\}/{ts:re:\d{2}:\d{2}}/{f:glob:*.txt}").unwrap();
        assert_eq!(compiled.segments.len(), 4);
        assert!(compiled.segments[1].regex.is_match("{a}"));
        assert!(!compiled.segments[1].regex.is_match("a"));
        assert!(compiled.segments[2].regex.is_match("19:45"));
        assert!(!compiled.segments[2].regex.is_match("1945"));
        let slashes = compile(r"data/\\/{f:glob:*.txt}").unwrap();
        assert!(slashes.segments[1].regex.is_match("\\"));
    }

    #[test]
    fn discovery_warnings_are_structured_and_result_affecting() {
        use crate::cli::ErrorFormat;
        use std::path::PathBuf;

        let error = std::io::Error::other("denied");
        let path = PathBuf::from("data/x");
        let emit = |format: ErrorFormat, quiet: bool| -> String {
            let mut bytes = Vec::new();
            {
                let mut reporter = Reporter::new(&mut bytes, format, quiet);
                warn_io("read_dir", &path, &error, &mut reporter);
                warn_io("read_dir_entry", &path, &error, &mut reporter);
                warn_io("file_type", &path, &error, &mut reporter);
            }
            String::from_utf8(bytes).unwrap()
        };

        let text = emit(ErrorFormat::Text, false);
        for kind in ["read_dir", "read_dir_entry", "file_type"] {
            assert!(
                text.contains(&format!("ux: warning: kind={kind} path=data/x: denied")),
                "{text}"
            );
        }
        // These drop entries, so they are result-affecting and survive --quiet.
        assert_eq!(emit(ErrorFormat::Text, true), text);

        let json = emit(ErrorFormat::Json, false);
        let lines: Vec<JsonValue> = json
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 3);
        for line in &lines {
            assert_eq!(line["code"], "warning");
            assert_eq!(line["exit_code"], 0);
            assert_eq!(line["category"], "result_set");
        }
        for (index, kind) in ["read_dir ", "read_dir_entry ", "file_type "]
            .into_iter()
            .enumerate()
        {
            assert!(
                lines[index]["message"].as_str().unwrap().contains(kind),
                "{json}"
            );
        }
    }
    #[test]
    fn nested_named_groups_are_captured_not_dropped() {
        // Field report: ts/seq matched but silently vanished from captures,
        // leaving file-embedded fields unreachable. They are captures now.
        let spec = r"data/{file:re:.*(?<ts>\d{8}_\d{6})_(?<seq>\d+)_hidumper\.txt}";
        let compiled = compile(spec).unwrap();
        let segment = &compiled.segments[1];
        let hit = segment
            .regex
            .captures("20261114_194549_0001_hidumper.txt")
            .unwrap();
        let json = capture_json(segment, &hit);
        assert_eq!(
            json["file"],
            JsonValue::String("20261114_194549_0001_hidumper.txt".to_owned())
        );
        assert_eq!(json["ts"], JsonValue::String("20261114_194549".to_owned()));
        assert_eq!(json["seq"], JsonValue::String("0001".to_owned()));
    }

    #[test]
    fn nested_group_name_colliding_across_segments_is_rejected() {
        // Same-segment duplicates fail in Regex::new already; cross-segment
        // collisions must fail in compile so captures stay unambiguous.
        for spec in [
            r"data/{ts:re:\d+}/{y:re:(?<ts>\d+)}.txt",
            r"data/{x:re:(?<id>\d+)}/{y:re:(?<id>\d+)}.txt",
        ] {
            let error = match compile(spec) {
                Err(error) => error.to_string(),
                Ok(_) => panic!("expected a Usage(2) error for {spec}"),
            };
            assert!(
                error.contains("duplicate capture name"),
                "{spec} -> {error}"
            );
        }
    }
}
