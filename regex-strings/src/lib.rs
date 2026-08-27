#![deny(unsafe_code)]
#![warn(missing_docs)]

//! Enumerate the strings matched by a [`Regex`], shortest first.
//!
//! ```
//! use regex::Regex;
//! use regex_strings::RegexExt;
//!
//! let re = Regex::new(r"^a(b|c)*d$").unwrap();
//! let found: Vec<String> = re.strings("abcd").take(4).collect();
//! assert_eq!(found, ["ad", "abd", "acd", "abbd"]);
//! ```
//!
//! Results are ordered by length, then lexicographically within each length.
//! The finite `alphabet` argument defines the characters that may be emitted;
//! unanchored patterns can therefore produce infinitely many strings.
//!
//! Internally, the regex is compiled to an ε-NFA and determinized lazily while
//! a depth-first search enumerates each target length. Use [`Strings::max_len`]
//! to bound expensive searches.

mod alphabet;
mod automaton;
mod strings;

pub use alphabet::Alphabet;
pub use strings::Strings;

use regex::Regex;

/// Extends [`Regex`] with lazy finite-alphabet enumeration.
pub trait RegexExt {
    /// Iterates the strings over `alphabet` that this regex matches, in
    /// nondecreasing length and lexicographic within each length.
    ///
    /// Membership is [`Regex::is_match`]: `s` is yielded if and only if the
    /// regex matches somewhere in `s` and `s` is built from `alphabet`. Anchor
    /// the pattern to enumerate just the language of the pattern itself.
    ///
    /// The iterator is lazy, and infinite whenever the regex is unanchored, so
    /// bound it with [`Iterator::take`].
    ///
    /// ```
    /// use regex::Regex;
    /// use regex_strings::RegexExt;
    ///
    /// let re = Regex::new(r"^\d{3}-\d{2}$").unwrap();
    /// let found: Vec<String> = re.strings("0123-").take(3).collect();
    /// assert_eq!(found, ["000-00", "000-01", "000-02"]);
    /// ```
    fn strings(&self, alphabet: impl Into<Alphabet>) -> Strings;
}

impl RegexExt for Regex {
    fn strings(&self, alphabet: impl Into<Alphabet>) -> Strings {
        let hir =
            regex_syntax::parse(self.as_str()).expect("a compiled Regex has a parseable pattern");
        Strings::new(&hir, alphabet.into())
    }
}

#[cfg(test)]
mod tests {
    use super::Alphabet;
    use super::RegexExt;
    use regex::Regex;

    #[test]
    fn alphabet_is_canonical() {
        let alphabet = Alphabet::from("cbac");
        assert_eq!(alphabet.as_slice(), &['a', 'b', 'c']);

        let alphabet: Alphabet = ['z', 'x', 'z'].into_iter().collect();
        assert_eq!(alphabet.as_slice(), &['x', 'z']);
    }

    fn collect_matches(pattern: &str, alphabet: &str, n: usize) -> Vec<String> {
        Regex::new(pattern)
            .unwrap()
            .strings(alphabet)
            .take(n)
            .collect()
    }

    #[test]
    fn anchored() {
        assert_eq!(collect_matches("^abc$", "abc", 9), ["abc"]);
        assert_eq!(
            collect_matches("^colou?r$", "colour", 9),
            ["color", "colour"]
        );
        assert_eq!(collect_matches("^a{2,4}$", "a", 9), ["aa", "aaa", "aaaa"]);
        assert_eq!(collect_matches("^[ab]*$", "ab", 4), ["", "a", "b", "aa"]);
        assert_eq!(collect_matches("^a.b$", "abc", 9), ["aab", "abb", "acb"]);
        assert_eq!(collect_matches("a+", "bc", 9), Vec::<String>::new());
    }

    #[test]
    fn unanchored() {
        assert_eq!(
            collect_matches("b", "ab", 5),
            ["b", "ab", "ba", "bb", "aab"]
        );
        assert_eq!(
            collect_matches("^a", "ab", 5),
            ["a", "aa", "ab", "aaa", "aab"]
        );
    }

    #[test]
    fn no_duplicates() {
        assert_eq!(
            collect_matches("^(a|aa)*$", "a", 5),
            ["", "a", "aa", "aaa", "aaaa"]
        );
    }

    #[test]
    fn agrees_with_is_match() {
        const MAX_LEN: usize = 6;
        let cases = [
            ("^a(b|c)*d$", "abcd"),
            ("^(a|aa)*$", "ab"),
            ("^[01]*1$", "01"),
            ("^a.*b$", "abc"),
            ("^(ab|ba)+$", "ab"),
            ("^a{2,4}b?$", "ab"),
            ("^[^a]c$", "abc"),
            ("^(a*b*)*c$", "abc"),
            ("^$", "ab"),
            ("(?i)^[ab]+$", "abAB"),
            ("abc", "abc"),
            ("^ab", "ab"),
            ("ab$", "ab"),
            ("a|^b", "ab"),
            ("(^a|b$)+", "ab"),
            (r"\bab\b", "ab-"),
            (r"\Bb", "ab-"),
            (r"^a\b", "ab-"),
            ("(?m)^b$", "ab\n"),
            ("(?m)a$", "ab\n"),
            ("\x1c++*+++++++++*++++++a", "ab-\n"),
            ("a=?aG???.{48}", "ab-\n"),
        ];

        for (pattern, alphabet) in cases {
            let re = Regex::new(pattern).unwrap();
            let expected: Vec<String> = words_up_to(alphabet, MAX_LEN)
                .into_iter()
                .filter(|s| re.is_match(s))
                .collect();
            let found: Vec<String> = re.strings(alphabet).max_len(MAX_LEN).collect();
            assert_eq!(
                found, expected,
                "mismatch for {pattern:?} over {alphabet:?}"
            );
        }
    }

    #[test]
    fn unmatchable_pattern_terminates() {
        for pattern in [r"a\bb", r"a$b", r"a^b", r"\Ba\A", r"a\b\Bb"] {
            assert_eq!(
                collect_matches(pattern, "ab-", 1),
                Vec::<String>::new(),
                "{pattern}"
            );
        }
    }

    fn words_up_to(alphabet: &str, max_len: usize) -> Vec<String> {
        let mut chars: Vec<char> = alphabet.chars().collect();
        chars.sort_unstable();
        chars.dedup();
        let mut all = vec![String::new()];
        let mut level = vec![String::new()];
        for _ in 0..max_len {
            let next_level: Vec<String> = level
                .iter()
                .flat_map(|word| chars.iter().map(move |c| format!("{word}{c}")))
                .collect();
            all.extend(next_level.iter().cloned());
            level = next_level;
        }
        all
    }
}
