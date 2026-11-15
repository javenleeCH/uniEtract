use std::borrow::Cow;
use std::io::{BufRead, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use ::regex::Regex;
use encoding_rs::Encoding as TextEncoding;
use serde_json::Value as JsonValue;

use crate::cli::{Cli, Encoding, InputFormat};
use crate::diagnostic::Reporter;
use crate::error::AppError;

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16BE_BOM: &[u8] = &[0xFE, 0xFF];

/// A decoded, BOM-free input together with its resolved format and size limits.
pub struct InputSource<'reader> {
    reader: Box<dyn BufRead + 'reader>,
    pub format: InputFormat,
    pub path: Option<Arc<PathBuf>>,
    pub max_input_size: u64,
    pub max_record_size: u64,
    pub captures: Option<Arc<JsonValue>>,
}

pub enum Opened<'reader> {
    Ready(InputSource<'reader>),
    /// Binary input discovered during directory scanning: warn and skip.
    Skipped(String),
}

impl<'reader> InputSource<'reader> {
    pub fn open(
        mut reader: Box<dyn BufRead + 'reader>,
        path: Option<Arc<PathBuf>>,
        captures: Option<Arc<JsonValue>>,
        cli: &Cli,
        reporter: &mut Reporter<'_>,
    ) -> Result<Opened<'reader>, AppError> {
        let label = path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<stdin>".to_owned());
        // fill_buf never waits for a full buffer, so interactive stdin stays streamable.
        let prefix = reader.fill_buf()?.to_vec();
        let (encoding, bom_length, assumed) = resolve_encoding(&prefix, cli.encoding, &label)?;
        if let Some(name) = assumed {
            reporter.warn(&format!("`{label}`: assumed {name}"));
        }

        if !cli.force_text && looks_binary(&prefix, encoding) {
            let explicit = path.as_ref().is_some_and(|path| {
                cli.paths
                    .iter()
                    .any(|spec| Path::new(spec) == path.as_path())
            });
            if explicit {
                return Err(AppError::Runtime(format!(
                    "`{label}`: binary input rejected; use --force-text to process it as text"
                )));
            }
            return Ok(Opened::Skipped(format!(
                "`{label}`: binary input skipped (use --force-text to override)"
            )));
        }

        let transcoding = !matches!(encoding, ResolvedEncoding::Utf8);
        let (final_reader, sniff): (Box<dyn BufRead + 'reader>, Vec<u8>) = if transcoding {
            let mut bytes = prefix.clone();
            reader.consume(prefix.len());
            // Non-UTF-8 inputs must be decoded in memory. Bound the raw read as
            // well: previously read_to_end ran before any size check, so
            // --max-input-size only applied to the decoded text (OOM risk).
            if cli.max_input_size > 0 {
                let limit = cli.max_input_size;
                if bytes.len() as u64 > limit {
                    return Err(AppError::Runtime(format!(
                        "`{label}`: input exceeds --max-input-size ({limit}) bytes"
                    )));
                }
                let mut chunk = [0_u8; 8192];
                loop {
                    let read = reader.read(&mut chunk).map_err(|error| {
                        AppError::Runtime(format!("`{label}`: cannot read: {error}"))
                    })?;
                    if read == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&chunk[..read]);
                    if bytes.len() as u64 > limit {
                        return Err(AppError::Runtime(format!(
                            "`{label}`: input exceeds --max-input-size ({limit}) bytes"
                        )));
                    }
                }
            } else {
                reader.read_to_end(&mut bytes).map_err(|error| {
                    AppError::Runtime(format!("`{label}`: cannot read: {error}"))
                })?;
            }
            let decoded = decode(&bytes[bom_length.min(bytes.len())..], encoding, &label)?;
            let normalized = normalize_xml_declaration(&decoded);
            let end = normalized.len().min(8192);
            let sniff = normalized.as_bytes()[..end].to_vec();
            (
                Box::new(Cursor::new(normalized.into_owned().into_bytes())),
                sniff,
            )
        } else {
            let skip = bom_length.min(prefix.len());
            reader.consume(skip);
            let sniff = prefix[skip..].to_vec();
            (reader, sniff)
        };

        let format = if cli.format == InputFormat::Auto {
            detect_format(path.as_ref().map(|path| path.as_path()), &sniff)
        } else {
            cli.format
        };

        // Transcoded bytes are already bounded by --max-input-size before
        // decoding. Decoding can expand them (GB18030: 2 bytes -> 3 UTF-8
        // bytes), so applying the same cap again would reject valid files.
        Ok(Opened::Ready(InputSource {
            reader: final_reader,
            format,
            path,
            max_input_size: if transcoding { 0 } else { cli.max_input_size },
            max_record_size: cli.max_record_size,
            captures,
        }))
    }

    pub fn reader(&mut self) -> &mut dyn BufRead {
        &mut *self.reader
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResolvedEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Gbk,
    Gb18030,
}

fn resolve_encoding(
    prefix: &[u8],
    requested: Encoding,
    label: &str,
) -> Result<(ResolvedEncoding, usize, Option<&'static str>), AppError> {
    if requested == Encoding::Auto {
        if prefix.starts_with(UTF8_BOM) {
            return Ok((ResolvedEncoding::Utf8, UTF8_BOM.len(), None));
        }
        if prefix.starts_with(UTF16LE_BOM) {
            return Ok((ResolvedEncoding::Utf16Le, UTF16LE_BOM.len(), None));
        }
        if prefix.starts_with(UTF16BE_BOM) {
            return Ok((ResolvedEncoding::Utf16Be, UTF16BE_BOM.len(), None));
        }
        // `error_len() == None` means an incomplete sequence at the end of the
        // sniff buffer, which is not evidence of a non-UTF-8 file.
        let is_utf8 = match std::str::from_utf8(prefix) {
            Ok(_) => true,
            Err(error) => error.error_len().is_none(),
        };
        if is_utf8 {
            return Ok((ResolvedEncoding::Utf8, 0, None));
        }
        return Ok((ResolvedEncoding::Gb18030, 0, Some("GB18030")));
    }

    let encoding = match requested {
        Encoding::Utf8 => ResolvedEncoding::Utf8,
        Encoding::Utf16Le => ResolvedEncoding::Utf16Le,
        Encoding::Utf16Be => ResolvedEncoding::Utf16Be,
        Encoding::Gbk => ResolvedEncoding::Gbk,
        Encoding::Gb18030 => ResolvedEncoding::Gb18030,
        Encoding::Auto => unreachable!(),
    };
    let bom_length = match encoding {
        ResolvedEncoding::Utf8 if prefix.starts_with(UTF8_BOM) => UTF8_BOM.len(),
        ResolvedEncoding::Utf16Le if prefix.starts_with(UTF16LE_BOM) => UTF16LE_BOM.len(),
        ResolvedEncoding::Utf16Be if prefix.starts_with(UTF16BE_BOM) => UTF16BE_BOM.len(),
        _ => 0,
    };
    if requested == Encoding::Utf8
        && let Err(error) = std::str::from_utf8(&prefix[bom_length..])
        && error.error_len().is_some()
    {
        return Err(AppError::Runtime(format!(
            "`{label}`: input is not valid UTF-8 at byte {}: use --encoding auto or --encoding gbk",
            bom_length + error.valid_up_to()
        )));
    }
    Ok((encoding, bom_length, None))
}

fn decode(bytes: &[u8], encoding: ResolvedEncoding, label: &str) -> Result<String, AppError> {
    let decoder: &'static TextEncoding = match encoding {
        ResolvedEncoding::Utf8 => encoding_rs::UTF_8,
        ResolvedEncoding::Utf16Le => encoding_rs::UTF_16LE,
        ResolvedEncoding::Utf16Be => encoding_rs::UTF_16BE,
        ResolvedEncoding::Gbk => encoding_rs::GBK,
        ResolvedEncoding::Gb18030 => encoding_rs::GB18030,
    };
    let (text, _, had_errors) = decoder.decode(bytes);
    if had_errors {
        return Err(AppError::Runtime(format!(
            "`{label}`: input cannot be decoded with the selected encoding"
        )));
    }
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned())
}

/// UTF-16 documents often keep an `encoding="UTF-16"` declaration even after
/// being decoded to a UTF-8 `String`; leaving it confuses the XML DOM parser.
fn normalize_xml_declaration(text: &str) -> Cow<'_, str> {
    static DECLARATION: OnceLock<Regex> = OnceLock::new();
    let Some(end) = text.find("?>") else {
        return Cow::Borrowed(text);
    };
    let pattern = DECLARATION
        .get_or_init(|| Regex::new(r#"(?i)\s+encoding\s*=\s*(?:"[^"]*"|'[^']*')"#).unwrap());
    let head = &text[..end];
    let cleaned = pattern.replace(head, "");
    if cleaned == head {
        return Cow::Borrowed(text);
    }
    let mut result = String::with_capacity(text.len());
    result.push_str(&cleaned);
    result.push_str(&text[end..]);
    Cow::Owned(result)
}

/// UTF-16/UTF-32 text is full of NUL bytes, so the NUL rule must be waived for it;
/// otherwise the encoding support and the binary guard would cancel each other out.
fn looks_binary(prefix: &[u8], encoding: ResolvedEncoding) -> bool {
    let utf16 = matches!(
        encoding,
        ResolvedEncoding::Utf16Le | ResolvedEncoding::Utf16Be
    );
    if !utf16 && prefix.contains(&0) {
        return true;
    }
    let sample = &prefix[..prefix.len().min(4096)];
    if sample.is_empty() {
        return false;
    }
    let non_printable = sample
        .iter()
        .filter(|byte| {
            if **byte >= 0x80 || (utf16 && **byte == 0) {
                return false;
            }
            !byte.is_ascii_graphic() && !matches!(**byte, b' ' | b'\t' | b'\n' | b'\r')
        })
        .count();
    non_printable * 100 > sample.len() * 30
}

pub(crate) fn detect_format(path: Option<&Path>, prefix: &[u8]) -> InputFormat {
    if let Some(extension) = path
        .and_then(Path::extension)
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
    {
        let known = match extension.as_str() {
            "json" => Some(InputFormat::Json),
            "jsonl" | "ndjson" => Some(InputFormat::Jsonl),
            "yaml" | "yml" => Some(InputFormat::Yaml),
            "toml" => Some(InputFormat::Toml),
            "xml" => Some(InputFormat::Xml),
            "html" | "htm" => Some(InputFormat::Html),
            "txt" | "log" => Some(InputFormat::Text),
            "csv" => Some(InputFormat::Csv),
            "tsv" | "tab" => Some(InputFormat::Tsv),
            _ => None,
        };
        if let Some(known) = known {
            return known;
        }
    }

    let start = prefix
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(prefix.len());
    let prefix = &prefix[start..];
    match prefix.first() {
        Some(b'{') | Some(b'[') => InputFormat::Json,
        Some(b'<') => {
            let beginning =
                String::from_utf8_lossy(&prefix[..prefix.len().min(128)]).to_ascii_lowercase();
            if beginning.contains("<!doctype html") || beginning.contains("<html") {
                InputFormat::Html
            } else {
                InputFormat::Xml
            }
        }
        _ => InputFormat::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ErrorFormat;
    use clap::Parser;

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    fn open(bytes: &[u8], args: &[&str]) -> (InputSource<'static>, Vec<u8>) {
        let cli = cli(args);
        let mut stderr = Vec::new();
        let opened = {
            let mut reporter = Reporter::new(&mut stderr, ErrorFormat::Text, false);
            InputSource::open(
                Box::new(Cursor::new(bytes.to_vec())),
                None,
                None,
                &cli,
                &mut reporter,
            )
            .unwrap()
        };
        match opened {
            Opened::Ready(mut source) => {
                let mut all = Vec::new();
                source.reader().read_to_end(&mut all).unwrap();
                (source, all)
            }
            Opened::Skipped(reason) => panic!("unexpected skip: {reason}"),
        }
    }

    #[test]
    fn strips_utf8_bom() {
        let (_, content) = open(b"\xEF\xBB\xBF{\"a\":1}", &["ux", "-e", "text()"]);
        assert_eq!(content, br#"{"a":1}"#);
    }

    #[test]
    fn utf16le_is_not_classified_as_binary() {
        let mut bytes = vec![0xFF, 0xFE];
        for byte in "hello".encode_utf16().flat_map(|unit| unit.to_le_bytes()) {
            bytes.push(byte);
        }
        let (_, content) = open(&bytes, &["ux", "-e", "text()", "--encoding", "auto"]);
        assert_eq!(String::from_utf8(content).unwrap(), "hello");
    }

    #[test]
    fn utf8_with_nul_is_binary() {
        let cli = cli(&["ux", "-e", "text()"]);
        let mut stderr = Vec::new();
        let opened = {
            let mut reporter = Reporter::new(&mut stderr, ErrorFormat::Text, false);
            InputSource::open(
                Box::new(Cursor::new(b"a\0b".to_vec())),
                None,
                None,
                &cli,
                &mut reporter,
            )
            .unwrap()
        };
        assert!(matches!(opened, Opened::Skipped(_)));
    }

    #[test]
    fn transcoding_respects_max_input_size() {
        let mut bytes = vec![0xFF, 0xFE];
        for byte in "hello world"
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
        {
            bytes.push(byte);
        }
        let cli = cli(&[
            "ux",
            "-e",
            "text()",
            "--encoding",
            "utf-16le",
            "--max-input-size",
            "4",
        ]);
        let mut stderr = Vec::new();
        let error = {
            let mut reporter = Reporter::new(&mut stderr, ErrorFormat::Text, false);
            InputSource::open(
                Box::new(Cursor::new(bytes)),
                None,
                None,
                &cli,
                &mut reporter,
            )
            .err()
            .expect("oversized transcoded input must be rejected")
        };
        assert_eq!(error.exit_code(), 1);
        assert!(error.to_string().contains("--max-input-size"));
    }

    #[test]
    fn quiet_suppresses_encoding_warning() {
        let bytes = [0xD6, 0xD0, 0xCE, 0xC4];
        let cli = cli(&["ux", "-e", "text()", "--encoding", "auto"]);
        let mut stderr = Vec::new();
        {
            let mut reporter = Reporter::new(&mut stderr, ErrorFormat::Text, true);
            let opened = InputSource::open(
                Box::new(Cursor::new(bytes.to_vec())),
                None,
                None,
                &cli,
                &mut reporter,
            )
            .unwrap();
            assert!(matches!(opened, Opened::Ready(_)));
        }
        assert!(stderr.is_empty());
    }

    #[test]
    fn auto_falls_back_to_gb18030() {
        // GBK bytes for "中文".
        let bytes = [0xD6, 0xD0, 0xCE, 0xC4];
        let (_, content) = open(&bytes, &["ux", "-e", "text()", "--encoding", "auto"]);
        assert_eq!(String::from_utf8(content).unwrap(), "中文");
    }

    #[test]
    fn normalizes_utf16_xml_declaration() {
        assert_eq!(
            normalize_xml_declaration(r#"<?xml version="1.0" encoding="UTF-16"?><r/>"#),
            r#"<?xml version="1.0"?><r/>"#
        );
    }

    #[test]
    fn detects_format_from_decoded_prefix() {
        assert_eq!(detect_format(None, br#"  {"a":1}"#), InputFormat::Json);
        assert_eq!(
            detect_format(None, b"<!doctype html><html>"),
            InputFormat::Html
        );
        assert_eq!(detect_format(None, b"plain text"), InputFormat::Text);
    }
}
