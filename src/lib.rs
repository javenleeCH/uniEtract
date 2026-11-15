pub mod cli;
mod diagnostic;
mod discover;
mod dsl;
mod engine;
mod error;
mod input;
mod markup;
mod model;
mod output;
mod pathspec;
mod source;
mod stage;
mod types;

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;

use cli::{Cli, OutputMode};
use diagnostic::Reporter;
use discover::{Input, discover};
use engine::{Pipeline, Relay};
use error::AppError;
use input::process_source;
use model::ItemSink;
use output::{OutputOptions, OutputSink};
use serde_json::{Value as JsonValue, json};
use source::{InputSource, Opened};
use types::Limits;

pub fn run(
    cli: &Cli,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    let mut reporter = Reporter::new(stderr, cli.error_format, cli.quiet);
    // Agents rely on 0/1/2. An unexpected panic must never surface as 101:
    // convert it into a normal structured runtime diagnostic instead.
    match catch_unwind(AssertUnwindSafe(|| {
        run_inner(cli, stdin, stdout, &mut reporter)
    })) {
        Ok(code) => code,
        Err(payload) => {
            let detail = panic_message(payload.as_ref());
            reporter.error(&AppError::Runtime(format!(
                "internal error (panic): {detail}"
            )))
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unexpected panic".to_owned()
    }
}

fn run_inner(
    cli: &Cli,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    reporter: &mut Reporter<'_>,
) -> u8 {
    if let Err(error) = cli.validate() {
        return reporter.fatal(error);
    }

    // Agents need to know the exact input set before deciding whether to run a
    // pipeline. --list performs discovery only and writes one JSON object per
    // input to stdout; no file is opened and no pipeline is compiled.
    if cli.list {
        let inputs = match discover(cli, reporter) {
            Ok(inputs) => inputs,
            Err(error) => return reporter.error(&error),
        };
        for input in inputs {
            let entry = match input {
                Input::Stdin => json!({ "type": "stdin", "path": null }),
                Input::File(path) => json!({
                    "type": "file",
                    "path": path.to_string_lossy().replace('\\', "/")
                }),
                Input::Matched { path, captures } => json!({
                    "type": "file",
                    "path": path.to_string_lossy().replace('\\', "/"),
                    "captures": (*captures).clone()
                }),
            };
            if let Err(error) = writeln!(stdout, "{entry}") {
                return reporter.error(&AppError::Io(error));
            }
        }
        return 0;
    }

    let pipeline_source = match cli.pipeline.as_deref() {
        Some(source) => source,
        None => {
            return reporter.fatal(AppError::Usage(
                "-e/--extract is required unless --list or --completions is used".to_owned(),
            ));
        }
    };
    let pipeline = match Pipeline::compile(
        pipeline_source,
        &cli.namespaces,
        cli.format,
        Limits {
            max_record_size: cli.max_record_size,
        },
    ) {
        Ok(pipeline) => pipeline,
        Err(error) => return reporter.fatal(error),
    };

    if cli.compact && cli.output != OutputMode::Json {
        reporter.warn("--compact only affects -O json; ignoring");
    }
    let mut seen_namespaces = HashSet::new();
    for (prefix, _) in &cli.namespaces {
        if !seen_namespaces.insert(prefix.as_str()) {
            reporter.warn(&format!(
                "duplicate --ns prefix `{prefix}`; the later binding wins"
            ));
        }
    }

    let inputs = match discover(cli, reporter) {
        Ok(inputs) => inputs,
        Err(error) => return reporter.fatal(error),
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
        let captures: Option<Arc<JsonValue>> = match &input {
            Input::Matched { captures, .. } => Some(captures.clone()),
            _ => None,
        };
        let (reader, path): (Box<dyn BufRead + '_>, Option<Arc<PathBuf>>) = match input {
            Input::Stdin => (Box::new(&mut *stdin), None),
            Input::File(path) | Input::Matched { path, .. } => match File::open(&path) {
                Ok(file) => (Box::new(BufReader::new(file)), Some(Arc::new(path))),
                Err(error) => {
                    reporter.error(&AppError::Runtime(format!(
                        "{}: cannot open: {error}",
                        path.display()
                    )));
                    had_errors = true;
                    if cli.strict {
                        break;
                    }
                    continue;
                }
            },
        };

        let mut source = match InputSource::open(reader, path, captures, cli, reporter) {
            Ok(Opened::Ready(source)) => source,
            Ok(Opened::Skipped(warning)) => {
                reporter.warn(&warning);
                continue;
            }
            Err(error) => {
                if error.is_broken_pipe() {
                    return 0;
                }
                reporter.error(&error);
                had_errors = true;
                if cli.strict {
                    break;
                }
                continue;
            }
        };

        if cli.skip_incompatible && !pipeline.accepts_source(source.format) {
            reporter.warn(&format!(
                "skipping {}: source type is incompatible with the first pipeline stage",
                source
                    .path
                    .as_ref()
                    .map_or("<stdin>".to_owned(), |path| path.display().to_string())
            ));
            continue;
        }

        let result = {
            let mut relay = Relay {
                pipeline: &pipeline,
                downstream: &mut output,
            };
            let sink: &mut dyn ItemSink = match session.as_mut() {
                Some(session) => session,
                None => &mut relay,
            };
            process_source(&mut source, cli, sink)
        };
        if let Err(error) = result {
            if error.is_broken_pipe() {
                return 0;
            }
            let limit_reached = error.is_limit();
            let code = reporter.error(&error);
            if limit_reached {
                // Do not emit a partial aggregate, and do not keep walking files:
                // every later record would only repeat the same diagnostic.
                output.abandon();
                let _ = output.flush();
                return code;
            }
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
        return reporter.error(&error);
    }

    // A failed `-O json` run must not publish a partial array. jsonl/text
    // records already written stay visible (partial success is intentional).
    if had_errors {
        output.abandon();
    }
    if let Err(error) = output.finish() {
        if error.is_broken_pipe() {
            return 0;
        }
        return reporter.error(&error);
    }
    u8::from(had_errors)
}

/// Public, panic-free entry point used by the DSL fuzz target.
/// Returns the number of parsed stages; malformed input is a normal `Err`.
pub fn parse_pipeline(source: &str) -> Result<usize, String> {
    dsl::parse(source)
        .map(|stages| stages.len())
        .map_err(|error| error.to_string())
}
