mod jq;
mod regex;
mod xpath;

use ::regex::Regex as RegexCrate;
use sxd_xpath::nodeset::Node as XPathNode;

use crate::dsl::StageSpec;
use crate::error::AppError;
use crate::model::Value;
use crate::types::{Limits, TypeSet};
use jq::JqStage;
use regex::RegexStage;
use xpath::XPathStage;

pub struct SplitStage {
    pattern: RegexCrate,
    max_record_size: u64,
}

impl SplitStage {
    pub fn compile(source: &str, limits: Limits) -> Result<Self, AppError> {
        let pattern = RegexCrate::new(source)
            .map_err(|error| AppError::Query(format!("invalid split pattern: {error}")))?;
        Ok(Self {
            pattern,
            max_record_size: limits.max_record_size,
        })
    }

    fn evaluate(&self, value: Value<'_>) -> Result<Vec<Value<'static>>, String> {
        let Value::Text(text) = value else {
            return Err("split accepts text only".to_owned());
        };
        let mut output = Vec::new();
        for part in self.pattern.split(&text) {
            if part.is_empty() {
                continue;
            }
            if self.max_record_size > 0 && part.len() as u64 > self.max_record_size {
                return Err(format!(
                    "split fragment exceeds --max-record-size ({} bytes)",
                    self.max_record_size
                ));
            }
            output.push(Value::Text(part.to_owned()));
        }
        Ok(output)
    }
}

pub enum CompiledStage {
    Jq(JqStage),
    XPath(XPathStage),
    Regex(RegexStage),
    Text,
    Attr(String),
    Slurp,
    Split(SplitStage),
    Captures,
}

impl CompiledStage {
    pub fn compile(
        spec: StageSpec,
        namespaces: &[(String, String)],
        limits: Limits,
    ) -> Result<Self, AppError> {
        match spec {
            StageSpec::Jq(source) => Ok(Self::Jq(JqStage::compile(&source)?)),
            StageSpec::XPath(source) => Ok(Self::XPath(XPathStage::compile(&source, namespaces)?)),
            StageSpec::Regex(source) => Ok(Self::Regex(RegexStage::compile(&source)?)),
            StageSpec::Text => Ok(Self::Text),
            StageSpec::Attr(name) => Ok(Self::Attr(name)),
            StageSpec::Slurp => Ok(Self::Slurp),
            StageSpec::Split(source) => Ok(Self::Split(SplitStage::compile(&source, limits)?)),
            StageSpec::Captures => Ok(Self::Captures),
        }
    }

    pub fn is_boundary(&self) -> bool {
        matches!(self, Self::Slurp)
    }

    /// Types this stage can accept. A subset of the actual runtime check.
    pub fn input_types(&self) -> TypeSet {
        match self {
            Self::Jq(_) => TypeSet::DATA.union(TypeSet::TEXT),
            Self::XPath(_) | Self::Attr(_) => TypeSet::NODE,
            Self::Regex(_) | Self::Split(_) => TypeSet::TEXT,
            Self::Text | Self::Slurp | Self::Captures => TypeSet::ALL,
        }
    }

    /// Upper bound on the types this stage may produce.
    pub fn output_types(&self) -> TypeSet {
        match self {
            Self::Jq(_) | Self::Regex(_) => TypeSet::DATA.union(TypeSet::TEXT),
            Self::XPath(_) => TypeSet::ALL,
            Self::Text | Self::Attr(_) | Self::Split(_) => TypeSet::TEXT,
            Self::Slurp | Self::Captures => TypeSet::DATA,
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Jq(_) => "jq".to_owned(),
            Self::XPath(_) => "xpath".to_owned(),
            Self::Regex(_) => "re".to_owned(),
            Self::Text => "text".to_owned(),
            Self::Attr(name) => format!("attr({name})"),
            Self::Slurp => "slurp".to_owned(),
            Self::Split(_) => "split".to_owned(),
            Self::Captures => "captures".to_owned(),
        }
    }

    /// Like `evaluate`, but `captures()` can read the item's path-spec captures.
    pub fn evaluate_with_origin<'doc>(
        &self,
        value: Value<'doc>,
        origin: Option<&crate::model::Origin>,
    ) -> Result<Vec<Value<'doc>>, String> {
        match self {
            Self::Jq(stage) => stage.evaluate(value),
            Self::XPath(stage) => stage.evaluate(value),
            Self::Regex(stage) => stage.evaluate(value),
            Self::Text => Ok(vec![Value::Text(value.string_value())]),
            Self::Attr(name) => {
                let Value::Node(node) = value else {
                    return Err("attr accepts element or attribute nodes only".to_owned());
                };
                match node {
                    XPathNode::Attribute(attribute) => {
                        Ok(vec![Value::Text(attribute.value().to_owned())])
                    }
                    XPathNode::Element(element) => Ok(element
                        .attribute_value(name.as_str())
                        .map(|value| vec![Value::Text(value.to_owned())])
                        .unwrap_or_default()),
                    _ => Err("attr accepts element or attribute nodes only".to_owned()),
                }
            }
            Self::Slurp => Err("slurp() is a boundary stage and must run in a session".to_owned()),
            Self::Split(stage) => stage.evaluate(value),
            Self::Captures => {
                let captures = origin
                    .and_then(|origin| origin.captures.as_ref())
                    .map(|captures| (**captures).clone())
                    .unwrap_or(serde_json::Value::Null);
                Ok(vec![Value::Data(captures)])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_drops_empty_fragments_without_trimming() {
        // split deliberately preserves surrounding whitespace; users trim via jq.
        let stage = SplitStage::compile(",", Limits::default()).unwrap();
        let values = stage.evaluate(Value::Text(" a ,,b,c ".to_owned())).unwrap();
        assert_eq!(
            values,
            vec![
                Value::Text(" a ".to_owned()),
                Value::Text("b".to_owned()),
                Value::Text("c ".to_owned()),
            ]
        );
    }

    #[test]
    fn split_honours_record_limit() {
        let stage = SplitStage::compile(",", Limits { max_record_size: 2 }).unwrap();
        assert!(stage.evaluate(Value::Text("abc,d".to_owned())).is_err());
    }
}
