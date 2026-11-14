use std::io::{self, IsTerminal};
use std::process::ExitCode;

use clap::Parser;
use uniextract::cli::Cli;

fn main() -> ExitCode {
    let mut cli = Cli::parse();
    // Interactive terminals get per-result flushing automatically; pipelines
    // that need output before EOF must opt in with --line-buffered.
    cli.interactive = io::stdout().is_terminal();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut errors = stderr.lock();

    let code = uniextract::run(&cli, &mut input, &mut output, &mut errors);
    ExitCode::from(code)
}
