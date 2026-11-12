use regex::Regex;
use serde_json::{Map, Value as JsonValue};

use crate::error::AppError;
use crate::model::Value;

pub struct RegexStage {
    regex: Regex,
}

impl RegexStage {
    pub fn compile(source: &str) -> Result<Self, AppError> {
        let regex = Regex::new(source)
            .map_err(|error| AppError::Query(format!("invalid regular expression: {error}")))?;
        Ok(Self { regex })
    }

    pub fn evaluate(&self, value: Value<'_>) -> Result<Vec<Value<'static>>, String> {
        let Value::Text(text) = value else {
            return Err("re accepts text only; use text() first".to_owned());
        };
        if self.regex.captures_len() == 1 {
            return Ok(self
                .regex
                .find_iter(&text)
                .map(|found| Value::Text(found.as_str().to_owned()))
                .collect());
        }

        let mut output = Vec::new();
        for captures in self.regex.captures_iter(&text) {
            let mut object = Map::new();
            for index in 0..self.regex.captures_len() {
                object.insert(index.to_string(), capture_json(captures.get(index)));
            }
            for name in self.regex.capture_names().flatten() {
                object.insert(name.to_owned(), capture_json(captures.name(name)));
            }
            output.push(Value::Data(JsonValue::Object(object)));
        }
        Ok(output)
    }
}

fn capture_json(capture: Option<regex::Match<'_>>) -> JsonValue {
    capture
        .map(|value| JsonValue::String(value.as_str().to_owned()))
        .unwrap_or(JsonValue::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn emits_numeric_and_named_captures() {
        let stage = RegexStage::compile(r"(?<word>[a-z]+)(?:-(\d+))?").unwrap();
        let values = stage
            .evaluate(Value::Text("abc-12 xyz".to_owned()))
            .unwrap();
        assert_eq!(
            values,
            vec![
                Value::Data(json!({"0":"abc-12","1":"abc","2":"12","word":"abc"})),
                Value::Data(json!({"0":"xyz","1":"xyz","2":null,"word":"xyz"})),
            ]
        );
    }
}
