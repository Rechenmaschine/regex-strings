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

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use regex::Regex;
use regex_syntax::hir::{Class, Hir, HirKind, Look};

/// The finite set of characters a [`RegexExt::strings`] iterator may emit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Alphabet(Vec<char>);

impl Alphabet {
    /// Builds an alphabet, sorting and deduplicating its characters.
    pub fn new(chars: impl IntoIterator<Item = char>) -> Self {
        let mut chars: Vec<char> = chars.into_iter().collect();
        chars.sort_unstable();
        chars.dedup();
        Self(chars)
    }

    /// Returns the alphabet's characters in enumeration order.
    pub fn as_slice(&self) -> &[char] {
        &self.0
    }
}

impl From<&str> for Alphabet {
    fn from(value: &str) -> Self {
        Self::new(value.chars())
    }
}

impl From<String> for Alphabet {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<&String> for Alphabet {
    fn from(value: &String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<Vec<char>> for Alphabet {
    fn from(value: Vec<char>) -> Self {
        Self::new(value)
    }
}

impl FromIterator<char> for Alphabet {
    fn from_iter<T: IntoIterator<Item = char>>(iter: T) -> Self {
        Self::new(iter)
    }
}

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

        let alphabet = alphabet.into();
        let chars = alphabet.0;

        let mut dfa = Dfa::new(Nfa::build(&hir, &chars), chars);
        let start = dfa.intern(vec![dfa.nfa.start], None);
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
                    self.dfa.states[state as usize].accepting,
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
                self.word.push(self.dfa.alphabet[i as usize]);
                self.path.push(Step {
                    state: target,
                    next: 0,
                });
            }
        }
    }
}

impl Strings {
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
            if self.barren > self.dfa.states.len() {
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
            return match self.dfa.states[state as usize].accepting {
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

        let states = self.dfa.states.len();
        if self.outlook.len() <= steps {
            self.outlook.resize_with(steps + 1, Vec::new);
        }
        let row = &mut self.outlook[steps];
        row.resize(states, None);
        row[state as usize] = Some(outlook);
        outlook
    }
}

/// The NFA determinized on demand.
///
/// A state is a set of NFA states *plus the character just consumed*, since
/// that is what a `$` or `\b` one step further on will look back at. Both are
/// needed to decide what happens next, so both make up the identity.
struct Dfa {
    nfa: Nfa,
    alphabet: Vec<char>,
    states: Vec<DfaState>,
    lookup: HashMap<(Vec<u32>, Option<char>), u32>,
}

struct DfaState {
    set: Vec<u32>,
    prev: Option<char>,
    accepting: bool,
    /// Outgoing transitions as `(alphabet index, state)`, filled lazily.
    onward: Option<Vec<(u32, u32)>>,
}

impl Dfa {
    fn new(nfa: Nfa, alphabet: Vec<char>) -> Dfa {
        Dfa {
            nfa,
            alphabet,
            states: Vec::new(),
            lookup: HashMap::new(),
        }
    }

    /// Interns a state, or `None` if no match is reachable from it.
    fn intern(&mut self, mut set: Vec<u32>, prev: Option<char>) -> Option<u32> {
        set.sort_unstable();
        set.dedup();

        let alive = match prev {
            None => &self.nfa.alive_anywhere,
            Some(_) => &self.nfa.alive_later,
        };
        if !set.iter().any(|&q| alive[q as usize]) {
            return None;
        }

        match self.lookup.entry((set, prev)) {
            Entry::Occupied(seen) => Some(*seen.get()),
            Entry::Vacant(slot) => {
                let id = self.states.len() as u32;
                let set = slot.key().0.clone();
                slot.insert(id);
                let accepting = self
                    .nfa
                    .closure(&set, prev, None)
                    .contains(&self.nfa.accept);
                self.states.push(DfaState {
                    set,
                    prev,
                    accepting,
                    onward: None,
                });
                Some(id)
            }
        }
    }

    fn onward(&mut self, id: u32) -> &[(u32, u32)] {
        if self.states[id as usize].onward.is_none() {
            let set = self.states[id as usize].set.clone();
            let prev = self.states[id as usize].prev;

            let mut onward = Vec::new();
            for i in 0..self.alphabet.len() {
                let ch = self.alphabet[i];
                let reached = self.nfa.step(&self.nfa.closure(&set, prev, Some(ch)), ch);
                if let Some(target) = self.intern(reached, Some(ch)) {
                    onward.push((i as u32, target));
                }
            }
            self.states[id as usize].onward = Some(onward);
        }
        self.states[id as usize]
            .onward
            .as_deref()
            .expect("just filled in")
    }
}

#[derive(Default)]
struct NfaState {
    eps: Vec<u32>,
    /// ε-transitions crossable only where the guard holds.
    guarded: Vec<(Look, u32)>,
    trans: Vec<(char, u32)>,
}

/// An ε-NFA with one start and one accepting state (Thompson's invariant).
struct Nfa {
    states: Vec<NfaState>,
    start: u32,
    accept: u32,
    /// Whether the accepting state is reachable from this state, at all.
    alive_anywhere: Vec<bool>,
    /// The same, but past the first character, where a `^` can no longer be
    /// crossed.
    alive_later: Vec<bool>,
}

impl Nfa {
    fn build(hir: &Hir, alphabet: &[char]) -> Nfa {
        // Wrap unanchored searches as `alphabet* pattern alphabet*`.
        let mut nfa = Nfa {
            states: vec![NfaState::default(), NfaState::default()],
            start: 0,
            accept: 1,
            alive_anywhere: Vec::new(),
            alive_later: Vec::new(),
        };
        let (pattern_start, pattern_accept) = nfa.compile(hir, alphabet);
        nfa.eps(nfa.start, pattern_start);
        nfa.eps(pattern_accept, nfa.accept);

        for &ch in alphabet {
            nfa.states[nfa.start as usize].trans.push((ch, nfa.start));
            nfa.states[nfa.accept as usize].trans.push((ch, nfa.accept));
        }

        nfa.alive_anywhere = nfa.co_accessible(|_| true);
        nfa.alive_later = nfa.co_accessible(|look| match look {
            // A single-line `^` cannot be crossed after the first character.
            Look::Start => false,
            Look::StartLF => alphabet.contains(&'\n'),
            Look::StartCRLF => alphabet.contains(&'\n') || alphabet.contains(&'\r'),
            _ => true,
        });
        nfa
    }

    fn push(&mut self) -> u32 {
        self.states.push(NfaState::default());
        (self.states.len() - 1) as u32
    }

    fn eps(&mut self, from: u32, to: u32) {
        self.states[from as usize].eps.push(to);
    }

    /// Compiles `hir` into a fresh `(start, accept)` pair of states.
    ///
    /// A sub-pattern that cannot be spelled from the alphabet matches nothing.
    fn compile(&mut self, hir: &Hir, alphabet: &[char]) -> (u32, u32) {
        let (start, accept) = (self.push(), self.push());
        match hir.kind() {
            HirKind::Empty => self.eps(start, accept),

            HirKind::Look(look) => self.states[start as usize].guarded.push((*look, accept)),

            HirKind::Literal(lit) => {
                let text = String::from_utf8_lossy(&lit.0);
                if text.chars().all(|ch| alphabet.contains(&ch)) {
                    let mut from = start;
                    for ch in text.chars() {
                        let to = self.push();
                        self.states[from as usize].trans.push((ch, to));
                        from = to;
                    }
                    self.eps(from, accept);
                }
            }

            HirKind::Class(class) => {
                for &ch in alphabet.iter().filter(|&&ch| class_contains(class, ch)) {
                    self.states[start as usize].trans.push((ch, accept));
                }
            }

            HirKind::Capture(cap) => {
                let (sub_start, sub_accept) = self.compile(&cap.sub, alphabet);
                self.eps(start, sub_start);
                self.eps(sub_accept, accept);
            }

            HirKind::Concat(parts) => {
                let mut from = start;
                for part in parts {
                    let (sub_start, sub_accept) = self.compile(part, alphabet);
                    self.eps(from, sub_start);
                    from = sub_accept;
                }
                self.eps(from, accept);
            }

            HirKind::Alternation(parts) => {
                for part in parts {
                    let (sub_start, sub_accept) = self.compile(part, alphabet);
                    self.eps(start, sub_start);
                    self.eps(sub_accept, accept);
                }
            }

            HirKind::Repetition(rep) => {
                let mut from = start;
                let mut last = None;
                for _ in 0..rep.min {
                    let (sub_start, sub_accept) = self.compile(&rep.sub, alphabet);
                    self.eps(from, sub_start);
                    last = Some(sub_start);
                    from = sub_accept;
                }
                match (rep.max, last) {
                    // Reuse the last mandatory copy for `x{n,}` to avoid
                    // expanding nested quantifiers.
                    (None, Some(sub_start)) => self.eps(from, sub_start),
                    (None, None) => {
                        let (sub_start, sub_accept) = self.compile(&rep.sub, alphabet);
                        self.eps(from, sub_start);
                        self.eps(sub_accept, sub_start);
                        self.eps(sub_accept, accept);
                    }
                    (Some(max), _) => {
                        for _ in rep.min..max {
                            self.eps(from, accept);
                            let (sub_start, sub_accept) = self.compile(&rep.sub, alphabet);
                            self.eps(from, sub_start);
                            from = sub_accept;
                        }
                    }
                }
                self.eps(from, accept);
            }
        }
        (start, accept)
    }

    /// Backwards reachability, conservatively accounting for guarded edges.
    fn co_accessible(&self, allow: impl Fn(Look) -> bool) -> Vec<bool> {
        let mut back = vec![Vec::new(); self.states.len()];
        for (from, state) in self.states.iter().enumerate() {
            let consuming = state.trans.iter().map(|&(_, to)| to);
            let guarded = state
                .guarded
                .iter()
                .filter(|&&(look, _)| allow(look))
                .map(|&(_, to)| to);
            for to in state.eps.iter().copied().chain(consuming).chain(guarded) {
                back[to as usize].push(from as u32);
            }
        }

        let mut alive = vec![false; self.states.len()];
        alive[self.accept as usize] = true;
        let mut stack = vec![self.accept];
        while let Some(q) = stack.pop() {
            for &from in &back[q as usize] {
                if !std::mem::replace(&mut alive[from as usize], true) {
                    stack.push(from);
                }
            }
        }
        alive
    }

    /// The ε-closure at a position with `prev` behind and `next` ahead.
    fn closure(&self, states: &[u32], prev: Option<char>, next: Option<char>) -> Vec<u32> {
        let mut seen = vec![false; self.states.len()];
        let mut open = Vec::with_capacity(states.len());
        for &q in states {
            if !std::mem::replace(&mut seen[q as usize], true) {
                open.push(q);
            }
        }

        let mut i = 0;
        while i < open.len() {
            let state = &self.states[open[i] as usize];
            let reachable = state.eps.iter().copied().chain(
                state
                    .guarded
                    .iter()
                    .filter(|&&(look, _)| look_holds(look, prev, next))
                    .map(|&(_, to)| to),
            );
            for to in reachable {
                if !std::mem::replace(&mut seen[to as usize], true) {
                    open.push(to);
                }
            }
            i += 1;
        }
        open
    }

    /// The states reached from `states` by consuming `ch`.
    fn step(&self, states: &[u32], ch: char) -> Vec<u32> {
        states
            .iter()
            .flat_map(|&q| &self.states[q as usize].trans)
            .filter(|&&(c, _)| c == ch)
            .map(|&(_, to)| to)
            .collect()
    }
}

/// Whether a look-around holds at a position with `prev` behind it and `next`
/// ahead of it.
fn look_holds(look: Look, prev: Option<char>, next: Option<char>) -> bool {
    let ascii_word = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
    let word = |c: Option<char>| c.is_some_and(regex_syntax::is_word_character);

    match look {
        Look::Start => prev.is_none(),
        Look::End => next.is_none(),
        Look::StartLF => prev.is_none() || prev == Some('\n'),
        Look::EndLF => next.is_none() || next == Some('\n'),
        Look::StartCRLF => {
            prev.is_none() || prev == Some('\n') || (prev == Some('\r') && next != Some('\n'))
        }
        Look::EndCRLF => {
            next.is_none() || next == Some('\r') || (next == Some('\n') && prev != Some('\r'))
        }
        Look::WordAscii => ascii_word(prev) != ascii_word(next),
        Look::WordAsciiNegate => ascii_word(prev) == ascii_word(next),
        Look::WordUnicode => word(prev) != word(next),
        Look::WordUnicodeNegate => word(prev) == word(next),
        Look::WordStartAscii => !ascii_word(prev) && ascii_word(next),
        Look::WordEndAscii => ascii_word(prev) && !ascii_word(next),
        Look::WordStartUnicode => !word(prev) && word(next),
        Look::WordEndUnicode => word(prev) && !word(next),
        Look::WordStartHalfAscii => !ascii_word(prev),
        Look::WordEndHalfAscii => !ascii_word(next),
        Look::WordStartHalfUnicode => !word(prev),
        Look::WordEndHalfUnicode => !word(next),
    }
}

fn class_contains(class: &Class, ch: char) -> bool {
    match class {
        Class::Unicode(class) => class
            .ranges()
            .iter()
            .any(|r| r.start() <= ch && ch <= r.end()),
        Class::Bytes(class) => class
            .ranges()
            .iter()
            .any(|r| char::from(r.start()) <= ch && ch <= char::from(r.end())),
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
