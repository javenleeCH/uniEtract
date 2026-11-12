pub mod cli;
mod dsl;
mod engine;
mod error;
mod input;
mod markup;
mod model;
mod output;
mod stage;

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;

use cli::Cli;
use engine::Pipeline;
use error::AppError;
use input::process_reader;
use output::OutputSink;

pub fn run(
    cli: &Cli,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    let pipeline = match Pipeline::compile(&cli.pipeline, &cli.namespaces) {
        Ok(pipeline) => pipeline,
        Err(error) => return report_fatal(error, stderr),
    };
    if cli.paths.iter().filter(|path| path.as_str() == "-").count() > 1 {
        return report_fatal(
            AppError::Query("stdin may be specified at most once".to_owned()),
            stderr,
        );
    }

    let paths = if cli.paths.is_empty() {
        vec!["-".to_owned()]
    } else {
        cli.paths.clone()
    };
    let mut output = OutputSink::new(stdout, cli.output, cli.with_source);
    let mut had_errors = false;

    for path in paths {
        let result = if path == "-" {
            process_reader(
                stdin,
                cli.format,
                cli.text_mode,
                None,
                &pipeline,
                &mut output,
            )
        } else {
            process_file(&path, cli, &pipeline, &mut output)
        };
        if let Err(error) = result {
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

    if let Err(error) = output.finish() {
        if error.is_broken_pipe() {
            return 0;
        }
        let _ = writeln!(stderr, "ux: {error}");
        return error.exit_code();
    }
    u8::from(had_errors)
}

fn process_file(
    name: &str,
    cli: &Cli,
    pipeline: &Pipeline,
    output: &mut OutputSink<'_>,
) -> Result<(), AppError> {
    let path = PathBuf::from(name);
    let file = File::open(&path)
        .map_err(|error| AppError::Runtime(format!("{}: cannot open: {error}", path.display())))?;
    let mut reader = BufReader::new(file);
    process_reader(
        &mut reader,
        cli.format,
        cli.text_mode,
        Some(Arc::new(path)),
        pipeline,
        output,
    )
}

fn report_fatal(error: AppError, stderr: &mut dyn Write) -> u8 {
    let _ = writeln!(stderr, "ux: {error}");
    error.exit_code()
}
