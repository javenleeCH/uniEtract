#![no_main]

use libfuzzer_sys::fuzz_target;

// `--path-spec` compiler surface: the hand-written segment splitter, tokenizer,
// `re:` fragment validator (brace/paren/character-class/escape scanning) and
// `glob` translation all run under `pathspec::compile`. Malformed specs must
// return `Err` without panicking.
fuzz_target!(|data: &[u8]| {
    if let Ok(spec) = std::str::from_utf8(data) {
        let _ = uniextract::parse_pathspec(spec);
    }
});