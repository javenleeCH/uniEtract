use std::collections::HashSet;
use std::io::{BufRead, Read};
use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Map, Value as JsonValue};

use crate::cli::{InputFormat, TextMode};
use crate::engine::Pipeline;
use crate::error::AppError;
use crate::model::{Item, Origin, Value};
use crate::output::OutputSink;

pub fn process_reader(
    reader: &mut dyn BufRead,
    requested_format: InputFormat,
    text_mode: TextMode,
    path: Option<Arc<std::path::PathBuf>>,
    pipeline: &Pipeline,
    output: &mut OutputSink<'_>,
) -> Result<(), AppError> {
    let format = if requested_format == InputFormat::Auto {
        detect_format(path.as_ref().map(|path| path.as_path()), reader.fill_buf()?)
    } else {
        requested_format
    };
    let base = Origin::new(path, 1);
    match format {
        InputFormat::Json => process_json(reader, &base, pipeline, output),
        InputFormat::Jsonl => process_jsonl(reader, &base, pipeline, output),
        InputFormat::Text => process_text(reader, text_mode, &base, pipeline, output),
        InputFormat::Xml | InputFormat::Html => {
            let source = read_utf8(reader, &base)?;
            crate::markup::process(&source, format, &base, pipeline, output)
        }
        InputFormat::Yaml => process_yaml(reader, &base, pipeline, output),
        InputFormat::Toml => process_toml(reader, &base, pipeline, output),
        InputFormat::Csv => process_csv(reader, b',', "CSV", &base, pipeline, output),
        InputFormat::Tsv => process_csv(reader, b'\t', "TSV", &base, pipeline, output),
        InputFormat::Auto => Err(AppError::Runtime(format!(
            "{base}: internal input format detection failure"
        ))),
    }
}

fn process_json(
    reader: &mut dyn BufRead,
    base: &Origin,
    pipeline: &Pipeline,
    output: &mut OutputSink<'_>,
) -> Result<(), AppError> {
    let source = read_utf8(reader, base)?;
    let value = serde_json::from_str(&source)
        .map_err(|error| AppError::Runtime(format!("{base}: invalid JSON: {error}")))?;
    emit(Value::Data(value), base, pipeline, output)
}

fn process_jsonl(
    reader: &mut dyn BufRead,
    base: &Origin,
    pipeline: &Pipeline,
    output: &mut OutputSink<'_>,
) -> Result<(), AppError> {
    let mut record = 0;
    for (index, line) in reader.lines().enumerate() {
        let line_number = index as u64 + 1;
        let line = line.map_err(|error| {
            AppError::Runtime(format!("{}: cannot read UTF-8 line: {error}", base.at_line(line_number)))
        })?;
        if line.trim().is_empty() {
            continue;
        }
        record += 1;
        let origin = Origin::at_line(&Origin::new(base.path.clone(), record), line_number);
        let value = serde_json::from_str(&line).map_err(|error| {
            AppError::Runtime(format!("{origin}: invalid JSONL record: {error}"))
        })?;
        emit(Value::Data(value), &origin, pipeline, output)?;
    }
    Ok(())
}

fn process_text(
    reader: &mut dyn BufRead,
    mode: TextMode,
    base: &Origin,
    pipeline: &Pipeline,
    output: &mut OutputSink<'_>,
) -> Result<(), AppError> {
    if mode == TextMode::Raw {
        let source = read_utf8(reader, base)?;
        return emit(Value::Text(source), base, pipeline, output);
    }
    for (index, line) in reader.lines().enumerate() {
        let line_number = index as u64 + 1;
        let line = line.map_err(|error| {
            AppError::Runtime(format!("{}: cannot read UTF-8 line: {error}", base.at_line(line_number)))
        })?;
        let origin = Origin::at_line(&Origin::new(base.path.clone(), line_number), line_number);
        emit(Value::Text(line), &origin, pipeline, output)?;
    }
    Ok(())
}

fn process_yaml(
    reader: &mut dyn BufRead,
    base: &Origin,
    pipeline: &Pipeline,
    output: &mut OutputSink<'_>,
) -> Result<(), AppError> {
    let source = read_utf8(reader, base)?;
    for (index, document) in serde_yaml_ng::Deserializer::from_str(&source).enumerate() {
        let origin = Origin::new(base.path.clone(), index as u64 + 1);
        let yaml = serde_yaml_ng::Value::deserialize(document).map_err(|error| {
            AppError::Runtime(format!("{origin}: invalid YAML: {error}"))
        })?;
        let value = yaml_to_json(yaml)
            .map_err(|message| AppError::Runtime(format!("{origin}: {message}")))?;
        emit(Value::Data(value), &origin, pipeline, output)?;
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
    pipeline: &Pipeline,
    output: &mut OutputSink<'_>,
) -> Result<(), AppError> {
    let source = read_utf8(reader, base)?;
    let value: toml::Value = toml::from_str(&source)
        .map_err(|error| AppError::Runtime(format!("{base}: invalid TOML: {error}")))?;
    let value = toml_to_json(value)
        .map_err(|message| AppError::Runtime(format!("{base}: {message}")))?;
    emit(Value::Data(value), base, pipeline, output)
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
    pipeline: &Pipeline,
    output: &mut OutputSink<'_>,
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
        let origin = Origin::new(base.path.clone(), index as u64 + 1);
        let record = record.map_err(|error| {
            AppError::Runtime(format!("{origin}: invalid {label} record: {error}"))
        })?;
        let origin = record.position().map_or(origin.clone(), |position| {
            origin.at_line(position.line())
        });
        let mut object = Map::new();
        for (header, field) in headers.iter().zip(record.iter()) {
            object.insert(header.to_owned(), JsonValue::String(field.to_owned()));
        }
        emit(
            Value::Data(JsonValue::Object(object)),
            &origin,
            pipeline,
            output,
        )?;
    }
    Ok(())
}

pub(crate) fn emit(
    value: Value<'_>,
    origin: &Origin,
    pipeline: &Pipeline,
    output: &mut OutputSink<'_>,
) -> Result<(), AppError> {
    pipeline.execute(Item::new(value, origin.clone()), &mut |item| output.emit(item))
}

fn read_utf8(reader: &mut dyn Read, base: &Origin) -> Result<String, AppError> {
    let mut source = String::new();
    reader.read_to_string(&mut source).map_err(|error| {
        AppError::Runtime(format!("{base}: input is not valid UTF-8: {error}"))
    })?;
    Ok(source)
}

fn detect_format(path: Option<&Path>, prefix: &[u8]) -> InputFormat {
    if let Some(extension) = path
        .and_then(Path::extension)
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
    {
        let known = match extension.as_str() {
            "json" => Some(InputFormat::Json),
            "jsonl" | "ndjson" => Some(InputFormat::Jsonl),
            "yaml" | "yml" => Some(InputFormat::Yaml),
            "toml" => Some(InputFormat::Toml),
            "xml" => Some(InputFormat::Xml),
            "html" | "htm" => Some(InputFormat::Html),
            "txt" | "log" => Some(InputFormat::Text),
            "csv" => Some(InputFormat::Csv),
            "tsv" | "tab" => Some(InputFormat::Tsv),
            _ => None,
        };
        if let Some(known) = known {
            return known;
        }
    }

    let start = prefix
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(prefix.len());
    let prefix = &prefix[start..];
    match prefix.first() {
        Some(b'{') | Some(b'[') => InputFormat::Json,
        Some(b'<') => {
            let beginning = String::from_utf8_lossy(&prefix[..prefix.len().min(128)])
                .to_ascii_lowercase();
            if beginning.contains("<!doctype html") || beginning.contains("<html") {
                InputFormat::Html
            } else {
                InputFormat::Xml
            }
        }
        _ => InputFormat::Text,
    }
}
