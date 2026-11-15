use clap::{Parser, ValueEnum};

use crate::error::AppError;

#[derive(Debug, Parser)]
#[command(
    name = "ux",
    bin_name = "ux",
    version,
    about = "Compose jq, XPath and regex extraction stages",
    long_about = "Compose jq, XPath and regex extraction stages into one record stream.\n\nReads files, directories, globs or stdin and emits JSONL by default. Diagnostics go to stderr with stable exit codes; stdout carries data only.",
    after_help = "Examples:\n  ux -e 'jq(.items[].msg)' data.json\n  ux -f jsonl -O text -e 'slurp() | jq(length)' a.jsonl\n  ux --list .\n\nRun `ux --help` for the stage reference and agent contract.",
    after_long_help = "STAGES\n  jq(FILTER)      JSON filter (jaq); JSON strings fall back to text\n  xpath(EXPR)     XPath 1.0 over XML/HTML nodes\n  re(PATTERN)     Rust regex; captures become objects with 0/1/named keys\n  text()          string-value of any item\n  attr(NAME)      attribute value of an element or attribute node\n  split(PATTERN)  split text into fragments\n  slurp()         aggregate ALL inputs into one array (memory proportional to records)\n\nINPUT / OUTPUT\n  Formats  auto|json|jsonl|yaml|toml|xml|html|text|csv|tsv\n  Output   -O jsonl (default) | text | json ; --with-source adds provenance\n  -f auto uses the extension; specify -f for JSONL/YAML/TOML/CSV without one.\n  -O json and slurp() buffer everything before writing the first byte.\n\nAGENT CONTRACT\n  stdout   data only; --list writes a JSONL inventory of the inputs instead\n  stderr   one diagnostic per line; --error-format json emits {code,message,exit_code}\n  --quiet  suppresses warnings, never errors\n  codes    0 success (including zero matches), 1 runtime, 2 usage/compile\n  stdin    no paths reads stdin; an interactive stdin is rejected unless - is given\n  --completions bash|zsh|fish|powershell|elvish prints a completion script\n\nEXAMPLES\n  ux -e 'jq(.items[].msg)' data.json\n  ux -f html -e 'xpath(\"//a\") | attr(\"href\")' page.html\n  ux -f jsonl -O text -e 'slurp() | jq(length)' a.jsonl b.jsonl\n  ux --list .\n  ux --quiet --error-format json -e 'jq(.id)' data.json"
)]
pub struct Cli {
    /// Extraction pipeline, for example: jq(.items[]) | re(r"id=(\d+)")
    /// Omit it only with --list or --completions.
    #[arg(
        short = 'e',
        long = "extract",
        value_name = "PIPELINE",
        required_unless_present_any = ["list", "completions"]
    )]
    pub pipeline: Option<String>,

    /// Input format
    #[arg(short = 'f', long = "format", value_enum, default_value_t = InputFormat::Auto)]
    pub format: InputFormat,

    /// Output mode
    #[arg(short = 'O', long = "output", value_enum, default_value_t = OutputMode::Jsonl)]
    pub output: OutputMode,

    /// Read text as lines or as one raw document
    #[arg(long, value_enum, default_value_t = TextMode::Lines)]
    pub text_mode: TextMode,

    /// XPath prefix binding (PREFIX=URI); the URI is matched against document namespaces
    #[arg(long = "ns", value_name = "PREFIX=URI", value_parser = parse_namespace)]
    pub namespaces: Vec<(String, String)>,

    /// Fuzzy-filter discovered files by normalized relative path
    #[arg(long, value_name = "QUERY")]
    pub fuzzy: Option<String>,

    /// Match files with a structured path spec, e.g.
    /// 'data/{ts:time:%Y-%m-%dT%H-%M-%S}_SRRG/{mins:re:\d+}min/{file:glob:*.txt}'
    #[arg(long = "path-spec", value_name = "SPEC", conflicts_with = "paths")]
    pub path_spec: Option<String>,

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

    /// Emit diagnostics as newline-delimited JSON on stderr (for agents)
    #[arg(long = "error-format", value_enum, value_name = "FORMAT", default_value_t = ErrorFormat::Text)]
    pub error_format: ErrorFormat,

    /// Suppress non-fatal warnings; errors are always reported
    #[arg(long, visible_alias = "silent")]
    pub quiet: bool,

    /// List discovered inputs as JSONL on stdout and exit without extracting
    #[arg(long, visible_alias = "dry-run")]
    pub list: bool,

    /// Reject stdin entirely (implicit or `-`); useful in CI/agent pipelines
    #[arg(long)]
    pub no_stdin: bool,

    /// Print a shell completion script to stdout and exit
    #[arg(long = "completions", value_enum, value_name = "SHELL")]
    pub completions: Option<CompletionShell>,

    /// Set by main.rs when stdout is a terminal; enables per-result flushing.
    #[arg(skip)]
    pub interactive: bool,

    /// Set by main.rs when stdin is a terminal; guards accidental blocking.
    #[arg(skip)]
    pub stdin_is_terminal: bool,

    /// Input files, directories, glob patterns, or - for stdin
    #[arg(value_name = "PATH|GLOB|-")]
    pub paths: Vec<String>,
}

impl Cli {
    /// Reject contradictory or malformed arguments before any input is read.
    pub fn validate(&self) -> Result<(), AppError> {
        // --list and --completions never read input or produce extracted data,
        // so output/aggregation options are irrelevant to them.
        if self.no_stdin
            && self.completions.is_none()
            && self.path_spec.is_none()
            && (self.paths.is_empty() || self.paths.iter().any(|path| path == "-"))
        {
            return Err(AppError::Usage(
                "--no-stdin forbids stdin, but no file paths were given (or `-` was used)"
                    .to_owned(),
            ));
        }
        // P5: compile the path spec before the first input is opened. Both
        // --list and a normal run go through here, so a bad spec is Usage(2)
        // in either mode.
        if let Some(spec) = &self.path_spec {
            crate::pathspec::compile(spec)?;
        }
        if self.list || self.completions.is_some() {
            return Ok(());
        }
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
        // An agent that inherits a terminal would otherwise block forever on a
        // pipeline with no explicit input. `-` remains an explicit opt-in.
        if self.stdin_is_terminal && self.paths.is_empty() && self.path_spec.is_none() {
            return Err(AppError::Usage(
                "no input paths and stdin is a terminal; pass a path/glob, pipe data into stdin, or add `-` to read stdin explicitly"
                    .to_owned(),
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

/// stderr diagnostic encoding. `Text` preserves the historical `ux: ...` lines.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum ErrorFormat {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    #[value(name = "powershell")]
    PowerShell,
    Elvish,
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

    #[test]
    fn list_and_completions_do_not_require_a_pipeline() {
        assert!(parse(&["ux", "--list", "."]).pipeline.is_none());
        assert_eq!(
            parse(&["ux", "--completions", "powershell"]).completions,
            Some(CompletionShell::PowerShell)
        );
        assert!(parse(&["ux", "--list", "."]).validate().is_ok());
    }

    #[test]
    fn error_format_defaults_to_text() {
        assert_eq!(
            parse(&["ux", "-e", "text()"]).error_format,
            ErrorFormat::Text
        );
        assert_eq!(
            parse(&["ux", "-e", "text()", "--error-format", "json"]).error_format,
            ErrorFormat::Json
        );
    }

    #[test]
    fn interactive_stdin_without_paths_is_a_usage_error() {
        let mut cli = parse(&["ux", "-e", "text()"]);
        cli.stdin_is_terminal = true;
        assert_eq!(cli.validate().unwrap_err().exit_code(), 2);
    }

    #[test]
    fn explicit_stdin_marker_allows_interactive_input() {
        let mut cli = parse(&["ux", "-e", "text()", "-"]);
        cli.stdin_is_terminal = true;
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn no_stdin_rejects_implicit_and_explicit_stdin() {
        assert_eq!(
            parse(&["ux", "-e", "text()", "--no-stdin"])
                .validate()
                .unwrap_err()
                .exit_code(),
            2
        );
        assert_eq!(
            parse(&["ux", "-e", "text()", "--no-stdin", "-"])
                .validate()
                .unwrap_err()
                .exit_code(),
            2
        );
        assert!(
            parse(&["ux", "-e", "text()", "--no-stdin", "a.json"])
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn dry_run_is_an_alias_of_list() {
        let cli = parse(&["ux", "--dry-run", "a.json"]);
        assert!(cli.list);
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn list_is_allowed_on_interactive_terminal_without_paths() {
        let mut cli = parse(&["ux", "--list"]);
        cli.stdin_is_terminal = true;
        assert!(cli.validate().is_ok());
    }
}
