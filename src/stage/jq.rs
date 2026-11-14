use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data, unwrap_valr};
use jaq_json::{Num, Val};
use serde_json::{Map, Number, Value as JsonValue};

use crate::error::AppError;
use crate::model::Value;

pub struct JqStage {
    filter: jaq_core::Filter<data::JustLut<Val>>,
}

impl JqStage {
    pub fn compile(source: &str) -> Result<Self, AppError> {
        let arena = Arena::default();
        let definitions = jaq_core::defs()
            .chain(jaq_std::defs())
            .chain(jaq_json::defs());
        let loader = Loader::new(definitions);
        let modules = loader
            .load(
                &arena,
                File {
                    path: (),
                    code: source,
                },
            )
            .map_err(|errors| AppError::Query(format!("invalid jq filter: {errors:?}")))?;
        let functions = jaq_core::funs()
            .chain(jaq_std::funs())
            .chain(jaq_json::funs());
        let filter = Compiler::default()
            .with_funs(functions)
            .compile(modules)
            .map_err(|errors| AppError::Query(format!("invalid jq filter: {errors:?}")))?;
        Ok(Self { filter })
    }

    pub fn evaluate(&self, value: Value<'_>) -> Result<Vec<Value<'static>>, String> {
        let json = match value {
            Value::Data(value) => value,
            Value::Text(value) => JsonValue::String(value),
            Value::Node(_) => return Err("jq accepts data or text, not node".to_owned()),
        };
        let input: Val = serde_json::from_value(json)
            .map_err(|error| format!("cannot convert input to jq value: {error}"))?;
        let context = Ctx::<data::JustLut<Val>>::new(&self.filter.lut, Vars::new([]));
        let results = self.filter.id.run((context, input)).map(unwrap_valr);
        let mut output = Vec::new();
        for result in results {
            let value = result.map_err(|error| format!("jq evaluation failed: {error}"))?;
            let json = val_to_json(&value)?;
            output.push(match json {
                JsonValue::String(value) => Value::Text(value),
                value => Value::Data(value),
            });
        }
        Ok(output)
    }
}

/// Convert a jaq value to JSON without a `Display` -> `from_str` round trip.
///
/// Printing a `Val` yields invalid JSON for non-finite floats and byte strings, and
/// re-parsing it loses the distinction between a number and its literal. Walking the
/// enum keeps the original numeric literal and lets us apply jq's `null` rule for
/// `NaN`/`+Inf`/`-Inf` at any nesting depth.
fn val_to_json(value: &Val) -> Result<JsonValue, String> {
    Ok(match value {
        Val::Null => JsonValue::Null,
        Val::Bool(value) => JsonValue::Bool(*value),
        Val::Num(number) => num_to_json(number)?,
        Val::TStr(bytes) | Val::BStr(bytes) => {
            let raw: &[u8] = (**bytes).as_ref();
            JsonValue::String(String::from_utf8_lossy(raw).into_owned())
        }
        Val::Arr(items) => JsonValue::Array(
            items
                .iter()
                .map(val_to_json)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Val::Obj(map) => {
            let mut object = Map::new();
            for (key, value) in map.iter() {
                let key = match key {
                    Val::TStr(bytes) | Val::BStr(bytes) => {
                        let raw: &[u8] = (**bytes).as_ref();
                        String::from_utf8_lossy(raw).into_owned()
                    }
                    other => other.to_string(),
                };
                object.insert(key, val_to_json(value)?);
            }
            JsonValue::Object(object)
        }
    })
}

fn num_to_json(number: &Num) -> Result<JsonValue, String> {
    let number = match number {
        Num::Int(value) => Number::from(*value as i64),
        Num::BigInt(value) => number_from_literal(&value.to_string())?,
        Num::Dec(value) => number_from_literal(value)?,
        Num::Float(value) => match Number::from_f64(*value) {
            Some(number) => number,
            None => return Ok(JsonValue::Null),
        },
    };
    Ok(JsonValue::Number(number))
}

fn number_from_literal(literal: &str) -> Result<Number, String> {
    serde_json::from_str::<Number>(literal).map_err(|error| {
        format!("jq number cannot be represented as JSON: {error}; value: {literal}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn eval(filter: &str, input: JsonValue) -> Vec<Value<'static>> {
        JqStage::compile(filter)
            .unwrap()
            .evaluate(Value::Data(input))
            .unwrap()
    }

    #[test]
    fn preserves_arbitrary_precision_numbers() {
        let input: JsonValue =
            serde_json::from_str(r#"{"n":123456789012345678901234567890,"f":1.5}"#).unwrap();
        assert_eq!(eval(".", input.clone()), vec![Value::Data(input)]);
    }

    #[test]
    fn maps_non_finite_floats_to_null_like_jq() {
        assert_eq!(
            eval("nan", JsonValue::Null),
            vec![Value::Data(JsonValue::Null)]
        );
        assert_eq!(
            eval("infinite", JsonValue::Null),
            vec![Value::Data(JsonValue::Null)]
        );
        assert_eq!(
            eval("[nan, infinite]", JsonValue::Null),
            vec![Value::Data(json!([null, null]))]
        );
    }

    #[test]
    fn json_strings_become_text_values() {
        assert_eq!(
            eval(".name", json!({"name": "alice"})),
            vec![Value::Text("alice".to_owned())]
        );
    }
}
