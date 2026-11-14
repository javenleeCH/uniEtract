use serde_json::Value as JsonValue;

use crate::cli::InputFormat;
use crate::dsl;
use crate::error::AppError;
use crate::model::{Item, ItemSink, Origin, Value};
use crate::stage::CompiledStage;
use crate::types::{Limits, source_types};

pub struct Pipeline {
    stages: Vec<CompiledStage>,
    boundary: Option<usize>,
}

impl Pipeline {
    pub fn compile(
        source: &str,
        namespaces: &[(String, String)],
        format: InputFormat,
        limits: Limits,
    ) -> Result<Self, AppError> {
        let specifications = dsl::parse(source)?;
        let stages = specifications
            .into_iter()
            .map(|specification| CompiledStage::compile(specification, namespaces, limits))
            .collect::<Result<Vec<_>, _>>()?;

        let boundaries = stages
            .iter()
            .enumerate()
            .filter(|(_, stage)| stage.is_boundary())
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if boundaries.len() > 1 {
            return Err(AppError::Query(
                "slurp() may appear at most once in a pipeline".to_owned(),
            ));
        }

        // Static upper-bound check: reject stage chains that are provably impossible.
        // A non-empty intersection does NOT guarantee runtime compatibility.
        for (index, pair) in stages.windows(2).enumerate() {
            let produced = pair[0].output_types();
            let required = pair[1].input_types();
            if !produced.intersects(required) {
                return Err(AppError::Query(format!(
                    "stage {} `{}` can only produce {}, but stage {} `{}` requires {}",
                    index + 1,
                    pair[0].name(),
                    produced.describe(),
                    index + 2,
                    pair[1].name(),
                    required.describe()
                )));
            }
        }
        if let (Some(source), Some(first)) = (source_types(format), stages.first())
            && !source.intersects(first.input_types())
        {
            return Err(AppError::Query(format!(
                "-f {format:?} produces {}, but the first stage `{}` requires {}",
                source.describe(),
                first.name(),
                first.input_types().describe()
            )));
        }

        Ok(Self {
            stages,
            boundary: boundaries.first().copied(),
        })
    }

    /// Static, no-trial-run predicate backing `--skip-incompatible`.
    pub fn accepts_source(&self, format: InputFormat) -> bool {
        match (source_types(format), self.stages.first()) {
            (Some(source), Some(first)) => source.intersects(first.input_types()),
            _ => true,
        }
    }

    pub fn execute<'doc>(
        &self,
        item: Item<'doc>,
        sink: &mut dyn FnMut(Item<'doc>) -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        self.execute_range(0, self.stages.len(), item, sink)
    }

    pub fn session(&self) -> Option<Session<'_>> {
        self.boundary.map(|_| Session {
            pipeline: self,
            buffer: Vec::new(),
            first_origin: None,
        })
    }

    fn execute_range<'doc>(
        &self,
        from: usize,
        to: usize,
        item: Item<'doc>,
        sink: &mut dyn FnMut(Item<'doc>) -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        self.execute_from(from, to, item, sink)
    }

    fn execute_from<'doc>(
        &self,
        index: usize,
        to: usize,
        item: Item<'doc>,
        sink: &mut dyn FnMut(Item<'doc>) -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        if index >= to {
            return sink(item);
        }
        let stage = &self.stages[index];
        let origin = item.origin.clone();
        let values = stage
            .evaluate(item.value)
            .map_err(|message| AppError::eval(&origin, stage.name(), message))?;
        for value in values {
            self.execute_from(index + 1, to, Item::new(value, origin.clone()), sink)?;
        }
        Ok(())
    }
}

/// Aggregation context spanning *all inputs of one run*. Values are materialized into
/// `JsonValue` at feed time so no `Node` borrow can escape its document (invariant I1).
pub struct Session<'p> {
    pipeline: &'p Pipeline,
    buffer: Vec<JsonValue>,
    first_origin: Option<Origin>,
}

impl ItemSink for Session<'_> {
    fn accept(&mut self, item: Item<'_>) -> Result<(), AppError> {
        self.feed(item)
    }
}

/// Adapts a whole-pipeline execution to an `ItemSink`. Used when there is no
/// boundary stage; with `slurp()` the input layer feeds the `Session` directly.
pub struct Relay<'downstream, 'pipeline> {
    pub pipeline: &'pipeline Pipeline,
    pub downstream: &'downstream mut dyn ItemSink,
}

impl ItemSink for Relay<'_, '_> {
    fn accept(&mut self, item: Item<'_>) -> Result<(), AppError> {
        self.pipeline
            .execute(item, &mut |produced| self.downstream.accept(produced))
    }
}

impl Session<'_> {
    pub fn feed(&mut self, item: Item<'_>) -> Result<(), AppError> {
        let boundary = self
            .pipeline
            .boundary
            .expect("session requires a slurp() stage");
        let buffer = &mut self.buffer;
        let first_origin = &mut self.first_origin;
        self.pipeline
            .execute_range(0, boundary, item, &mut |produced| {
                if first_origin.is_none() {
                    *first_origin = Some(produced.origin.clone());
                }
                buffer.push(produced.value.into_json());
                Ok(())
            })
    }

    pub fn finish(self, sink: &mut dyn ItemSink) -> Result<(), AppError> {
        let Session {
            pipeline,
            buffer,
            first_origin,
        } = self;
        let boundary = pipeline.boundary.expect("session requires a slurp() stage");
        // Zero input still yields one `[]`, matching `jq -s`.
        let item = Item::new(
            Value::Data(JsonValue::Array(buffer)),
            first_origin.unwrap_or_default(),
        );
        pipeline.execute_range(boundary + 1, pipeline.stages.len(), item, &mut |produced| {
            sink.accept(produced)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(pipeline: &str, format: InputFormat) -> Result<Pipeline, AppError> {
        Pipeline::compile(pipeline, &[], format, Limits::default())
    }

    #[test]
    fn rejects_provably_impossible_stage_chains() {
        let error = compile("slurp() | xpath(\"//a\")", InputFormat::Jsonl)
            .err()
            .unwrap();
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("requires Node"));
        assert!(compile("jq(.) | xpath(\"//a\")", InputFormat::Json).is_err());
        assert!(compile("text() | xpath(\"//a\")", InputFormat::Text).is_err());
    }

    #[test]
    fn accepts_possibly_compatible_chains() {
        assert!(compile("jq(.) | re(r\"a\")", InputFormat::Json).is_ok());
        assert!(compile("slurp() | jq(length)", InputFormat::Jsonl).is_ok());
    }

    #[test]
    fn rejects_explicit_format_conflicts_before_reading_input() {
        let error = compile("xpath(\"//a\")", InputFormat::Json).err().unwrap();
        assert!(error.to_string().contains("produces Data"));
    }

    #[test]
    fn rejects_multiple_slurp_stages() {
        assert!(compile("slurp() | jq(.) | slurp()", InputFormat::Jsonl).is_err());
    }

    #[test]
    fn skip_incompatible_uses_the_first_stage_only() {
        let pipeline = compile("xpath(\"//a\") | text()", InputFormat::Auto).unwrap();
        assert!(!pipeline.accepts_source(InputFormat::Json));
        assert!(pipeline.accepts_source(InputFormat::Xml));
    }
}
