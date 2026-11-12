use sxd_document::Package;
use sxd_xpath::nodeset::Node;

use crate::cli::InputFormat;
use crate::engine::Pipeline;
use crate::error::AppError;
use crate::input::emit;
use crate::model::{Origin, Value};
use crate::output::OutputSink;

pub fn process(
    source: &str,
    format: InputFormat,
    base: &Origin,
    pipeline: &Pipeline,
    output: &mut OutputSink<'_>,
) -> Result<(), AppError> {
    let package = match format {
        InputFormat::Xml => sxd_document::parser::parse(source).map_err(|errors| {
            AppError::Runtime(format!("{base}: invalid XML: {errors:?}"))
        })?,
        InputFormat::Html => sxd_html::parse_html(source),
        _ => {
            return Err(AppError::Runtime(format!(
                "{base}: internal markup format mismatch"
            )));
        }
    };
    process_package(&package, base, pipeline, output)
}

fn process_package(
    package: &Package,
    base: &Origin,
    pipeline: &Pipeline,
    output: &mut OutputSink<'_>,
) -> Result<(), AppError> {
    let root = package.as_document().root();
    emit(
        Value::Node(Node::from(root)),
        base,
        pipeline,
        output,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::OutputMode;
    use serde_json::Value as JsonValue;

    fn extract(source: &str, format: InputFormat, query: &str) -> Vec<JsonValue> {
        let pipeline = Pipeline::compile(
            query,
            &[("svg".to_owned(), "http://www.w3.org/2000/svg".to_owned())],
        )
        .unwrap();
        let mut bytes = Vec::new();
        let mut output = OutputSink::new(&mut bytes, OutputMode::Jsonl, false);
        process(source, format, &Origin::default(), &pipeline, &mut output).unwrap();
        output.finish().unwrap();
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
        let pipeline = Pipeline::compile("text()", &[]).unwrap();
        let mut bytes = Vec::new();
        let mut output = OutputSink::new(&mut bytes, OutputMode::Jsonl, false);
        let result = process(
            "<root><broken></root>",
            InputFormat::Xml,
            &Origin::default(),
            &pipeline,
            &mut output,
        );
        assert!(result.is_err());
    }
}
