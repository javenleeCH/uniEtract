#![no_main]

use libfuzzer_sys::fuzz_target;

// Primary crash surfaces (now routed through the pest PEG in src/dsl.pest):
// unterminated raw strings, unbalanced parentheses, trailing backslashes,
// multibyte characters next to delimiters, an `r` immediately followed by a
// quote outside a string, and deeply nested groups (guarded at 128 levels).
fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let _ = uniextract::parse_pipeline(source);
    }
});
