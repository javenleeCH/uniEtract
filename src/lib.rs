pub mod cli;
mod discover;
mod dsl;
mod engine;
mod error;
mod input;
mod markup;
mod model;
mod output;
mod source;
mod stage;
mod types;

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;

use cli::{Cli, OutputMode};
use discover::{Input, discover};
use engine::{Pipeline, Relay};
use error::AppError;
use input::process_source;
use model::ItemSink;
use output::{OutputOptions, OutputSink};
use source::{InputSource, Opened};
use types::Limits;

pub fn run(
    cli: &Cli,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    if let Err(error) = cli.validate() {
        return report_fatal(error, stderr);
    }
    let pipeline = match Pipeline::compile(
        &cli.pipeline,
        &cli.namespaces,
        cli.format,
        Limits {
            max_record_size: cli.max_record_size,
        },
    ) {
        Ok(pipeline) => pipeline,
        Err(error) => return report_fatal(error, stderr),
    };

    if cli.compact && cli.output != OutputMode::Json {
        let _ = writeln!(
            stderr,
            "ux: warning: --compact only affects -O json; ignoring"
        );
    }
    let mut seen_namespaces = HashSet::new();
    for (prefix, _) in &cli.namespaces {
        if !seen_namespaces.insert(prefix.as_str()) {
            let _ = writeln!(
                stderr,
                "ux: warning: duplicate --ns prefix `{prefix}`; the later binding wins"
            );
        }
    }

    let inputs = match discover(cli, stderr) {
        Ok(inputs) => inputs,
        Err(error) => return report_fatal(error, stderr),
    };

    let mut output = OutputSink::new(
        stdout,
        cli.output,
        cli.with_source,
        OutputOptions {
            compact: cli.compact,
            line_buffered: cli.line_buffered || cli.interactive,
            max_results: cli.max_results,
        },
    );
    let mut session = pipeline.session();
    let mut had_errors = false;

    for input in inputs {
        let (reader, path): (Box<dyn BufRead + '_>, Option<Arc<PathBuf>>) = match input {
            Input::Stdin => (Box::new(&mut *stdin), None),
            Input::File(path) => match File::open(&path) {
                Ok(file) => (Box::new(BufReader::new(file)), Some(Arc::new(path))),
                Err(error) => {
                    let _ = writeln!(stderr, "ux: {}: cannot open: {error}", path.display());
                    had_errors = true;
                    if cli.strict {
                        break;
                    }
                    continue;
                }
            },
        };

        let mut source = match InputSource::open(reader, path, cli, stderr) {
            Ok(Opened::Ready(source)) => source,
            Ok(Opened::Skipped(warning)) => {
                let _ = writeln!(stderr, "{warning}");
                continue;
            }
            Err(error) => {
                if error.is_broken_pipe() {
                    return 0;
                }
                let _ = writeln!(stderr, "ux: {error}");
                had_errors = true;
                if cli.strict {
                    break;
                }
                continue;
            }
        };

        if cli.skip_incompatible && !pipeline.accepts_source(source.format) {
            let _ = writeln!(
                stderr,
                "ux: warning: skipping {}: source type is incompatible with the first pipeline stage",
                source
                    .path
                    .as_ref()
                    .map_or("<stdin>".to_owned(), |path| path.display().to_string())
            );
            continue;
        }

        let mut relay = Relay {
            pipeline: &pipeline,
            downstream: &mut output,
        };
        let sink: &mut dyn ItemSink = match session.as_mut() {
            Some(session) => session,
            None => &mut relay,
        };
        if let Err(error) = process_source(&mut source, cli, sink) {
            if error.is_broken_pipe() {
                return 0;
            }
            let _ = writeln!(stderr, "ux: {error}");
            had_errors = true;
            if cli.strict {
                break;
            }
        }
    }

    // A slurp session deliberately emits nothing until every input succeeded:
    // a partial aggregate would look like a complete statistic (P2/P3).
    if session.is_some() && had_errors {
        return 1;
    }
    if let Some(session) = session
        && let Err(error) = session.finish(&mut output)
    {
        if error.is_broken_pipe() {
            return 0;
        }
        let _ = writeln!(stderr, "ux: {error}");
        return error.exit_code();
    }

    if let Err(error) = output.finish() {
        if error.is_broken_pipe() {
            return 0;
        }
        let _ = writeln!(stderr, "ux: {error}");
        return error.exit_code();
    }
    u8::from(had_errors)
}

fn report_fatal(error: AppError, stderr: &mut dyn Write) -> u8 {
    let _ = writeln!(stderr, "ux: {error}");
    error.exit_code()
}

/// Public, panic-free entry point used by the DSL fuzz target.
/// Returns the number of parsed stages; malformed input is a normal `Err`.
pub fn parse_pipeline(source: &str) -> Result<usize, String> {
    dsl::parse(source)
        .map(|stages| stages.len())
        .map_err(|error| error.to_string())
}
