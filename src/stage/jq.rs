use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data, unwrap_valr};
use jaq_json::Val;

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
            .load(&arena, File { path: (), code: source })
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
            Value::Text(value) => serde_json::Value::String(value),
            Value::Node(_) => return Err("jq accepts data or text, not node".to_owned()),
        };
        let input: Val = serde_json::from_value(json)
            .map_err(|error| format!("cannot convert input to jq value: {error}"))?;
        let context = Ctx::<data::JustLut<Val>>::new(&self.filter.lut, Vars::new([]));
        let results = self.filter.id.run((context, input)).map(unwrap_valr);
        let mut output = Vec::new();
        for result in results {
            let value = result.map_err(|error| format!("jq evaluation failed: {error}"))?;
            let serialized = value.to_string();
            let json = serde_json::from_str(&serialized).map_err(|error| {
                format!("jq result cannot be represented as JSON: {error}; value: {serialized}")
            })?;
            output.push(match json {
                serde_json::Value::String(value) => Value::Text(value),
                value => Value::Data(value),
            });
        }
        Ok(output)
    }
}
