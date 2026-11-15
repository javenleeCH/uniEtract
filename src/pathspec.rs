//! Structured path matching (`--path-spec`), design §4 / §5.
//!
//! A spec is a `/`-separated sequence of segments. Each segment is a sequence
//! of literal fragments and `{placeholder}` fragments, so
//! `{%Y-%m-%d}_SRRG` and `{\d+}min` work without classifying a whole segment
//! as "regex" or "glob". Placeholders may be explicit
//! (`{name:re:\d+}`, `{name:glob:*.txt}`, `{name:time:%Y-%m-%d}`,
//! `{name:lit:foo}`) or shorthand (`{\d+}`, `{%Y-%m-%d}`, `{*.txt}`).
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
use serde_json::{Map, Value as JsonValue, json};

use crate::cli::Cli;
use crate::diagnostic::Reporter;
use crate::discover::{DEFAULT_IGNORED_DIRS, Input, fuzzy_hit, is_excluded};
use crate::error::AppError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Re,
    Glob,
    Time,
    Lit,
}

enum Capture {
    Plain(String),
    Time { name: String, parser: Regex },
}

struct Segment {
    regex: Regex,
    captures: Vec<Capture>,
}

pub struct Compiled {
    segments: Vec<Segment>,
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
        if ch == '\\' {
            if let Some('{' | '}' | '\\') = chars.peek() {
                literal.push(chars.next().unwrap());
                continue;
            }
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

fn infer_kind(body: &str) -> Kind {
    if body.starts_with('%') {
        Kind::Time
    } else if (body.contains('*') || body.contains('?')) && !body.contains('\\') {
        Kind::Glob
    } else {
        Kind::Re
    }
}

fn parse_placeholder(body: &str, auto: &mut usize) -> Result<(String, Kind, String), String> {
    let mut parts = body.splitn(3, ':');
    let first = parts.next().unwrap_or_default();
    if is_identifier(first) {
        if let Some(kind_text) = parts.next() {
            let kind = match kind_text {
                "re" => Some(Kind::Re),
                "glob" => Some(Kind::Glob),
                "time" => Some(Kind::Time),
                "lit" => Some(Kind::Lit),
                _ => None,
            };
            if let Some(kind) = kind {
                let rest = parts.next().unwrap_or_default().to_owned();
                return Ok((first.to_owned(), kind, rest));
            }
            // Named shorthand `{name:body}` (the prototype's spelling).
            let rest = match parts.next() {
                Some(third) => format!("{kind_text}:{third}"),
                None => kind_text.to_owned(),
            };
            return Ok((first.to_owned(), infer_kind(&rest), rest));
        }
        return Ok((first.to_owned(), Kind::Lit, String::new()));
    }
    *auto += 1;
    Ok((format!("cap{auto}"), infer_kind(body), body.to_owned()))
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

/// Build (shape regex, field-parser regex) for a time placeholder.
fn time_fragments(fmt: &str) -> Result<(String, Regex), String> {
    let mut shape = String::new();
    let mut parser = String::new();
    let mut chars = fmt.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let code = chars
                .next()
                .ok_or_else(|| "time format ends with a dangling `%`".to_owned())?;
            let (pattern, letter) = match code {
                'Y' => (r"\d{4}", 'Y'),
                'm' => (r"\d{2}", 'm'),
                'd' => (r"\d{2}", 'd'),
                'H' => (r"\d{2}", 'H'),
                'M' => (r"\d{2}", 'M'),
                'S' => (r"\d{2}", 'S'),
                _ => {
                    return Err(format!(
                        "unknown time directive `%{code}` (supported: %Y %m %d %H %M %S)"
                    ));
                }
            };
            shape.push_str(pattern);
            parser.push_str(&format!("(?P<{letter}>{pattern})"));
        } else if ch == ':' {
            return Err(
                "time format contains ':': Windows directory names cannot contain a colon; use %H-%M-%S or an explicit {name:re:...}"
                    .to_owned(),
            );
        } else {
            let escaped = regex::escape(&ch.to_string());
            shape.push_str(&escaped);
            parser.push_str(&escaped);
        }
    }
    let parser = Regex::new(&format!(r"\A(?:{parser})\z"))
        .map_err(|error| format!("bad time format: {error}"))?;
    Ok((shape, parser))
}

fn valid_time(raw: &str, parser: &Regex) -> Option<[u32; 6]> {
    let hits = parser.captures(raw)?;
    let get = |name: &str| -> Option<u32> { hits.name(name)?.as_str().parse().ok() };
    let year = get("Y").unwrap_or(1970);
    let month = get("m").unwrap_or(1);
    let day = get("d").unwrap_or(1);
    let hour = get("H").unwrap_or(0);
    let minute = get("M").unwrap_or(0);
    let second = get("S").unwrap_or(0);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if day == 0 || day > days || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some([year, month, day, hour, minute, second])
}

pub fn compile(spec: &str) -> Result<Compiled, AppError> {
    let mut segments = Vec::new();
    let mut capture_names = Vec::new();
    let mut auto = 0_usize;
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
            let (name, kind, value) = parse_placeholder(&token, &mut auto)
                .map_err(|error| usage(format!("segment {segment_number}: {error}")))?;
            if !is_identifier(&name) {
                return Err(usage(format!(
                    "segment {segment_number}: illegal capture name `{name}`"
                )));
            }
            if capture_names.contains(&name) {
                return Err(usage(format!(
                    "segment {segment_number}: duplicate capture name `{name}`"
                )));
            }
            if capture_names.len() >= 32 {
                return Err(usage("too many captures (limit 32)"));
            }
            let fragment = match kind {
                Kind::Re => value.clone(),
                Kind::Glob => glob_fragment(&value)
                    .map_err(|error| usage(format!("segment {segment_number}: {error}")))?,
                Kind::Lit => regex::escape(&value),
                Kind::Time => {
                    let (shape, parser) = time_fragments(&value)
                        .map_err(|error| usage(format!("segment {segment_number}: {error}")))?;
                    captures.push(Capture::Time {
                        name: name.clone(),
                        parser,
                    });
                    shape
                }
            };
            if !matches!(kind, Kind::Time) {
                captures.push(Capture::Plain(name.clone()));
            }
            body.push_str(&format!("(?P<{name}>{fragment})"));
            capture_names.push(name);
        }
        let regex = Regex::new(&format!(r"\A(?:{body})\z")).map_err(|error| {
            usage(format!(
                "segment {segment_number}: bad regex fragment: {error}"
            ))
        })?;
        segments.push(Segment { regex, captures });
    }
    Ok(Compiled { segments })
}

fn capture_json(
    segment: &Segment,
    hit: &regex::Captures<'_>,
) -> Option<BTreeMap<String, JsonValue>> {
    let mut out = BTreeMap::new();
    for capture in &segment.captures {
        let raw = hit.name(match capture {
            Capture::Plain(name) => name,
            Capture::Time { name, .. } => name,
        })?;
        let raw = raw.as_str().to_owned();
        match capture {
            Capture::Plain(name) => {
                out.insert(name.clone(), JsonValue::String(raw));
            }
            Capture::Time { name, parser } => {
                let fields = valid_time(&raw, parser)?;
                out.insert(name.clone(), JsonValue::String(raw));
                out.insert(format!("{name}_fields"), json!(fields.to_vec()));
            }
        }
    }
    Some(out)
}

/// Segment-level traversal: an unmatched directory is never entered (§5.1).
pub fn discover(
    spec: &str,
    cli: &Cli,
    reporter: &mut Reporter<'_>,
) -> Result<Vec<Input>, AppError> {
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
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) => {
                    reporter.warn(&format!(
                        "cannot read directory `{}`: {error}",
                        directory.display()
                    ));
                    continue;
                }
            };
            let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let name = entry.file_name().to_string_lossy().into_owned();
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(_) => continue,
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
                let Some(found) = capture_json(segment, &hit) else {
                    continue; // impossible calendar date
                };
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
    found.sort_by(|left, right| left.1.cmp(&right.1));
    if found.is_empty() {
        reporter.warn("no input files matched");
    }
    Ok(found
        .into_iter()
        .map(|(path, _, captures)| Input::Matched { path, captures })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_the_user_scenario() {
        let spec = "data/{ts:time:%Y-%m-%dT%H-%M-%S}_SRRG/render_service/{mins:re:\\d+}min/hidumper/txt/{file:glob:*.txt}";
        let compiled = compile(spec).unwrap();
        assert_eq!(compiled.segments.len(), 7);
        let segment = &compiled.segments[1];
        assert!(segment.regex.is_match("2026-11-14T19-45-49_SRRG"));
        assert!(!segment.regex.is_match("junk_SRRG"));
        let hit = segment.regex.captures("2026-11-14T19-45-49_SRRG").unwrap();
        let json = capture_json(segment, &hit).unwrap();
        assert_eq!(json["ts"], json!("2026-11-14T19-45-49"));
        assert_eq!(json["ts_fields"], json!([2026, 11, 14, 19, 45, 49]));
    }

    #[test]
    fn shorthand_placeholders_still_work() {
        let spec = r"data/{%Y-%m-%d}_SRRG/{\d+}min/{*.txt}";
        let compiled = compile(spec).unwrap();
        assert!(compiled.segments[3].regex.is_match("dump1.txt"));
        let hit = compiled.segments[2].regex.captures("30min").unwrap();
        let json = capture_json(&compiled.segments[2], &hit).unwrap();
        assert_eq!(json["cap2"], json!("30"));
    }

    #[test]
    fn regex_quantifier_inside_placeholder_does_not_close_it() {
        // The `{4}` / `{2}` in `\d{4}` must be treated as part of the regex
        // value, not as the placeholder delimiter.
        let spec = r"data/{ts:re:\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}}_SRRG/render_service/{mins:re:\d+}min/hidumper/txt/{file:glob:*.txt}";
        let compiled = compile(spec).unwrap();
        let ts = &compiled.segments[1].regex;
        assert!(ts.is_match("2026-11-14T19:45:49_SRRG"));
        assert!(!ts.is_match("junk_SRRG"));
        assert!(compiled.segments[3].regex.is_match("30min"));
    }

    #[test]
    fn rejects_bad_specs_before_touching_the_filesystem() {
        for spec in [
            "data/{ts:time:%Y-%m-%dT%H:%M:%S}_SRRG/x.txt",
            "data/{ts:time:%Y-%q}_SRRG/x.txt",
            "data//x.txt",
            "data/{a:re:\\d+}/{a:re:\\d+}.txt",
            "data/{ts:time:%Y-%m-%d}_SRRG/{f:glob:[ab].txt}",
        ] {
            assert!(compile(spec).is_err(), "expected an error for {spec}");
        }
    }

    #[test]
    fn calendar_validation_is_strict() {
        let spec = "data/{ts:time:%Y-%m-%dT%H-%M-%S}_SRRG/{file:glob:*.txt}";
        let compiled = compile(spec).unwrap();
        let hit = compiled.segments[1]
            .regex
            .captures("2026-02-30T10-00-00_SRRG")
            .unwrap();
        assert!(capture_json(&compiled.segments[1], &hit).is_none());
        let hit = compiled.segments[1]
            .regex
            .captures("2024-02-29T23-59-59_SRRG")
            .unwrap();
        let json = capture_json(&compiled.segments[1], &hit).unwrap();
        assert_eq!(json["ts_fields"], json!([2024, 2, 29, 23, 59, 59]));
    }

    #[test]
    fn directory_ending_spec_does_not_match_files_below() {
        let compiled = compile("data/{ts:time:%Y-%m-%d}_SRRG/txt").unwrap();
        assert_eq!(compiled.segments.len(), 3);
        assert!(compiled.segments[2].regex.is_match("txt"));
        assert!(!compiled.segments[2].regex.is_match("dump1.txt"));
    }
}
