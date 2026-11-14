use std::io::{BufWriter, Write};

use serde_json::{Map, Value as JsonValue};

use crate::cli::OutputMode;
use crate::error::AppError;
use crate::model::{Item, ItemSink, source_json};

#[derive(Clone, Copy, Debug, Default)]
pub struct OutputOptions {
    pub compact: bool,
    pub line_buffered: bool,
    pub max_results: u64,
}

pub struct OutputSink<'writer> {
    writer: BufWriter<&'writer mut dyn Write>,
    mode: OutputMode,
    with_source: bool,
    options: OutputOptions,
    aggregate: Vec<JsonValue>,
    emitted: u64,
}

impl<'writer> OutputSink<'writer> {
    pub fn new(
        writer: &'writer mut dyn Write,
        mode: OutputMode,
        with_source: bool,
        options: OutputOptions,
    ) -> Self {
        Self {
            writer: BufWriter::with_capacity(64 * 1024, writer),
            mode,
            with_source,
            options,
            aggregate: Vec::new(),
            emitted: 0,
        }
    }

    pub fn emit(&mut self, item: Item<'_>) -> Result<(), AppError> {
        if self.options.max_results > 0 && self.emitted >= self.options.max_results {
            return Err(AppError::Runtime(format!(
                "--max-results ({}) exceeded: refusing to silently truncate output",
                self.options.max_results
            )));
        }
        self.emitted += 1;

        let value = if self.with_source {
            let mut wrapped = Map::new();
            wrapped.insert("value".to_owned(), item.value.into_json());
            wrapped.insert("source".to_owned(), source_json(&item.origin));
            JsonValue::Object(wrapped)
        } else if self.mode == OutputMode::Text {
            self.writer
                .write_all(item.value.string_value().as_bytes())?;
            self.writer.write_all(b"\n")?;
            return self.flush_if_line_buffered();
        } else {
            item.value.into_json()
        };

        if self.mode == OutputMode::Json {
            self.aggregate.push(value);
            return Ok(());
        }
        write_json_line(&mut self.writer, &value, false)?;
        self.flush_if_line_buffered()
    }

    pub fn finish(&mut self) -> Result<(), AppError> {
        if self.mode == OutputMode::Json {
            let value = JsonValue::Array(std::mem::take(&mut self.aggregate));
            write_json_line(&mut self.writer, &value, !self.options.compact)?;
        }
        self.writer.flush()?;
        Ok(())
    }

    fn flush_if_line_buffered(&mut self) -> Result<(), AppError> {
        if self.options.line_buffered {
            self.writer.flush()?;
        }
        Ok(())
    }
}

impl ItemSink for OutputSink<'_> {
    fn accept(&mut self, item: Item<'_>) -> Result<(), AppError> {
        self.emit(item)
    }
}

fn write_json_line(
    writer: &mut dyn Write,
    value: &JsonValue,
    pretty: bool,
) -> Result<(), AppError> {
    let encoded = if pretty {
        serde_json::to_vec_pretty(value)
    } else {
        serde_json::to_vec(value)
    }
    .map_err(|error| AppError::Runtime(format!("cannot serialize output: {error}")))?;
    writer.write_all(&encoded)?;
    // Output newlines are always LF, independent of the platform.
    writer.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Origin, Value};
    use serde_json::json;

    #[test]
    fn writes_source_wrapped_json_lines() {
        let mut bytes = Vec::new();
        let mut sink = OutputSink::new(
            &mut bytes,
            OutputMode::Jsonl,
            true,
            OutputOptions::default(),
        );
        sink.emit(Item::new(
            Value::Data(json!({"id": 7})),
            Origin::new(None, 2),
        ))
        .unwrap();
        sink.finish().unwrap();
        drop(sink);
        let output: JsonValue = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(output["value"], json!({"id": 7}));
        assert_eq!(output["source"]["record"], json!(2));
    }

    #[test]
    fn json_mode_is_pretty_by_default_and_compact_on_request() {
        let mut pretty = Vec::new();
        let mut sink = OutputSink::new(
            &mut pretty,
            OutputMode::Json,
            false,
            OutputOptions::default(),
        );
        sink.emit(Item::new(Value::Data(json!(1)), Origin::default()))
            .unwrap();
        sink.finish().unwrap();
        drop(sink);
        assert!(String::from_utf8(pretty).unwrap().contains("\n  1\n"));

        let mut compact = Vec::new();
        let mut sink = OutputSink::new(
            &mut compact,
            OutputMode::Json,
            false,
            OutputOptions {
                compact: true,
                ..OutputOptions::default()
            },
        );
        sink.emit(Item::new(Value::Data(json!(1)), Origin::default()))
            .unwrap();
        sink.finish().unwrap();
        drop(sink);
        assert_eq!(compact, b"[1]\n");
    }

    #[test]
    fn max_results_errors_instead_of_truncating() {
        let mut bytes = Vec::new();
        let mut sink = OutputSink::new(
            &mut bytes,
            OutputMode::Jsonl,
            false,
            OutputOptions {
                max_results: 1,
                ..OutputOptions::default()
            },
        );
        sink.emit(Item::new(Value::Data(json!(1)), Origin::default()))
            .unwrap();
        assert!(
            sink.emit(Item::new(Value::Data(json!(2)), Origin::default()))
                .is_err()
        );
    }

    #[test]
    fn output_uses_lf_only() {
        let mut bytes = Vec::new();
        let mut sink = OutputSink::new(
            &mut bytes,
            OutputMode::Text,
            false,
            OutputOptions::default(),
        );
        sink.emit(Item::new(
            Value::Text("a\r\nb".to_owned()),
            Origin::default(),
        ))
        .unwrap();
        sink.finish().unwrap();
        drop(sink);
        // The content keeps its CR, but the record terminator is a single LF.
        assert_eq!(bytes, b"a\r\nb\n");
    }
}
