use clap::{Parser, ValueEnum};

use crate::error::AppError;

#[derive(Debug, Parser)]
#[command(
    name = "ux",
    version,
    about = "Compose jq, XPath and regex extraction stages"
)]
pub struct Cli {
    /// Extraction pipeline, for example: jq(.items[]) | re(r"id=(\d+)")
    #[arg(
        short = 'e',
        long = "extract",
        value_name = "PIPELINE",
        required = true
    )]
    pub pipeline: String,

    /// Input format
    #[arg(short = 'f', long = "format", value_enum, default_value_t = InputFormat::Auto)]
    pub format: InputFormat,

    /// Output mode
    #[arg(short = 'O', long = "output", value_enum, default_value_t = OutputMode::Jsonl)]
    pub output: OutputMode,

    /// Read text as lines or as one raw document
    #[arg(long, value_enum, default_value_t = TextMode::Lines)]
    pub text_mode: TextMode,

    /// XPath expression prefix binding (PREFIX=URI); the URI is NOT matched against document namespaces
    #[arg(long = "ns", value_name = "PREFIX=URI", value_parser = parse_namespace)]
    pub namespaces: Vec<(String, String)>,

    /// Fuzzy-filter discovered files by normalized relative path
    #[arg(long, value_name = "QUERY")]
    pub fuzzy: Option<String>,

    /// Maximum directory recursion depth (0 = direct children only)
    #[arg(long, value_name = "N")]
    pub max_depth: Option<usize>,

    /// Additional exclusion glob, matched against normalized relative paths (repeatable)
    #[arg(long = "exclude", value_name = "GLOB")]
    pub exclude: Vec<String>,

    /// Do not apply the default directory ignore list
    #[arg(long)]
    pub no_default_ignore: bool,

    /// Skip files whose source type cannot feed the first pipeline stage
    #[arg(long)]
    pub skip_incompatible: bool,

    /// Wrap each output value with its source location
    #[arg(long)]
    pub with_source: bool,

    /// Stop at the first file error instead of processing remaining files
    #[arg(long)]
    pub strict: bool,

    /// Source encoding
    #[arg(long, value_enum, default_value_t = Encoding::Utf8)]
    pub encoding: Encoding,

    /// Treat every input as text, bypassing binary detection
    #[arg(long)]
    pub force_text: bool,

    /// Maximum size for whole-document formats (0 = unlimited)
    #[arg(long, value_name = "SIZE", default_value = "256MiB", value_parser = parse_size)]
    pub max_input_size: u64,

    /// Maximum size of a single line, record or split fragment (0 = unlimited)
    #[arg(long, value_name = "SIZE", default_value = "8MiB", value_parser = parse_size)]
    pub max_record_size: u64,

    /// Stop with an error when a further result would be produced (0 = unlimited)
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub max_results: u64,

    /// Flush stdout after every result (for pipelines needing output before EOF)
    #[arg(long)]
    pub line_buffered: bool,

    /// Disable pretty printing for -O json
    #[arg(long)]
    pub compact: bool,

    /// Set by main.rs when stdout is a terminal; enables per-result flushing.
    #[arg(skip)]
    pub interactive: bool,

    /// Input files, directories, glob patterns, or - for stdin
    #[arg(value_name = "PATH|GLOB|-")]
    pub paths: Vec<String>,
}

impl Cli {
    /// Reject contradictory or malformed arguments before any input is read.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.with_source && self.output == OutputMode::Text {
            return Err(AppError::Usage(
                "--with-source and -O text are mutually exclusive; use -O jsonl or -O json to carry source metadata"
                    .to_owned(),
            ));
        }
        if self
            .paths
            .iter()
            .filter(|path| path.as_str() == "-")
            .count()
            > 1
        {
            return Err(AppError::Usage(
                "stdin (`-`) may be specified at most once".to_owned(),
            ));
        }
        for pattern in &self.exclude {
            glob::Pattern::new(pattern).map_err(|error| {
                AppError::Usage(format!("invalid --exclude glob `{pattern}`: {error}"))
            })?;
        }
        if self.fuzzy.as_deref().is_some_and(str::is_empty) {
            return Err(AppError::Usage(
                "--fuzzy query must not be empty".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum InputFormat {
    #[default]
    Auto,
    Json,
    Jsonl,
    Yaml,
    Toml,
    Xml,
    Html,
    Text,
    Csv,
    Tsv,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputMode {
    #[default]
    Jsonl,
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum TextMode {
    #[default]
    Lines,
    Raw,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum Encoding {
    #[default]
    #[value(name = "utf-8")]
    Utf8,
    #[value(name = "utf-16le")]
    Utf16Le,
    #[value(name = "utf-16be")]
    Utf16Be,
    Gbk,
    Gb18030,
    Auto,
}

pub fn parse_size(value: &str) -> Result<u64, String> {
    let (digits, multiplier) = if let Some(rest) = value.strip_suffix("KiB") {
        (rest, 1024)
    } else if let Some(rest) = value.strip_suffix("MiB") {
        (rest, 1024 * 1024)
    } else if let Some(rest) = value.strip_suffix("GiB") {
        (rest, 1024 * 1024 * 1024)
    } else {
        (value, 1)
    };
    let number: u64 = digits
        .parse()
        .map_err(|_| format!("invalid size `{value}`; expected N or N + KiB|MiB|GiB"))?;
    number
        .checked_mul(multiplier)
        .ok_or_else(|| format!("size `{value}` overflows u64"))
}

fn parse_namespace(value: &str) -> Result<(String, String), String> {
    let (prefix, uri) = value
        .split_once('=')
        .ok_or_else(|| "expected PREFIX=URI".to_owned())?;
    if prefix.is_empty() || uri.is_empty() {
        return Err("namespace prefix and URI must not be empty".to_owned());
    }
    if prefix.contains(':') {
        return Err("namespace prefix must not contain ':'".to_owned());
    }
    Ok((prefix.to_owned(), uri.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn rejects_with_source_in_text_mode_before_reading_input() {
        let cli = parse(&["ux", "-e", "text()", "-O", "text", "--with-source"]);
        let error = cli.validate().unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("mutually exclusive"));
    }

    #[test]
    fn rejects_multiple_stdin_markers() {
        let cli = parse(&["ux", "-e", "text()", "-", "-"]);
        assert_eq!(cli.validate().unwrap_err().exit_code(), 2);
    }

    #[test]
    fn rejects_bad_exclude_glob_before_reading_input() {
        let cli = parse(&["ux", "-e", "text()", "--exclude", "["]);
        assert_eq!(cli.validate().unwrap_err().exit_code(), 2);
    }

    #[test]
    fn accepts_fuzzy_now_that_discovery_exists() {
        assert!(
            parse(&["ux", "-e", "text()", "--fuzzy", "prodconfig", "."])
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("0"), Ok(0));
        assert_eq!(parse_size("12"), Ok(12));
        assert_eq!(parse_size("2KiB"), Ok(2048));
        assert_eq!(parse_size("3MiB"), Ok(3 * 1024 * 1024));
        assert!(parse_size("1MB").is_err());
    }

    #[test]
    fn accepts_valid_combinations() {
        assert!(
            parse(&["ux", "-e", "text()", "-O", "text"])
                .validate()
                .is_ok()
        );
        assert!(
            parse(&["ux", "-e", "text()", "--with-source"])
                .validate()
                .is_ok()
        );
    }
}
