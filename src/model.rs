use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Map, Number, Value as JsonValue};
use sxd_xpath::nodeset::Node;

use crate::error::AppError;

/// A consumer of pipeline items. Using a trait (instead of a closure with a single
/// inferred lifetime) lets `Item<'doc>` with document-bound `Node` values cross module
/// boundaries without leaking the borrow (invariant I1).
pub trait ItemSink {
    fn accept(&mut self, item: Item<'_>) -> Result<(), AppError>;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Origin {
    pub path: Option<Arc<PathBuf>>,
    pub line: Option<u64>,
    pub record: u64,
    /// Named captures from `--path-spec`; emitted as `source.captures`.
    /// Pure JSON data (never a document-bound `Node`), so I1 holds.
    pub captures: Option<Arc<JsonValue>>,
}

impl Origin {
    pub fn new(path: Option<Arc<PathBuf>>, record: u64) -> Self {
        Self {
            path,
            line: None,
            record,
            captures: None,
        }
    }

    /// Reuse this origin for another record in the same input.
    pub fn with_record(&self, record: u64) -> Self {
        Self {
            path: self.path.clone(),
            line: None,
            record,
            captures: self.captures.clone(),
        }
    }

    pub fn with_captures(mut self, captures: Option<Arc<JsonValue>>) -> Self {
        self.captures = captures;
        self
    }

    pub fn at_line(&self, line: u64) -> Self {
        Self {
            path: self.path.clone(),
            line: Some(line),
            record: self.record,
            captures: self.captures.clone(),
        }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Some(path) => write!(formatter, "{}", path.display())?,
            None => write!(formatter, "<stdin>")?,
        }
        if let Some(line) = self.line {
            write!(formatter, ":{line}")?;
        }
        write!(formatter, " (record {})", self.record)
    }
}

#[derive(Clone, Debug)]
pub struct Item<'doc> {
    pub value: Value<'doc>,
    pub origin: Origin,
}

impl<'doc> Item<'doc> {
    pub fn new(value: Value<'doc>, origin: Origin) -> Self {
        Self { value, origin }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value<'doc> {
    Data(JsonValue),
    Text(String),
    Node(Node<'doc>),
}

impl Value<'_> {
    pub fn string_value(&self) -> String {
        match self {
            Self::Data(JsonValue::String(value)) => value.clone(),
            Self::Data(value) => value.to_string(),
            Self::Text(value) => value.clone(),
            Self::Node(node) => node.string_value(),
        }
    }

    pub fn into_json(self) -> JsonValue {
        match self {
            Self::Data(value) => value,
            Self::Text(value) => JsonValue::String(value),
            Self::Node(node) => JsonValue::String(node.string_value()),
        }
    }
}

/// Normalize a path to forward slashes for stable, cross-platform output.
/// `--list`, `source.path`, the eval diagnostic `path`, and the stderr warning
/// text must all agree (Windows `\` -> `/`) so an Agent can reconcile data and
/// human-readable diagnostics without a second implementation.
pub fn slash_normalize(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn source_json(origin: &Origin) -> JsonValue {
    let mut source = Map::new();
    source.insert(
        "path".to_owned(),
        origin
            .path
            .as_ref()
            // `--list` (lib.rs) emits forward slashes; `source.path` must join
            // with it. Windows PathBufs would otherwise serialize with `\`,
            // and cross-referencing the two outputs fails wholesale.
            .map(|path| JsonValue::String(slash_normalize(path)))
            .unwrap_or(JsonValue::Null),
    );
    source.insert(
        "line".to_owned(),
        origin
            .line
            .map(|line| JsonValue::Number(Number::from(line)))
            .unwrap_or(JsonValue::Null),
    );
    source.insert(
        "record".to_owned(),
        JsonValue::Number(Number::from(origin.record)),
    );
    if let Some(captures) = &origin.captures {
        source.insert("captures".to_owned(), (**captures).clone());
    }
    source.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_path_uses_forward_slashes_like_list_output() {
        // A backslash-bearing path stays a single PathBuf entry on Unix, so
        // this regression test is meaningful on every platform.
        let origin = Origin::new(
            Some(Arc::new(PathBuf::from(r"D:\OH\group\case\dump.txt"))),
            1,
        );
        let json = source_json(&origin);
        assert_eq!(json["path"].as_str(), Some("D:/OH/group/case/dump.txt"));
    }

    #[test]
    fn slash_normalize_converts_windows_separators() {
        assert_eq!(
            slash_normalize(std::path::Path::new(r"D:\OH\group\dump.txt")),
            "D:/OH/group/dump.txt"
        );
        // Forward slashes and no-slashes are left untouched.
        assert_eq!(slash_normalize(std::path::Path::new("a/b.txt")), "a/b.txt");
    }
}
