use std::collections::HashSet;
use std::io::{BufRead, Read};

use serde::Deserialize;
use serde_json::{Map, Value as JsonValue};

use crate::cli::{Cli, InputFormat, TextMode};
use crate::error::AppError;
use crate::model::{Item, ItemSink, Origin, Value};
use crate::source::InputSource;

pub fn process_source(
    source: &mut InputSource<'_>,
    cli: &Cli,
    sink: &mut dyn ItemSink,
) -> Result<(), AppError> {
    let format = source.format;
    let max_input_size = source.max_input_size;
    let max_record_size = source.max_record_size;
    let base = Origin::new(source.path.clone(), 1).with_captures(source.captures.clone());
    let reader = source.reader();
    match format {
        InputFormat::Json => process_json(reader, &base, max_input_size, sink),
        InputFormat::Jsonl => process_jsonl(reader, &base, max_record_size, sink),
        InputFormat::Text => process_text(
            reader,
            cli.text_mode,
            &base,
            max_input_size,
            max_record_size,
            sink,
        ),
        InputFormat::Xml | InputFormat::Html => {
            let source = read_utf8_limited(reader, &base, max_input_size)?;
            crate::markup::process(&source, format, &base, sink)
        }
        InputFormat::Yaml => process_yaml(reader, &base, max_input_size, sink),
        InputFormat::Toml => process_toml(reader, &base, max_input_size, sink),
        InputFormat::Csv => process_csv(reader, b',', "CSV", &base, max_record_size, sink),
        InputFormat::Tsv => process_csv(reader, b'\t', "TSV", &base, max_record_size, sink),
        InputFormat::Auto => Err(AppError::Runtime(format!(
            "{base}: internal input format detection failure"
        ))),
    }
}

fn process_json(
    reader: &mut dyn BufRead,
    base: &Origin,
    limit: u64,
    sink: &mut dyn ItemSink,
) -> Result<(), AppError> {
    let source = read_utf8_limited(reader, base, limit)?;
    let value = serde_json::from_str(&source)
        .map_err(|error| AppError::Runtime(format!("{base}: invalid JSON: {error}")))?;
    emit(Value::Data(value), base, sink)
}

fn process_jsonl(
    reader: &mut dyn BufRead,
    base: &Origin,
    limit: u64,
    sink: &mut dyn ItemSink,
) -> Result<(), AppError> {
    let mut line_number = 0_u64;
    let mut record = 0_u64;
    while let Some(line) = next_line(reader, limit, base, line_number + 1)? {
        line_number += 1;
        if line.trim().is_empty() {
            continue;
        }
        record += 1;
        let origin = base.with_record(record).at_line(line_number);
        let value = serde_json::from_str(&line).map_err(|error| {
            AppError::Runtime(format!("{origin}: invalid JSONL record: {error}"))
        })?;
        emit(Value::Data(value), &origin, sink)?;
    }
    Ok(())
}

fn process_text(
    reader: &mut dyn BufRead,
    mode: TextMode,
    base: &Origin,
    input_limit: u64,
    record_limit: u64,
    sink: &mut dyn ItemSink,
) -> Result<(), AppError> {
    if mode == TextMode::Raw {
        let source = read_utf8_limited(reader, base, input_limit)?;
        return emit(Value::Text(source), base, sink);
    }
    let mut line_number = 0_u64;
    // Empty lines are records too (§5.6), so they are emitted, not skipped.
    while let Some(line) = next_line(reader, record_limit, base, line_number + 1)? {
        line_number += 1;
        let origin = base.with_record(line_number).at_line(line_number);
        emit(Value::Text(line), &origin, sink)?;
    }
    Ok(())
}

fn process_yaml(
    reader: &mut dyn BufRead,
    base: &Origin,
    limit: u64,
    sink: &mut dyn ItemSink,
) -> Result<(), AppError> {
    let source = read_utf8_limited(reader, base, limit)?;
    for (index, document) in serde_yaml_ng::Deserializer::from_str(&source).enumerate() {
        let origin = base.with_record(index as u64 + 1);
        let yaml = serde_yaml_ng::Value::deserialize(document)
            .map_err(|error| AppError::Runtime(format!("{origin}: invalid YAML: {error}")))?;
        let value = yaml_to_json(yaml)
            .map_err(|message| AppError::Runtime(format!("{origin}: {message}")))?;
        emit(Value::Data(value), &origin, sink)?;
    }
    Ok(())
}

fn yaml_to_json(value: serde_yaml_ng::Value) -> Result<JsonValue, String> {
    match value {
        serde_yaml_ng::Value::Null => Ok(JsonValue::Null),
        serde_yaml_ng::Value::Bool(value) => Ok(JsonValue::Bool(value)),
        serde_yaml_ng::Value::Number(value) => serde_json::to_value(value)
            .map_err(|error| format!("YAML number cannot be represented as JSON: {error}")),
        serde_yaml_ng::Value::String(value) => Ok(JsonValue::String(value)),
        serde_yaml_ng::Value::Sequence(values) => values
            .into_iter()
            .map(yaml_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        serde_yaml_ng::Value::Mapping(values) => {
            let mut object = Map::new();
            for (key, value) in values {
                let serde_yaml_ng::Value::String(key) = key else {
                    return Err("YAML mapping keys must be strings".to_owned());
                };
                object.insert(key, yaml_to_json(value)?);
            }
            Ok(JsonValue::Object(object))
        }
        serde_yaml_ng::Value::Tagged(value) => yaml_to_json(value.value),
    }
}

fn process_toml(
    reader: &mut dyn BufRead,
    base: &Origin,
    limit: u64,
    sink: &mut dyn ItemSink,
) -> Result<(), AppError> {
    let source = read_utf8_limited(reader, base, limit)?;
    let value: toml::Value = toml::from_str(&source)
        .map_err(|error| AppError::Runtime(format!("{base}: invalid TOML: {error}")))?;
    let value =
        toml_to_json(value).map_err(|message| AppError::Runtime(format!("{base}: {message}")))?;
    emit(Value::Data(value), base, sink)
}

fn toml_to_json(value: toml::Value) -> Result<JsonValue, String> {
    match value {
        toml::Value::String(value) => Ok(JsonValue::String(value)),
        toml::Value::Integer(value) => Ok(JsonValue::Number(value.into())),
        toml::Value::Float(value) => serde_json::Number::from_f64(value)
            .map(JsonValue::Number)
            .ok_or_else(|| "TOML NaN or infinite number cannot be represented as JSON".to_owned()),
        toml::Value::Boolean(value) => Ok(JsonValue::Bool(value)),
        toml::Value::Datetime(value) => Ok(JsonValue::String(value.to_string())),
        toml::Value::Array(values) => values
            .into_iter()
            .map(toml_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        toml::Value::Table(values) => {
            let mut object = Map::new();
            for (key, value) in values {
                object.insert(key, toml_to_json(value)?);
            }
            Ok(JsonValue::Object(object))
        }
    }
}

fn process_csv(
    reader: &mut dyn BufRead,
    delimiter: u8,
    label: &str,
    base: &Origin,
    record_limit: u64,
    sink: &mut dyn ItemSink,
) -> Result<(), AppError> {
    let mut csv = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(reader);
    let headers = csv
        .headers()
        .map_err(|error| AppError::Runtime(format!("{base}: invalid {label} header: {error}")))?
        .clone();
    if headers.is_empty() {
        return Ok(());
    }
    let mut seen = HashSet::new();
    for header in &headers {
        if header.is_empty() {
            return Err(AppError::Runtime(format!(
                "{base}: {label} headers must not be empty"
            )));
        }
        if !seen.insert(header.to_owned()) {
            return Err(AppError::Runtime(format!(
                "{base}: duplicate {label} header `{header}`"
            )));
        }
    }

    for (index, record) in csv.records().enumerate() {
        let origin = base.with_record(index as u64 + 1);
        let record = record.map_err(|error| {
            AppError::Runtime(format!("{origin}: invalid {label} record: {error}"))
        })?;
        let origin = record
            .position()
            .map_or(origin.clone(), |position| origin.at_line(position.line()));
        if record_limit > 0 && record.as_slice().len() as u64 > record_limit {
            return Err(AppError::Runtime(format!(
                "{origin}: {label} record exceeds --max-record-size ({record_limit} bytes)"
            )));
        }
        let mut object = Map::new();
        for (header, field) in headers.iter().zip(record.iter()) {
            object.insert(header.to_owned(), JsonValue::String(field.to_owned()));
        }
        emit(Value::Data(JsonValue::Object(object)), &origin, sink)?;
    }
    Ok(())
}

pub(crate) fn emit(
    value: Value<'_>,
    origin: &Origin,
    sink: &mut dyn ItemSink,
) -> Result<(), AppError> {
    sink.accept(Item::new(value, origin.clone()))
}

fn read_utf8_limited(
    reader: &mut dyn BufRead,
    base: &Origin,
    limit: u64,
) -> Result<String, AppError> {
    let mut bytes = Vec::new();
    if limit == 0 {
        reader
            .read_to_end(&mut bytes)
            .map_err(|error| AppError::Runtime(format!("{base}: cannot read input: {error}")))?;
    } else {
        let mut taken = reader.take(limit.saturating_add(1));
        taken
            .read_to_end(&mut bytes)
            .map_err(|error| AppError::Runtime(format!("{base}: cannot read input: {error}")))?;
        if bytes.len() as u64 > limit {
            return Err(AppError::Runtime(format!(
                "{base}: whole-document input exceeds --max-input-size ({limit} bytes)"
            )));
        }
    }
    String::from_utf8(bytes).map_err(|error| {
        AppError::Runtime(format!(
            "{base}: input is not valid UTF-8 at byte {}: use --encoding auto or --encoding gbk",
            error.utf8_error().valid_up_to()
        ))
    })
}

/// Reads one logical line, stripping the trailing LF and (for CRLF) CR, and
/// enforcing `--max-record-size` before the whole line is materialized.
fn next_line(
    reader: &mut dyn BufRead,
    limit: u64,
    base: &Origin,
    line_number: u64,
) -> Result<Option<String>, AppError> {
    let mut bytes = Vec::new();
    let read = if limit == 0 {
        reader.read_until(b'\n', &mut bytes)?
    } else {
        let mut taken = (&mut *reader).take(limit.saturating_add(1));
        taken.read_until(b'\n', &mut bytes)?
    };
    if read == 0 {
        return Ok(None);
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    if limit > 0 && bytes.len() as u64 > limit {
        return Err(AppError::Runtime(format!(
            "{}: line exceeds --max-record-size ({limit} bytes)",
            base.at_line(line_number)
        )));
    }
    let line = String::from_utf8(bytes).map_err(|error| {
        AppError::Runtime(format!(
            "{}: cannot read UTF-8 line at byte {}: use --encoding auto or --encoding gbk",
            base.at_line(line_number),
            error.utf8_error().valid_up_to()
        ))
    })?;
    Ok(Some(line))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn next_line_strips_crlf_and_enforces_limit() {
        let mut reader = Cursor::new(b"one\r\ntwo\n".to_vec());
        let base = Origin::default();
        assert_eq!(
            next_line(&mut reader, 0, &base, 1).unwrap(),
            Some("one".to_owned())
        );
        assert_eq!(
            next_line(&mut reader, 0, &base, 2).unwrap(),
            Some("two".to_owned())
        );
        assert_eq!(next_line(&mut reader, 0, &base, 3).unwrap(), None);

        let mut short = Cursor::new(b"abcdef\n".to_vec());
        assert!(next_line(&mut short, 3, &base, 1).is_err());
    }

    #[test]
    fn read_utf8_limited_reports_overflow() {
        let mut reader = Cursor::new(b"123456".to_vec());
        assert!(read_utf8_limited(&mut reader, &Origin::default(), 3).is_err());
    }
}
