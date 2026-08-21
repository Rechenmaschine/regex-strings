//! Compare enumeration with `regex::Regex::is_match` over a small alphabet.
#![no_main]

use libfuzzer_sys::fuzz_target;
use regex::RegexBuilder;
use regex_strings::RegexExt;

const ALPHABETS: &[&str] = &["", "a", "ba", "ab-\n", "éaé", "\r\n", "b-a\nb"];
const MAX_LEN: usize = 6;

fuzz_target!(|data: &[u8]| {
    let alphabet = ALPHABETS[data.first().copied().unwrap_or_default() as usize % ALPHABETS.len()];
    let max_len = data.get(1).copied().unwrap_or_default() as usize % (MAX_LEN + 1);

    let Ok(pattern) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(re) = RegexBuilder::new(pattern).size_limit(1 << 16).build() else {
        return;
    };

    let found: Vec<String> = re.strings(alphabet).max_len(max_len).collect();

    let expected: Vec<String> = words(alphabet, max_len)
        .into_iter()
        .filter(|s| re.is_match(s))
        .collect();

    assert_eq!(found, expected, "pattern {pattern:?}");
});

fn words(alphabet: &str, max_len: usize) -> Vec<String> {
    let mut chars: Vec<char> = alphabet.chars().collect();
    chars.sort_unstable();
    chars.dedup();

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
