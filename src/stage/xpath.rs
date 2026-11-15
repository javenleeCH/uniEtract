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
        // sxd-xpath 0.4 panics ("No namespace for prefix") while evaluating an
        // expression whose prefix was never bound. A panic would exit 101 and
        // break every agent that relies on the documented exit-code contract,
        // so reject it at compile time as a normal Query(2) error instead.
        for prefix in used_prefixes(source) {
            if !namespaces.iter().any(|(bound, _)| bound == prefix.as_str()) {
                return Err(AppError::Query(format!(
                    "XPath prefix `{prefix}` is not bound; add --ns {prefix}=URI"
                )));
            }
        }
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

/// Collect QName prefixes from an XPath expression. String literals are skipped
/// (they often contain URIs such as `urn:books`) and axis separators `::` are
/// ignored. This is intentionally a conservative lexical scan, not a parser.
fn used_prefixes(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut prefixes = Vec::new();
    let mut quote = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if byte == b':' && bytes.get(index.wrapping_sub(1)).copied() != Some(b':') {
            let next = bytes.get(index + 1).copied();
            let is_qname = next.is_some_and(|byte| byte == b'*' || is_name_start(byte));
            if is_qname && next != Some(b':') {
                let mut start = index;
                while start > 0 && is_name_char(bytes[start - 1]) {
                    start -= 1;
                }
                if start < index && is_name_start(bytes[start]) {
                    let prefix = &source[start..index];
                    if !prefixes.iter().any(|seen| seen == prefix) {
                        prefixes.push(prefix.to_owned());
                    }
                }
            }
        }
        index += 1;
    }
    prefixes
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_prefixes_but_ignores_axes_and_literals() {
        assert!(used_prefixes("//book/title").is_empty());
        assert!(used_prefixes("child::node()").is_empty());
        assert!(used_prefixes(r#"//*[namespace-uri()='urn:books']"#).is_empty());
        assert_eq!(used_prefixes("//b:book | //b:mag"), vec!["b"]);
        assert_eq!(
            used_prefixes(r#"//*[namespace-uri()='urn:books']/x:item"#),
            vec!["x"]
        );
        assert_eq!(used_prefixes("//@b:sku"), vec!["b"]);
        assert_eq!(used_prefixes("//b:*"), vec!["b"]);
    }

    #[test]
    fn unbound_prefix_is_a_query_error_not_a_panic() {
        let error = XPathStage::compile("//x:book", &[])
            .err()
            .expect("an unbound prefix must be rejected at compile time");
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("not bound"));
    }

    #[test]
    fn bound_prefix_compiles() {
        let bindings = vec![("b".to_owned(), "urn:books".to_owned())];
        assert!(XPathStage::compile("//b:book", &bindings).is_ok());
    }
}
