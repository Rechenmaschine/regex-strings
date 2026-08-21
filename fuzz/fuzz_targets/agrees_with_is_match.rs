//! Compare enumeration with `regex::Regex::is_match` over a small alphabet.
#![no_main]

use libfuzzer_sys::fuzz_target;
use regex::RegexBuilder;
use regex_strings::RegexExt;

const ALPHABET: &str = "ab-\n";
const MAX_LEN: usize = 4;

fuzz_target!(|data: &[u8]| {
    let Ok(pattern) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(re) = RegexBuilder::new(pattern).size_limit(1 << 16).build() else {
        return;
    };

    let found: Vec<String> = re.strings(ALPHABET).max_len(MAX_LEN).collect();

    let expected: Vec<String> = words(MAX_LEN)
        .into_iter()
        .filter(|s| re.is_match(s))
        .collect();

    assert_eq!(found, expected, "pattern {pattern:?}");
});

fn words(max_len: usize) -> Vec<String> {
    let mut chars: Vec<char> = ALPHABET.chars().collect();
    chars.sort_unstable();

    let mut all = vec![String::new()];
    let mut level = vec![String::new()];
    for _ in 0..max_len {
        level = level
            .iter()
            .flat_map(|word| chars.iter().map(move |c| format!("{word}{c}")))
            .collect();
        all.extend(level.iter().cloned());
    }
    all
}
