use crate::dsl;
use crate::error::AppError;
use crate::model::Item;
use crate::stage::CompiledStage;

pub struct Pipeline {
    stages: Vec<CompiledStage>,
}

impl Pipeline {
    pub fn compile(source: &str, namespaces: &[(String, String)]) -> Result<Self, AppError> {
        let specifications = dsl::parse(source)?;
        let stages = specifications
            .into_iter()
            .map(|specification| CompiledStage::compile(specification, namespaces))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { stages })
    }

    pub fn execute<'doc>(
        &self,
        item: Item<'doc>,
        sink: &mut dyn FnMut(Item<'doc>) -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        self.execute_from(0, item, sink)
    }

    fn execute_from<'doc>(
        &self,
        index: usize,
        item: Item<'doc>,
        sink: &mut dyn FnMut(Item<'doc>) -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let Some(stage) = self.stages.get(index) else {
            return sink(item);
        };
        let origin = item.origin.clone();
        let values = stage
            .evaluate(item.value)
            .map_err(|message| AppError::eval(&origin, stage.name(), message))?;
        for value in values {
            self.execute_from(
                index + 1,
                Item::new(value, origin.clone()),
                sink,
            )?;
        }
        Ok(())
    }
}
