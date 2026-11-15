use std::io::{self, IsTerminal};
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};
use uniextract::cli::{Cli, CompletionShell, ErrorFormat};

fn main() -> ExitCode {
    let mut cli = Cli::parse();

    if let Some(requested) = cli.completions {
        let shell = match requested {
            CompletionShell::Bash => Shell::Bash,
            CompletionShell::Zsh => Shell::Zsh,
            CompletionShell::Fish => Shell::Fish,
            CompletionShell::PowerShell => Shell::PowerShell,
            CompletionShell::Elvish => Shell::Elvish,
        };
        let mut command = Cli::command();
        let name = command.get_name().to_owned();
        generate(shell, &mut command, name, &mut io::stdout());
        return ExitCode::SUCCESS;
    }

    if cli.error_format == ErrorFormat::Json {
        // Keep stderr as pure JSONL even if an unexpected panic occurs;
        // lib::run converts panics into a structured runtime diagnostic.
        std::panic::set_hook(Box::new(|_| {}));
    }

    // Interactive terminals get per-result flushing automatically; pipelines
    // that need output before EOF must opt in with --line-buffered.
    cli.interactive = io::stdout().is_terminal();
    // An agent inheriting a terminal must not hang on the implicit stdin read.
    cli.stdin_is_terminal = io::stdin().is_terminal();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut errors = stderr.lock();

    let code = uniextract::run(&cli, &mut input, &mut output, &mut errors);
    ExitCode::from(code)
}
