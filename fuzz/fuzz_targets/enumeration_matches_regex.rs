//! Differentially test enumeration against `regex::Regex::is_match`.
#![no_main]

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use regex::RegexBuilder;
use regex_strings::{Alphabet, RegexExt};

const MAX_ALPHABET_LEN: usize = 4;
const MAX_LEN: usize = 7;
const ALPHABET_CHARS: &[char] = &[
    '\0', '\n', '\r', '-', '0', '1', 'a', 'b', 'c', 'é', '中', '🙂',
];

fuzz_target!(|data: &[u8]| {
    let Some(case) = Case::from_bytes(data) else {
        return;
    };
    let Ok(regex) = RegexBuilder::new(case.pattern)
        .size_limit(1 << 16)
        .build()
    else {
        return;
    };

    let found: Vec<String> = regex
        .strings(case.alphabet.clone())
        .max_len(case.max_len)
        .collect();
    let expected: Vec<String> = words(case.alphabet.as_slice(), case.max_len)
        .into_iter()
        .filter(|word| regex.is_match(word))
        .collect();

    assert_eq!(
        found,
        expected,
        "pattern {:?}, alphabet {:?}, max_len {}",
        case.pattern,
        case.alphabet.as_slice(),
        case.max_len,
    );
});

struct Case<'a> {
    pattern: &'a str,
    alphabet: Alphabet,
    max_len: usize,
}

impl<'a> Case<'a> {
    fn from_bytes(data: &'a [u8]) -> Option<Self> {
        let mut input = Unstructured::new(data);
        let alphabet_len = input.int_in_range(0..=MAX_ALPHABET_LEN).ok()?;
        let alphabet_bytes: [u8; MAX_ALPHABET_LEN] = input.arbitrary().ok()?;
        let max_len = input.int_in_range(0..=MAX_LEN).ok()?;
        let pattern = std::str::from_utf8(input.take_rest()).ok()?;
        let alphabet: Alphabet = (0..alphabet_len)
            .map(|index| ALPHABET_CHARS[alphabet_bytes[index] as usize % ALPHABET_CHARS.len()])
            .collect();

        Some(Self {
            pattern,
            alphabet,
            max_len,
        })
    }
}

fn words(chars: &[char], max_len: usize) -> Vec<String> {
    let mut all = vec![String::new()];
    let mut level = vec![String::new()];

    for _ in 0..max_len {
        let mut next_level = Vec::with_capacity(level.len() * chars.len());
        for word in &level {
            for &ch in chars {
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
