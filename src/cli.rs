use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "ux", version, about = "Compose jq, XPath and regex extraction stages")]
pub struct Cli {
    /// Extraction pipeline, for example: jq(.items[]) | re(r"id=(\d+)")
    #[arg(short = 'e', long = "extract", value_name = "PIPELINE", required = true)]
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

    /// XPath namespace binding in PREFIX=URI form
    #[arg(long = "ns", value_name = "PREFIX=URI", value_parser = parse_namespace)]
    pub namespaces: Vec<(String, String)>,

    /// Fuzzy-filter discovered files by relative path
    #[arg(long, value_name = "QUERY")]
    pub fuzzy: Option<String>,

    /// Wrap each output value with its source location
    #[arg(long)]
    pub with_source: bool,

    /// Stop at the first file error instead of processing remaining files
    #[arg(long)]
    pub strict: bool,

    /// Input files, directories, glob patterns, or - for stdin
    #[arg(value_name = "PATH|GLOB|-")]
    pub paths: Vec<String>,
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
