use serde_json::Number;
use sxd_xpath::{Context, Factory, Value as XPathValue, XPath};

use crate::error::AppError;
use crate::model::Value;

pub struct XPathStage {
    expression: XPath,
    namespaces: Vec<(String, String)>,
}

impl XPathStage {
    pub fn compile(source: &str, namespaces: &[(String, String)]) -> Result<Self, AppError> {
        let expression = Factory::new()
            .build(source)
            .map_err(|error| AppError::Query(format!("invalid XPath: {error}")))?
            .ok_or_else(|| AppError::Query("XPath expression must not be empty".to_owned()))?;
        Ok(Self {
            expression,
            namespaces: namespaces.to_vec(),
        })
    }

    pub fn evaluate<'doc>(&self, value: Value<'doc>) -> Result<Vec<Value<'doc>>, String> {
        let Value::Node(node) = value else {
            return Err("xpath accepts nodes only".to_owned());
        };
        let mut context = Context::new();
        for (prefix, uri) in &self.namespaces {
            context.set_namespace(prefix, uri);
        }
        let result = self
            .expression
            .evaluate(&context, node)
            .map_err(|error| format!("XPath evaluation failed: {error}"))?;
        match result {
            XPathValue::Nodeset(nodes) => Ok(nodes
                .document_order()
                .into_iter()
                .map(Value::Node)
                .collect()),
            XPathValue::String(value) => Ok(vec![Value::Text(value)]),
            XPathValue::Boolean(value) => Ok(vec![Value::Data(value.into())]),
            XPathValue::Number(value) => {
                let number = Number::from_f64(value).ok_or_else(|| {
                    "XPath NaN or infinite number cannot be represented as JSON".to_owned()
                })?;
                Ok(vec![Value::Data(number.into())])
            }
        }
    }
}
