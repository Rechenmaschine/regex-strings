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

pub use alphabet::Alphabet;

use regex::Regex;
use regex_syntax::hir::Hir;

use crate::automaton::{Dfa, Nfa};

/// Enumeration for [`Regex`].
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

/// An iterator over the strings a regex matches. See [`RegexExt::strings`].
///
/// All the state is the one path the depth-first walk is standing on.
pub struct Strings {
    dfa: Dfa,
    start: Option<u32>,
    /// Length being enumerated right now.
    target: usize,
    max_len: Option<usize>,
    /// Whether a pass has run, and so whether `target` needs advancing.
    started: bool,
    /// Consecutive lengths tried that turned up nothing.
    barren: usize,
    /// The states along `word`, starting with the start state, so one longer
    /// than `word` has characters.
    path: Vec<Step>,
    word: String,
    /// Memo for [`Strings::probe`], indexed by `[steps][state]`.
    outlook: Vec<Vec<Option<Outlook>>>,
}

#[derive(Clone, Copy)]
struct Step {
    state: u32,
    /// Index of the next alphabet character to try from here.
    next: usize,
}

/// What is still ahead, some number of characters on from a state.
///
/// Ordered by how promising it is, so merging what several characters lead to
/// is `max`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Outlook {
    /// No path that long leaves the state at all.
    Nothing,
    /// Paths that long exist, but none of them end in a match.
    Barren,
    /// Some path that long ends in a match.
    Matches,
}

impl Strings {
    pub(crate) fn new(hir: &Hir, alphabet: Alphabet) -> Strings {
        let chars = alphabet.into_vec();
        let mut dfa = Dfa::new(Nfa::build(hir, &chars), chars);
        let start = dfa.intern(vec![dfa.start_state()], None);
        Strings {
            dfa,
            start,
            target: 0,
            max_len: None,
            started: false,
            barren: 0,
            path: Vec::new(),
            word: String::new(),
            outlook: Vec::new(),
        }
    }

    /// Stops after strings of `max_len` characters.
    ///
    /// Also limits how far ahead the lazy search builds the automaton.
    ///
    /// ```
    /// use regex::Regex;
    /// use regex_strings::RegexExt;
    ///
    /// let re = Regex::new("ab").unwrap();
    /// let found: Vec<String> = re.strings("ab").max_len(2).collect();
    /// assert_eq!(found, ["ab"]);
    /// ```
    pub fn max_len(mut self, max_len: usize) -> Strings {
        self.max_len = Some(max_len);
        self
    }

    fn back_out(&mut self) {
        self.path.pop();
        self.word.pop();
    }

    /// Starts the pass for the next length that has any matches, or returns
    /// `false` once no longer string can exist at all.
    fn begin_pass(&mut self) -> bool {
        let Some(start) = self.start else {
            return false;
        };
        if self.started {
            self.target += 1;
        }
        self.started = true;

        loop {
            if self.max_len.is_some_and(|max_len| self.target > max_len) {
                return false;
            }
            match self.probe(start, self.target) {
                Outlook::Nothing => return false,
                Outlook::Matches => {
                    self.barren = 0;
                    self.path.push(Step {
                        state: start,
                        next: 0,
                    });
                    return true;
                }
                Outlook::Barren => {}
            }

            // A sparse language may have empty lengths, but a barren run longer
            // than the number of DFA states cannot contain a later match.
            self.barren += 1;
            if self.barren > self.dfa.state_count() {
                return false;
            }
            self.target += 1;
        }
    }

    /// What is still possible `steps` characters on from `state`.
    ///
    /// Memoized, so each state is worked out once per distance rather than once
    /// per prefix arriving at it.
    fn probe(&mut self, state: u32, steps: usize) -> Outlook {
        if steps == 0 {
            return match self.dfa.accepting(state) {
                true => Outlook::Matches,
                false => Outlook::Barren,
            };
        }
        if let Some(known) = self
            .outlook
            .get(steps)
            .and_then(|row| row.get(state as usize))
            .copied()
            .flatten()
        {
            return known;
        }

        // Recursion can grow the DFA state table, so copy the transitions first.
        let onward = self.dfa.onward(state).to_vec();
        let mut outlook = Outlook::Nothing;
        for (_, target) in onward {
            outlook = outlook.max(self.probe(target, steps - 1));
        }

        let states = self.dfa.state_count();
        if self.outlook.len() <= steps {
            self.outlook.resize_with(steps + 1, Vec::new);
        }
        let row = &mut self.outlook[steps];
        row.resize(states, None);
        row[state as usize] = Some(outlook);
        outlook
    }
}

impl Iterator for Strings {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        loop {
            if self.path.is_empty() && !self.begin_pass() {
                return None;
            }

            let depth = self.path.len() - 1;
            let Step { state, next } = self.path[depth];

            if depth == self.target {
                debug_assert!(
                    self.dfa.accepting(state),
                    "the look-ahead only descends where a match of exactly this length is ahead",
                );
                let word = self.word.clone();
                self.back_out();
                return Some(word);
            }

            let Some((i, target)) = self.dfa.onward(state).get(next).copied() else {
                self.back_out();
                continue;
            };
            self.path[depth].next += 1;

            if self.probe(target, self.target - depth - 1) == Outlook::Matches {
                self.word.push(self.dfa.alphabet_char(i));
                self.path.push(Step {
                    state: target,
                    next: 0,
                });
            }
        }
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

    fn take(pattern: &str, alphabet: &str, n: usize) -> Vec<String> {
        Regex::new(pattern)
            .unwrap()
            .strings(alphabet)
            .take(n)
            .collect()
    }

    #[test]
    fn anchored() {
        assert_eq!(take("^abc$", "abc", 9), ["abc"]);
        assert_eq!(take("^colou?r$", "colour", 9), ["color", "colour"]);
        assert_eq!(take("^a{2,4}$", "a", 9), ["aa", "aaa", "aaaa"]);
        assert_eq!(take("^[ab]*$", "ab", 4), ["", "a", "b", "aa"]);
        assert_eq!(take("^a.b$", "abc", 9), ["aab", "abb", "acb"]);
        assert_eq!(take("a+", "bc", 9), Vec::<String>::new());
    }

    #[test]
    fn unanchored() {
        assert_eq!(take("b", "ab", 5), ["b", "ab", "ba", "bb", "aab"]);
        assert_eq!(take("^a", "ab", 5), ["a", "aa", "ab", "aaa", "aab"]);
    }

    /// `(a|aa)*` derives most of its strings more than one way; enumerating the
    /// automaton cannot emit duplicates.
    #[test]
    fn no_duplicates() {
        assert_eq!(take("^(a|aa)*$", "a", 5), ["", "a", "aa", "aaa", "aaaa"]);
    }

    /// The whole contract: over a small alphabet, what comes out is exactly the
    /// strings the regex itself matches, in the promised order.
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

    /// Matches nothing at all — a boundary cannot sit between two word
    /// characters — yet the automaton always has somewhere left to go, so only
    /// the barren-run bound ends the search.
    #[test]
    fn unmatchable_pattern_terminates() {
        for pattern in [r"a\bb", r"a$b", r"a^b", r"\Ba\A", r"a\b\Bb"] {
            assert_eq!(take(pattern, "ab-", 1), Vec::<String>::new(), "{pattern}");
        }
    }

    /// Every word over the alphabet up to `max_len`, by length then lexicographically.
    fn words_up_to(alphabet: &str, max_len: usize) -> Vec<String> {
        let mut chars: Vec<char> = alphabet.chars().collect();
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
}
