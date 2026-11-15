use serde_json::{Map, Value as JsonValue};
use thiserror::Error;

use crate::model::Origin;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("query error: {0}")]
    Query(String),

    #[error("{0}")]
    Usage(String),

    #[error("{origin}: stage `{stage}`: {message}")]
    Eval {
        origin: Origin,
        stage: String,
        message: String,
    },

    #[error("{0}")]
    Runtime(String),

    /// A hard output limit was reached. Unlike other runtime errors this must
    /// stop the whole run immediately: emitting more input would just repeat
    /// the same diagnostic and could produce a misleading partial aggregate.
    #[error("{0}")]
    Limit(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Stable, machine-readable error classification. The string values are part
/// of the CLI contract: callers may branch on them across releases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    Query,
    Usage,
    Eval,
    Runtime,
    LimitExceeded,
    Io,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Usage => "usage",
            Self::Eval => "eval",
            Self::Runtime => "runtime",
            Self::LimitExceeded => "limit_exceeded",
            Self::Io => "io",
        }
    }
}

impl AppError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Query(_) => ErrorCode::Query,
            Self::Usage(_) => ErrorCode::Usage,
            Self::Eval { .. } => ErrorCode::Eval,
            Self::Runtime(_) => ErrorCode::Runtime,
            Self::Limit(_) => ErrorCode::LimitExceeded,
            Self::Io(_) => ErrorCode::Io,
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Query(_) | Self::Usage(_) => 2,
            _ => 1,
        }
    }

    pub fn is_broken_pipe(&self) -> bool {
        matches!(self, Self::Io(error) if error.kind() == std::io::ErrorKind::BrokenPipe)
    }

    pub fn is_limit(&self) -> bool {
        matches!(self, Self::Limit(_))
    }

    pub fn eval(origin: &Origin, stage: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Eval {
            origin: origin.clone(),
            stage: stage.into(),
            message: message.into(),
        }
    }

    /// One JSON object per diagnostic line. `message` mirrors the human-readable
    /// text format so both channels always describe the same failure.
    pub fn diagnostic(&self) -> JsonValue {
        let mut object = Map::new();
        object.insert(
            "code".to_owned(),
            JsonValue::String(self.code().as_str().to_owned()),
        );
        object.insert("message".to_owned(), JsonValue::String(self.to_string()));
        object.insert(
            "exit_code".to_owned(),
            JsonValue::Number(self.exit_code().into()),
        );
        if let Self::Eval { origin, stage, .. } = self {
            object.insert("stage".to_owned(), JsonValue::String(stage.clone()));
            object.insert(
                "path".to_owned(),
                origin
                    .path
                    .as_ref()
                    .map(|path| JsonValue::String(path.to_string_lossy().into_owned()))
                    .unwrap_or(JsonValue::Null),
            );
            object.insert(
                "line".to_owned(),
                origin
                    .line
                    .map(|line| JsonValue::Number(line.into()))
                    .unwrap_or(JsonValue::Null),
            );
            object.insert("record".to_owned(), JsonValue::Number(origin.record.into()));
        }
        object.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn json(error: &AppError) -> JsonValue {
        error.diagnostic()
    }

    #[test]
    fn codes_and_exit_codes_are_stable() {
        let cases = [
            (AppError::Query("bad".to_owned()), "query", 2),
            (AppError::Usage("bad".to_owned()), "usage", 2),
            (AppError::Runtime("bad".to_owned()), "runtime", 1),
            (
                AppError::Limit("--max-results".to_owned()),
                "limit_exceeded",
                1,
            ),
        ];
        for (error, code, exit_code) in cases {
            assert_eq!(error.code().as_str(), code);
            assert_eq!(error.exit_code(), exit_code);
            assert_eq!(json(&error)["code"], code);
            assert_eq!(json(&error)["exit_code"], exit_code);
            assert!(json(&error)["message"].is_string());
        }
    }

    #[test]
    fn eval_diagnostics_carry_stage_and_origin() {
        let origin = Origin {
            path: Some(Arc::new(PathBuf::from("a.json"))),
            line: Some(7),
            record: 3,
            captures: None,
        };
        let error = AppError::eval(&origin, "jq", "boom");
        let diagnostic = json(&error);
        assert_eq!(diagnostic["stage"], "jq");
        assert_eq!(diagnostic["path"], "a.json");
        assert_eq!(diagnostic["line"], 7);
        assert_eq!(diagnostic["record"], 3);
    }

    #[test]
    fn stdin_origin_is_null_not_a_missing_key() {
        let error = AppError::eval(&Origin::default(), "re", "boom");
        let diagnostic = json(&error);
        assert_eq!(diagnostic["path"], JsonValue::Null);
        assert_eq!(diagnostic["line"], JsonValue::Null);
    }

    #[test]
    fn limit_errors_are_distinguishable() {
        assert!(AppError::Limit("x".to_owned()).is_limit());
        assert!(!AppError::Runtime("x".to_owned()).is_limit());
    }
}
