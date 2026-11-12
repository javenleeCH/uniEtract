use std::io::Write;

use serde_json::{Map, Value as JsonValue};

use crate::cli::OutputMode;
use crate::error::AppError;
use crate::model::{Item, source_json};

pub struct OutputSink<'writer> {
    writer: &'writer mut dyn Write,
    mode: OutputMode,
    with_source: bool,
    aggregate: Vec<JsonValue>,
}

impl<'writer> OutputSink<'writer> {
    pub fn new(
        writer: &'writer mut dyn Write,
        mode: OutputMode,
        with_source: bool,
    ) -> Self {
        Self {
            writer,
            mode,
            with_source,
            aggregate: Vec::new(),
        }
    }

    pub fn emit(&mut self, item: Item<'_>) -> Result<(), AppError> {
        let value = if self.with_source {
            let mut wrapped = Map::new();
            wrapped.insert("value".to_owned(), item.value.into_json());
            wrapped.insert("source".to_owned(), source_json(&item.origin));
            JsonValue::Object(wrapped)
        } else if self.mode == OutputMode::Text {
            self.writer.write_all(item.value.string_value().as_bytes())?;
            self.writer.write_all(b"\n")?;
            self.writer.flush()?;
            return Ok(());
        } else {
            item.value.into_json()
        };

        if self.mode == OutputMode::Json {
            self.aggregate.push(value);
            return Ok(());
        }
        write_json_line(self.writer, &value)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), AppError> {
        if self.mode == OutputMode::Json {
            let value = JsonValue::Array(std::mem::take(&mut self.aggregate));
            write_json_line(self.writer, &value)?;
        }
        self.writer.flush()?;
        Ok(())
    }
}

fn write_json_line(writer: &mut dyn Write, value: &JsonValue) -> Result<(), AppError> {
    let encoded = serde_json::to_vec(value)
        .map_err(|error| AppError::Runtime(format!("cannot serialize output: {error}")))?;
    writer.write_all(&encoded)?;
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
        let mut sink = OutputSink::new(&mut bytes, OutputMode::Jsonl, true);
        sink.emit(Item::new(
            Value::Data(json!({"id": 7})),
            Origin::new(None, 2),
        ))
        .unwrap();
        sink.finish().unwrap();
        let output: JsonValue = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(output["value"], json!({"id": 7}));
        assert_eq!(output["source"]["record"], json!(2));
    }
}
