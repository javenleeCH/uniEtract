mod jq;
mod regex;
mod xpath;

use crate::dsl::StageSpec;
use crate::error::AppError;
use crate::model::Value;
use jq::JqStage;
use regex::RegexStage;
use xpath::XPathStage;

pub enum CompiledStage {
    Jq(JqStage),
    XPath(XPathStage),
    Regex(RegexStage),
    Text,
    Attr(String),
}

impl CompiledStage {
    pub fn compile(
        spec: StageSpec,
        namespaces: &[(String, String)],
    ) -> Result<Self, AppError> {
        match spec {
            StageSpec::Jq(source) => Ok(Self::Jq(JqStage::compile(&source)?)),
            StageSpec::XPath(source) => {
                Ok(Self::XPath(XPathStage::compile(&source, namespaces)?))
            }
            StageSpec::Regex(source) => Ok(Self::Regex(RegexStage::compile(&source)?)),
            StageSpec::Text => Ok(Self::Text),
            StageSpec::Attr(name) => Ok(Self::Attr(name)),
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Jq(_) => "jq".to_owned(),
            Self::XPath(_) => "xpath".to_owned(),
            Self::Regex(_) => "re".to_owned(),
            Self::Text => "text".to_owned(),
            Self::Attr(name) => format!("attr({name})"),
        }
    }

    pub fn evaluate<'doc>(&self, value: Value<'doc>) -> Result<Vec<Value<'doc>>, String> {
        match self {
            Self::Jq(stage) => stage.evaluate(value),
            Self::XPath(stage) => stage.evaluate(value),
            Self::Regex(stage) => stage.evaluate(value),
            Self::Text => Ok(vec![Value::Text(value.string_value())]),
            Self::Attr(name) => {
                let Value::Node(node) = value else {
                    return Err("attr accepts element nodes only".to_owned());
                };
                let element = node
                    .element()
                    .ok_or_else(|| "attr accepts element nodes only".to_owned())?;
                Ok(element
                    .attribute_value(name.as_str())
                    .map(|value| vec![Value::Text(value.to_owned())])
                    .unwrap_or_default())
            }
        }
    }
}
