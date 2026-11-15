use std::io::Write;

use serde_json::{Map, Value as JsonValue};

use crate::cli::ErrorFormat;
use crate::error::AppError;

/// Single funnel for every stderr write. Keeping warnings and errors behind one
/// type makes `--quiet`, `--error-format json` and the text format consistent,
/// including the warnings emitted from discovery and input hygiene.
pub struct Reporter<'a> {
    stderr: &'a mut dyn Write,
    format: ErrorFormat,
    quiet: bool,
}

impl<'a> Reporter<'a> {
    pub fn new(stderr: &'a mut dyn Write, format: ErrorFormat, quiet: bool) -> Self {
        Self {
            stderr,
            format,
            quiet,
        }
    }

    /// Non-fatal diagnostics. `--quiet` suppresses them, never errors.
    pub fn warn(&mut self, message: &str) {
        if self.quiet {
            return;
        }
        match self.format {
            ErrorFormat::Text => {
                let _ = writeln!(self.stderr, "ux: warning: {message}");
            }
            ErrorFormat::Json => {
                let mut object = Map::new();
                object.insert("code".to_owned(), JsonValue::String("warning".to_owned()));
                object.insert("message".to_owned(), JsonValue::String(message.to_owned()));
                object.insert("exit_code".to_owned(), JsonValue::Number(0.into()));
                let _ = writeln!(self.stderr, "{}", JsonValue::Object(object));
            }
        }
    }

    /// Report one error and return the process exit code. stdout is never
    /// touched here, so `-O json` stays parseable even on failure.
    pub fn error(&mut self, error: &AppError) -> u8 {
        if error.is_broken_pipe() {
            return 0;
        }
        match self.format {
            ErrorFormat::Text => {
                let _ = writeln!(self.stderr, "ux: {error}");
            }
            ErrorFormat::Json => {
                let encoded = serde_json::to_string(&error.diagnostic())
                    .unwrap_or_else(|_| "{\"code\":\"io\",\"message\":\"diagnostic serialization failed\",\"exit_code\":1}".to_owned());
                let _ = writeln!(self.stderr, "{encoded}");
            }
        }
        error.exit_code()
    }

    pub fn fatal(&mut self, error: AppError) -> u8 {
        self.error(&error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ErrorFormat;

    fn report(format: ErrorFormat, quiet: bool, error: Option<&AppError>) -> String {
        let mut bytes = Vec::new();
        {
            let mut reporter = Reporter::new(&mut bytes, format, quiet);
            reporter.warn("heads up");
            if let Some(error) = error {
                reporter.error(error);
            }
        }
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn text_format_keeps_the_legacy_shape() {
        let text = report(
            ErrorFormat::Text,
            false,
            Some(&AppError::Usage("bad".to_owned())),
        );
        assert!(text.contains("ux: warning: heads up"));
        assert!(text.contains("ux: bad"));
    }

    #[test]
    fn json_format_emits_one_parseable_object_per_line() {
        let text = report(
            ErrorFormat::Json,
            false,
            Some(&AppError::Usage("bad".to_owned())),
        );
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let warning: JsonValue = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(warning["code"], "warning");
        assert_eq!(warning["exit_code"], 0);
        let error: JsonValue = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(error["code"], "usage");
        assert_eq!(error["exit_code"], 2);
        assert_eq!(error["message"], "bad");
    }

    #[test]
    fn quiet_suppresses_warnings_but_not_errors() {
        let text = report(
            ErrorFormat::Text,
            true,
            Some(&AppError::Usage("bad".to_owned())),
        );
        assert!(!text.contains("heads up"));
        assert!(text.contains("ux: bad"));

        let json = report(
            ErrorFormat::Json,
            true,
            Some(&AppError::Usage("bad".to_owned())),
        );
        assert_eq!(json.lines().count(), 1);
        let error: JsonValue = serde_json::from_str(json.trim()).unwrap();
        assert_eq!(error["code"], "usage");
    }
}
