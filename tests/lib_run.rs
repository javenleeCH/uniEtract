//! Contract tests for `uniextract::run()` when called directly as a library.
//!
//! `src/main.rs` installs an empty panic hook in `--error-format json` mode and
//! injects TTY state. A direct `run()` caller inherits neither, so these tests
//! install their own hook and pin down the 0.4.0 diagnostic rule, which is
//! keyed on the I/O DIRECTION:
//!
//! * output-side `AppError::Io(BrokenPipe)` -> 0, no diagnostic (normal pipe
//!   termination, e.g. `| head`).
//! * input-side read failure (a closed upstream pipe) -> 1 + `runtime` JSON.
//!
//! Input reads surface `AppError::Runtime` (never a bare `AppError::Io`) so the
//! two directions stay distinguishable by error variant.
use std::io::{self, BufRead, Cursor, Read, Write};
use std::sync::{Mutex, MutexGuard};

use clap::Parser;
use uniextract::cli::Cli;

fn broken_pipe() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe")
}

fn cli(args: &[&str]) -> Cli {
    Cli::try_parse_from(args).unwrap()
}

/// `main.rs` owns the process-wide hook; a library caller must manage its own.
type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

struct QuietHook(Option<PanicHook>);

impl QuietHook {
    fn install() -> Self {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        Self(Some(previous))
    }
}

impl Drop for QuietHook {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            std::panic::set_hook(previous);
        }
    }
}

/// The hook is process-wide: serialize every test that installs one, otherwise
/// a parallel test can restore a hook installed by a different test.
static HOOK_LOCK: Mutex<()> = Mutex::new(());

fn hook_guard() -> MutexGuard<'static, ()> {
    HOOK_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct BrokenOutput;

impl Write for BrokenOutput {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(broken_pipe())
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(broken_pipe())
    }
}

/// Serves one byte so `InputSource::open` can resolve the encoding, then fails
/// on the whole-document read (which wraps errors as `AppError::Runtime`).
struct BrokenInput {
    served: bool,
}

impl Read for BrokenInput {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(broken_pipe())
    }
}

impl BufRead for BrokenInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.served {
            return Err(broken_pipe());
        }
        self.served = true;
        Ok(b"{")
    }

    fn consume(&mut self, _amt: usize) {}
}

/// Fails on the very first `fill_buf`, i.e. `InputSource::open`'s bare `?`.
struct ImmediateBrokenInput;

impl Read for ImmediateBrokenInput {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(broken_pipe())
    }
}

impl BufRead for ImmediateBrokenInput {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Err(broken_pipe())
    }

    fn consume(&mut self, _amt: usize) {}
}

/// Fires exactly once: `BufWriter`'s Drop flushes during unwind, and a second
/// panic there would abort the whole test binary.
struct PanickingOutput {
    fired: bool,
}

impl Write for PanickingOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.fired {
            self.fired = true;
            panic!("synthetic writer panic");
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn diagnostic_from(stderr: &[u8]) -> serde_json::Value {
    serde_json::from_slice(stderr.trim_ascii_end()).unwrap()
}

#[test]
fn run_emits_data_only_and_returns_zero() {
    let _guard = hook_guard();
    let _hook = QuietHook::install();
    let parsed = cli(&["ux", "-f", "jsonl", "-O", "text", "-e", "jq(.id)"]);
    let mut stdin = Cursor::new(b"{\"id\":1}\n{\"id\":2}\n".to_vec());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = uniextract::run(&parsed, &mut stdin, &mut stdout, &mut stderr);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8(stdout).unwrap(), "1\n2\n");
    assert!(stderr.is_empty(), "{:?}", String::from_utf8_lossy(&stderr));
}

#[test]
fn an_output_side_broken_pipe_is_a_normal_termination() {
    let _guard = hook_guard();
    let _hook = QuietHook::install();
    let parsed = cli(&[
        "ux",
        "--error-format",
        "json",
        "-f",
        "jsonl",
        "-O",
        "text",
        "-e",
        "jq(.id)",
    ]);
    let mut stdin = Cursor::new(b"{\"id\":1}\n".to_vec());
    let mut stdout = BrokenOutput;
    let mut stderr = Vec::new();
    let code = uniextract::run(&parsed, &mut stdin, &mut stdout, &mut stderr);
    assert_eq!(code, 0);
    assert!(stderr.is_empty(), "{:?}", String::from_utf8_lossy(&stderr));
}

#[test]
fn a_runtime_wrapped_read_failure_is_reported() {
    let _guard = hook_guard();
    let _hook = QuietHook::install();
    // -f json takes the whole-document read path, which wraps io::Error into
    // AppError::Runtime. is_broken_pipe() only matches AppError::Io, so this is
    // a real error, NOT a normal pipe termination.
    let parsed = cli(&["ux", "--error-format", "json", "-f", "json", "-e", "jq(.)"]);
    let mut stdin = BrokenInput { served: false };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = uniextract::run(&parsed, &mut stdin, &mut stdout, &mut stderr);
    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    let diagnostic = diagnostic_from(&stderr);
    assert_eq!(diagnostic["code"], "runtime");
    assert_eq!(diagnostic["exit_code"], 1);
}

#[test]
fn an_input_side_broken_pipe_is_a_runtime_error() {
    let _guard = hook_guard();
    let _hook = QuietHook::install();
    // 0.4.0 narrows BrokenPipe by direction: an input-side broken pipe (closed
    // upstream) is a real read failure. Both input paths now wrap reads as
    // AppError::Runtime, so they report `runtime(1)` instead of a silent 0.
    let cases: &[(&[&str], &str)] = &[
        (
            &["ux", "--error-format", "json", "-f", "text", "-e", "text()"],
            "line",
        ),
        (
            &["ux", "--error-format", "json", "-f", "json", "-e", "jq(.)"],
            "first",
        ),
    ];
    for (args, which) in cases {
        let parsed = cli(args);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = match *which {
            "line" => {
                let mut stdin = BrokenInput { served: false };
                uniextract::run(&parsed, &mut stdin, &mut stdout, &mut stderr)
            }
            _ => {
                let mut stdin = ImmediateBrokenInput;
                uniextract::run(&parsed, &mut stdin, &mut stdout, &mut stderr)
            }
        };
        assert_eq!(code, 1, "{which}: {:?}", String::from_utf8_lossy(&stderr));
        assert!(stdout.is_empty(), "{which}");
        let diagnostic = diagnostic_from(&stderr);
        assert_eq!(diagnostic["code"], "runtime", "{which}");
        assert_eq!(diagnostic["exit_code"], 1, "{which}");
    }
}

#[test]
fn usage_errors_remain_structured_without_a_process_entry_point() {
    let _guard = hook_guard();
    let _hook = QuietHook::install();
    let parsed = cli(&[
        "ux",
        "--error-format",
        "json",
        "-e",
        "text()",
        "-O",
        "text",
        "--with-source",
    ]);
    let mut stdin = Cursor::new(Vec::new());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = uniextract::run(&parsed, &mut stdin, &mut stdout, &mut stderr);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    let diagnostic = diagnostic_from(&stderr);
    assert_eq!(diagnostic["code"], "usage");
    assert_eq!(diagnostic["exit_code"], 2);
}

#[test]
fn a_panic_is_converged_into_a_runtime_diagnostic() {
    let _guard = hook_guard();
    let _hook = QuietHook::install();
    let parsed = cli(&[
        "ux",
        "--error-format",
        "json",
        "-f",
        "jsonl",
        "-O",
        "text",
        "-e",
        "jq(.id)",
    ]);
    let mut stdin = Cursor::new(b"{\"id\":1}\n".to_vec());
    let mut stdout = PanickingOutput { fired: false };
    let mut stderr = Vec::new();
    let code = uniextract::run(&parsed, &mut stdin, &mut stdout, &mut stderr);
    assert_eq!(code, 1);
    let diagnostic = diagnostic_from(&stderr);
    assert_eq!(diagnostic["code"], "runtime");
    assert_eq!(diagnostic["exit_code"], 1);
    assert!(
        diagnostic["message"]
            .as_str()
            .unwrap()
            .contains("synthetic writer panic")
    );
}
