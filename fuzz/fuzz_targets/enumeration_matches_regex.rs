//! Differentially test enumeration against `regex::Regex::is_match`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use regex::RegexBuilder;
use regex_strings::RegexExt;

const ALPHABETS: &[&str] = &[
    "", "a", "ab", "a0_-", "ab-\n", "\r\n", "éaé", "中🙂a", "\0a",
];
const MAX_LEN: usize = 6;

fuzz_target!(|data: &[u8]| {
    let Some((&alphabet_index, data)) = data.split_first() else {
        return;
    };
    let Some((&max_len, pattern)) = data.split_first() else {
        return;
    };
    let Ok(pattern) = std::str::from_utf8(pattern) else {
        return;
    };
    let alphabet = ALPHABETS[alphabet_index as usize % ALPHABETS.len()];
    let max_len = max_len as usize % (MAX_LEN + 1);
    let Ok(regex) = RegexBuilder::new(pattern)
        .size_limit(1 << 16)
        .build()
    else {
        return;
    };

    let found: Vec<String> = regex
        .strings(alphabet)
        .max_len(max_len)
        .collect();
    let expected: Vec<String> = words(alphabet, max_len)
        .into_iter()
        .filter(|word| regex.is_match(word))
        .collect();

    assert_eq!(
        found,
        expected,
        "pattern {:?}, alphabet {:?}, max_len {}",
        pattern,
        alphabet,
        max_len,
    );
});

fn words(alphabet: &str, max_len: usize) -> Vec<String> {
    let mut chars: Vec<char> = alphabet.chars().collect();
    chars.sort_unstable();
    chars.dedup();

    let mut all = vec![String::new()];
    let mut level = vec![String::new()];

    for _ in 0..max_len {
        let mut next_level = Vec::with_capacity(level.len() * chars.len());
        for word in &level {
            for &ch in &chars {
                let mut next = word.clone();
                next.push(ch);
                next_level.push(next);
            }
        }
        all.extend(next_level.iter().cloned());
        level = next_level;
    }

    all
}
