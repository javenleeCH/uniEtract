use sxd_document::Package;
use sxd_xpath::nodeset::Node;

use crate::cli::InputFormat;
use crate::error::AppError;
use crate::model::{Item, ItemSink, Origin, Value};

pub fn process(
    source: &str,
    format: InputFormat,
    base: &Origin,
    sink: &mut dyn ItemSink,
) -> Result<(), AppError> {
    let package = match format {
        InputFormat::Xml => sxd_document::parser::parse(source)
            .map_err(|errors| AppError::Runtime(format!("{base}: invalid XML: {errors:?}")))?,
        InputFormat::Html => sxd_html::parse_html(source),
        _ => {
            return Err(AppError::Runtime(format!(
                "{base}: internal markup format mismatch"
            )));
        }
    };
    process_package(&package, base, sink)
}

fn process_package(
    package: &Package,
    base: &Origin,
    sink: &mut dyn ItemSink,
) -> Result<(), AppError> {
    let root = package.as_document().root();
    sink.accept(Item::new(Value::Node(Node::from(root)), base.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::OutputMode;
    use crate::engine::{Pipeline, Relay};
    use crate::output::{OutputOptions, OutputSink};
    use crate::types::Limits;
    use serde_json::Value as JsonValue;

    struct Collector(Vec<String>);

    impl ItemSink for Collector {
        fn accept(&mut self, item: Item<'_>) -> Result<(), AppError> {
            self.0.push(item.value.string_value());
            Ok(())
        }
    }

    fn extract(source: &str, format: InputFormat, query: &str) -> Vec<JsonValue> {
        let pipeline = Pipeline::compile(
            query,
            &[("svg".to_owned(), "http://www.w3.org/2000/svg".to_owned())],
            InputFormat::Auto,
            Limits::default(),
        )
        .unwrap();
        let mut bytes = Vec::new();
        let mut output = OutputSink::new(
            &mut bytes,
            OutputMode::Jsonl,
            false,
            OutputOptions::default(),
        );
        {
            let mut relay = Relay {
                pipeline: &pipeline,
                downstream: &mut output,
            };
            process(source, format, &Origin::default(), &mut relay).unwrap();
        }
        output.finish().unwrap();
        drop(output);
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn evaluates_xpath_on_fault_tolerant_html() {
        let html = r#"<!doctype html><html><body><div><a href="https://example.com/a">first</a><div><a href="/second">second</a>"#;
        let values = extract(html, InputFormat::Html, r#"xpath("//a") | attr("href")"#);
        assert_eq!(values, vec!["https://example.com/a", "/second"]);
    }

    #[test]
    fn reads_attribute_nodes_directly() {
        let html = r#"<html><body><a href="/one">x</a><a href="/two">y</a></body></html>"#;
        let values = extract(
            html,
            InputFormat::Html,
            r#"xpath("//@href") | attr("href")"#,
        );
        assert_eq!(values, vec!["/one", "/two"]);
    }

    #[test]
    fn preserves_foreign_html_namespaces_and_script_text() {
        let html = r#"<!doctype html><svg><circle id="dot"></circle></svg><script>if (a < b) { x(); }</script>"#;
        let ids = extract(
            html,
            InputFormat::Html,
            r#"xpath("//svg:circle") | attr("id")"#,
        );
        assert_eq!(ids, vec!["dot"]);
        let script = extract(html, InputFormat::Html, r#"xpath("//script") | text()"#);
        assert_eq!(script, vec!["if (a < b) { x(); }"]);
    }

    #[test]
    fn rejects_malformed_xml() {
        let mut sink = Collector(Vec::new());
        let result = process(
            "<root><broken></root>",
            InputFormat::Xml,
            &Origin::default(),
            &mut sink,
        );
        assert!(result.is_err());
    }
}
