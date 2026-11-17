use pest::Parser;
use pest_derive::Parser;

use crate::error::AppError;

#[derive(Parser)]
#[grammar = "dsl.pest"]
struct DslParser;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StageSpec {
    Jq(String),
    XPath(String),
    Regex(String),
    Text,
    Attr(String),
    Slurp,
    Split(String),
    Captures,
    Check(String),
}

/// Parse a whole pipeline. `dsl.pest` owns the structural scan (top-level
/// `|`, balanced parens, string literals); this module owns the semantics
/// (stage names, literal decoding) so `StageSpec` stays the single source of
/// truth for which stages exist.
pub fn parse(input: &str) -> Result<Vec<StageSpec>, AppError> {
    check_nesting(input)?;
    let mut pairs = DslParser::parse(Rule::pipeline, input).map_err(query_error)?;
    let pipeline = pairs
        .next()
        .ok_or_else(|| AppError::Query("empty pipeline stage".to_owned()))?;

    let mut stages = Vec::new();
    for pair in pipeline.into_inner() {
        // `EOI` also shows up as a child pair; only stage pairs carry data.
        if pair.as_rule() != Rule::stage {
            continue;
        }
        let mut parts = pair.into_inner();
        let name = parts
            .next()
            .ok_or_else(|| AppError::Query("missing stage name".to_owned()))?
            .as_str();
        let argument = parts
            .next()
            .ok_or_else(|| AppError::Query(format!("stage `{name}` requires an argument")))?
            .as_str();
        stages.push(build_stage(name, argument)?);
    }

    if stages.is_empty() {
        return Err(AppError::Query("empty pipeline stage".to_owned()));
    }
    Ok(stages)
}

/// Maximum paren nesting accepted in a single pipeline.
///
/// Resource guard, not a parser: `group` is indirectly recursive in the
/// grammar (`balanced → group → balanced`), so unbounded nesting would
/// overflow the native stack (measured: ~500 levels aborts the process).
/// This scans only for depth and skips string literals; **all** syntax
/// decisions still belong to `dsl.pest`. We deliberately do not use
/// `pest::set_call_limit` — it counts rule calls, which scale with input
/// *length* (a long regex would be rejected) while still allowing enough
/// nested calls to blow the stack. Nor do we enable pest's `stack-guard`
/// feature: with a 128-level cap the recursion is already bounded, so dynamic
/// stack extension buys nothing. (pest's default `std` feature pulls in
/// `pest_stack_guard`/`stacker`/`psm`/`cc` regardless, so disabling
/// `stack-guard` does not remove those build-time dependencies.)
const MAX_NESTING: usize = 128;

fn check_nesting(input: &str) -> Result<(), AppError> {
    let bytes = input.as_bytes();
    let mut index = 0;
    let mut depth = 0_usize;
    while index < bytes.len() {
        if let Some((prefix_len, hashes)) = raw_prefix(&bytes[index..]) {
            let body_start = index + prefix_len;
            let terminator = format!("\"{}", "#".repeat(hashes));
            match input[body_start..].find(&terminator) {
                Some(offset) => {
                    index = body_start + offset + terminator.len();
                    continue;
                }
                None => break, // let the grammar report the unterminated literal
            }
        }
        match bytes[index] {
            b'\'' | b'"' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() && bytes[index] != quote {
                    if bytes[index] == b'\\' {
                        index += 1;
                    }
                    index += 1;
                }
            }
            b'(' => {
                depth += 1;
                if depth > MAX_NESTING {
                    return Err(AppError::Query(format!(
                        "pipeline nesting exceeds {MAX_NESTING} levels"
                    )));
                }
            }
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        index += 1;
    }
    Ok(())
}

fn query_error(error: pest::error::Error<Rule>) -> AppError {
    let position = match error.location {
        pest::error::InputLocation::Pos(position) => position,
        pest::error::InputLocation::Span((position, _)) => position,
    };
    AppError::Query(format!(
        "invalid pipeline at byte {position}: {}",
        error.variant.message()
    ))
}

fn build_stage(name: &str, argument: &str) -> Result<StageSpec, AppError> {
    let inner = argument.trim();
    match name {
        "jq" if !inner.is_empty() => Ok(StageSpec::Jq(inner.to_owned())),
        "jq" => Err(AppError::Query("jq requires a filter".to_owned())),
        "xpath" => Ok(StageSpec::XPath(parse_string_argument(inner, "xpath")?)),
        "re" => Ok(StageSpec::Regex(parse_string_argument(inner, "re")?)),
        "text" if inner.is_empty() => Ok(StageSpec::Text),
        "text" => Err(AppError::Query("text() takes no arguments".to_owned())),
        "slurp" if inner.is_empty() => Ok(StageSpec::Slurp),
        "slurp" => Err(AppError::Query("slurp() takes no arguments".to_owned())),
        "attr" => Ok(StageSpec::Attr(parse_string_argument(inner, "attr")?)),
        "split" => Ok(StageSpec::Split(parse_string_argument(inner, "split")?)),
        "captures" if inner.is_empty() => Ok(StageSpec::Captures),
        "captures" => Err(AppError::Query("captures() takes no arguments".to_owned())),
        "check" if !inner.is_empty() => Ok(StageSpec::Check(inner.to_owned())),
        "check" => Err(AppError::Query("check requires a filter".to_owned())),
        _ => Err(AppError::Query(format!("unknown stage `{name}`"))),
    }
}

/// Recognise the `r` / `r#` / `r##` ... prefix. The grammar already validated
/// the matching terminator; this second read is only needed to locate the
/// content slice for literal extraction.
fn raw_prefix(bytes: &[u8]) -> Option<(usize, usize)> {
    if bytes.first() != Some(&b'r') {
        return None;
    }
    let mut index = 1;
    while bytes.get(index) == Some(&b'#') {
        index += 1;
    }
    if bytes.get(index) == Some(&b'"') {
        Some((index + 1, index - 1))
    } else {
        None
    }
}

fn parse_string_argument(input: &str, stage: &str) -> Result<String, AppError> {
    if let Some((prefix_len, hashes)) = raw_prefix(input.as_bytes()) {
        let content = &input[prefix_len..];
        let terminator = format!("\"{}", "#".repeat(hashes));
        let value = content
            .strip_suffix(&terminator)
            .ok_or_else(|| AppError::Query(format!("unterminated raw string in `{stage}`")))?;
        return Ok(value.to_owned());
    }

    let delimiter = input
        .chars()
        .next()
        .filter(|character| matches!(character, '\'' | '"'))
        .ok_or_else(|| AppError::Query(format!("{stage} requires one quoted string")))?;
    let mut value = String::new();
    let mut characters = input[delimiter.len_utf8()..].chars();
    while let Some(character) = characters.next() {
        if character == delimiter {
            if characters.as_str().trim().is_empty() {
                return Ok(value);
            }
            return Err(AppError::Query(format!(
                "unexpected content after {stage} string argument"
            )));
        }
        if character == '\\' {
            let escaped = characters
                .next()
                .ok_or_else(|| AppError::Query("unterminated escape sequence".to_owned()))?;
            value.push(match escaped {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                _ => {
                    return Err(AppError::Query(format!(
                        "unsupported escape \\{escaped}; use a raw string r\"...\" for regular expressions"
                    )));
                }
            });
        } else {
            value.push(character);
        }
    }
    Err(AppError::Query(format!("unterminated string in `{stage}`")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_jq_pipes_and_nested_parentheses() {
        let stages = parse(r#"jq(.items[] | select(.ok) | {id}) | text()"#).unwrap();
        assert_eq!(
            stages,
            vec![
                StageSpec::Jq(".items[] | select(.ok) | {id}".to_owned()),
                StageSpec::Text,
            ]
        );
    }

    #[test]
    fn parses_raw_and_escaped_strings() {
        let stages = parse(r##"re(r#"user=(?<id>\d+)"#) | attr("data-id")"##).unwrap();
        assert_eq!(
            stages,
            vec![
                StageSpec::Regex(r"user=(?<id>\d+)".to_owned()),
                StageSpec::Attr("data-id".to_owned()),
            ]
        );
    }

    #[test]
    fn rejects_empty_and_unclosed_stages() {
        assert!(parse("text() | | text()").is_err());
        assert!(parse("jq(.a").is_err());
        assert!(parse(r#"re("\d")"#).is_err());
    }

    #[test]
    fn parses_slurp_and_split() {
        let stages = parse(r#"slurp() | jq(length) | split(r"\n+")"#).unwrap();
        assert_eq!(
            stages,
            vec![
                StageSpec::Slurp,
                StageSpec::Jq("length".to_owned()),
                StageSpec::Split(r"\n+".to_owned()),
            ]
        );
    }

    #[test]
    fn rejects_arguments_to_nullary_stages() {
        assert!(parse("slurp(x)").is_err());
        assert!(parse("text(x)").is_err());
        assert!(parse("split()").is_err());
    }

    #[test]
    fn raw_strings_support_arbitrary_hash_counts() {
        let stages = parse(r###"re(r##"a"b(c)"##) | re(r#"x"#) | re(r"z")"###).unwrap();
        assert_eq!(
            stages,
            vec![
                StageSpec::Regex(r##"a"b(c)"##.to_owned()),
                StageSpec::Regex("x".to_owned()),
                StageSpec::Regex("z".to_owned()),
            ]
        );
    }

    #[test]
    fn parens_and_pipes_inside_strings_do_not_split_the_pipeline() {
        let stages = parse(r#"re("a)b|c") | xpath('//a[text()="("]')"#).unwrap();
        assert_eq!(
            stages,
            vec![
                StageSpec::Regex("a)b|c".to_owned()),
                StageSpec::XPath(r#"//a[text()="("]"#.to_owned()),
            ]
        );
    }

    #[test]
    fn multibyte_characters_next_to_separators_are_safe() {
        let stages = parse(r#"re("中文|处理") | text()"#).unwrap();
        assert_eq!(
            stages,
            vec![StageSpec::Regex("中文|处理".to_owned()), StageSpec::Text,]
        );
    }

    #[test]
    fn whitespace_around_stage_parts_is_tolerated() {
        let stages = parse("  jq( .a )  |  text( )  ").unwrap();
        assert_eq!(
            stages,
            vec![StageSpec::Jq(".a".to_owned()), StageSpec::Text]
        );
    }

    #[test]
    fn mismatched_raw_string_hashes_are_rejected() {
        // `POP` must match exactly the hash run recorded by `PUSH`.
        // Plain Rust strings here avoid nested raw-string delimiter ambiguity.
        assert!(parse("re(r##\"a\"#)").is_err());
        assert!(parse("re(r#\"a\"##)").is_err());
        assert!(parse("re(r#####\"a\"#####)").is_ok());
    }

    #[test]
    fn a_bare_r_is_not_mistaken_for_a_raw_string() {
        assert_eq!(
            parse("jq(r) | text()").unwrap(),
            vec![StageSpec::Jq("r".to_owned()), StageSpec::Text,]
        );
        assert_eq!(
            parse(r#"re(r"z")"#).unwrap(),
            vec![StageSpec::Regex("z".to_owned())]
        );
    }

    #[test]
    fn malformed_inputs_never_panic() {
        // The five documented fuzz surfaces (§4.2) plus a deterministic
        // pseudo-random stream; `parse` must always return, never panic.
        let seeds = [
            r##"re(r#"unterminated"##,
            "jq(.a)) | text()",
            "jq(((.a)",
            r#"re("trailing\)"#,
            "re(\"中|文\") | text()",
            "r\"not a string start\"",
            "text() | ",
            "| text()",
            "",
            "re(r#####\"x\"#####)",
        ];
        for seed in seeds {
            let _ = parse(seed);
        }

        const PIECES: [&str; 14] = [
            "r", "#", "\"", "'", "(", ")", "|", "\\", " ", "\n", "jq", "text()", "中", ".",
        ];
        let mut state = 0x1234_5678_u64;
        for _ in 0..2000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let length = (state >> 33) as usize % 24;
            let mut input = String::new();
            for _ in 0..length {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                input.push_str(PIECES[(state >> 33) as usize % PIECES.len()]);
            }
            let _ = parse(&input);
        }
    }

    #[test]
    fn nesting_guard_rejects_deep_input_without_overflowing_the_stack() {
        let make = |depth: usize| format!("jq({}.a{})", "(".repeat(depth), ")".repeat(depth));
        assert!(parse(&make(8)).is_ok());
        assert!(parse(&make(127)).is_ok()); // plus the stage's own `(` = 128
        let error = parse(&make(1_000)).err().unwrap().to_string();
        assert!(error.contains("nesting exceeds"), "{error}");
    }

    #[test]
    fn long_flat_arguments_are_not_rejected_by_the_nesting_guard() {
        let long = "a".repeat(20_000);
        assert_eq!(
            parse(&format!("re(\"{long}\")")).unwrap(),
            vec![StageSpec::Regex(long)]
        );
    }

    #[test]
    fn structural_errors_carry_a_byte_position() {
        let error = parse("jq(.a | | text()").err().unwrap().to_string();
        assert!(error.contains("invalid pipeline at byte"), "{error}");
    }

    #[test]
    fn unterminated_raw_string_then_deep_parens_is_a_query_error_not_a_abort() {
        // Regression for a review finding: `check_nesting` `break`s on an
        // unterminated raw string, so the hundreds of trailing `(` are not
        // depth-counted. The grammar must still reject this input *without*
        // recursing into `group` (which would overflow the stack and abort the
        // process, bypassing 0/1/2). The unterminated `"` blocks `plain`/`group`,
        // so pest backtracks to "expected stage" — pin that safe behavior.
        let mut input = String::from("re(r#\"");
        input.push_str(&"(".repeat(2000));
        let error = parse(&input).expect_err("must be a Query error, not an abort");
        assert_eq!(error.exit_code(), 2, "{error}");
        assert!(
            error.to_string().contains("invalid pipeline at byte"),
            "{error}"
        );
    }
}
