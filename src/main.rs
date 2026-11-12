use std::io;
use std::process::ExitCode;

use clap::Parser;
use uniextract::cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut errors = stderr.lock();

    let code = uniextract::run(&cli, &mut input, &mut output, &mut errors);
    ExitCode::from(code)
}
