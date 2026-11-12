use winnow::ModalResult;
use winnow::Parser;
use winnow::token::take_while;

use crate::error::AppError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StageSpec {
    Jq(String),
    XPath(String),
    Regex(String),
    Text,
    Attr(String),
}

pub fn parse(input: &str) -> Result<Vec<StageSpec>, AppError> {
    split_pipeline(input)?
        .into_iter()
        .map(parse_stage)
        .collect()
}

fn split_pipeline(input: &str) -> Result<Vec<&str>, AppError> {
    #[derive(Clone, Copy)]
    enum State {
        Normal,
        Quoted(u8),
        Raw(usize),
    }

    let bytes = input.as_bytes();
    let mut state = State::Normal;
    let mut depth = 0_i32;
    let mut start = 0;
    let mut parts = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        match state {
            State::Normal => {
                if bytes[index] == b'r' {
                    if let Some((prefix_len, hashes)) = raw_prefix(&bytes[index..]) {
                        state = State::Raw(hashes);
                        index += prefix_len;
                        continue;
                    }
                }
                match bytes[index] {
                    b'\'' | b'"' => state = State::Quoted(bytes[index]),
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth < 0 {
                            return Err(AppError::Query(format!(
                                "unmatched ')' at byte {index}"
                            )));
                        }
                    }
                    b'|' if depth == 0 => {
                        let part = input[start..index].trim();
                        if part.is_empty() {
                            return Err(AppError::Query("empty pipeline stage".to_owned()));
                        }
                        parts.push(part);
                        start = index + 1;
                    }
                    _ => {}
                }
            }
            State::Quoted(delimiter) => match bytes[index] {
                b'\\' => index += 1,
                byte if byte == delimiter => state = State::Normal,
                _ => {}
            },
            State::Raw(hashes) => {
                if bytes[index] == b'"'
                    && bytes
                        .get(index + 1..index + 1 + hashes)
                        .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
                {
                    state = State::Normal;
                    index += hashes;
                }
            }
        }
        index += 1;
    }

    if !matches!(state, State::Normal) {
        return Err(AppError::Query("unterminated string literal".to_owned()));
    }
    if depth != 0 {
        return Err(AppError::Query("unclosed '(' in pipeline".to_owned()));
    }
    let last = input[start..].trim();
    if last.is_empty() {
        return Err(AppError::Query("empty pipeline stage".to_owned()));
    }
    parts.push(last);
    Ok(parts)
}

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

fn identifier<'input>(input: &mut &'input str) -> ModalResult<&'input str> {
    take_while(1.., |character: char| {
        character.is_ascii_alphabetic() || character == '_'
    })
    .parse_next(input)
}

fn parse_stage(stage: &str) -> Result<StageSpec, AppError> {
    let mut rest = stage;
    let name = identifier(&mut rest)
        .map_err(|_| AppError::Query(format!("invalid stage `{stage}`")))?;
    let rest = rest.trim_start();
    let inner = rest
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .ok_or_else(|| AppError::Query(format!("stage `{name}` requires parenthesized arguments")))?
        .trim();

    match name {
        "jq" if !inner.is_empty() => Ok(StageSpec::Jq(inner.to_owned())),
        "jq" => Err(AppError::Query("jq requires a filter".to_owned())),
        "xpath" => Ok(StageSpec::XPath(parse_string_argument(inner, "xpath")?)),
        "re" => Ok(StageSpec::Regex(parse_string_argument(inner, "re")?)),
        "text" if inner.is_empty() => Ok(StageSpec::Text),
        "text" => Err(AppError::Query("text() takes no arguments".to_owned())),
        "attr" => Ok(StageSpec::Attr(parse_string_argument(inner, "attr")?)),
        _ => Err(AppError::Query(format!("unknown stage `{name}`"))),
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
    Err(AppError::Query(format!(
        "unterminated string in `{stage}`"
    )))
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
}
