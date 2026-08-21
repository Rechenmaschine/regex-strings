use regex_syntax::hir::Hir;

use crate::Alphabet;
use crate::automaton::{Dfa, Nfa};

/// An iterator over the strings a regex matches. See [`crate::RegexExt::strings`].
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
