use regex_syntax::hir::Hir;

use crate::Alphabet;
use crate::automaton::{Dfa, Nfa};

/// An iterator over the strings a regex matches. See [`crate::RegexExt::strings`].
///
/// It keeps one depth-first path and memoized lookahead.
pub struct Strings {
    dfa: Dfa,
    start: Option<u32>,
    /// Current target length.
    target: usize,
    max_len: Option<usize>,
    /// Whether the current target length has been started.
    started: bool,
    /// Consecutive target lengths without a match.
    barren: usize,
    /// DFA states along `word`, including its start state.
    path: Vec<Step>,
    word: String,
    /// Memoized lookahead indexed by `[steps][state]`.
    outlook: Vec<Vec<Option<Outlook>>>,
}

#[derive(Clone, Copy)]
struct Step {
    state: u32,
    /// Next transition to try from this state.
    next: usize,
}

/// Whether a match is reachable at an exact distance.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Outlook {
    /// No path of that length exists.
    Nothing,
    /// A path exists, but none ends in a match.
    Barren,
    /// At least one path ends in a match.
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

    /// Starts the next target length that can produce a match.
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

            // A barren run longer than the number of DFA states cannot contain
            // a later match.
            self.barren += 1;
            if self.barren > self.dfa.state_count() {
                return false;
            }
            self.target += 1;
        }
    }

    /// Returns whether a match is reachable in exactly `steps` transitions.
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

        // `onward` may grow the DFA, so copy its transitions before recursing.
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

            let Some((ch, target)) = self.dfa.onward(state).get(next).copied() else {
                self.back_out();
                continue;
            };
            self.path[depth].next += 1;

            if self.probe(target, self.target - depth - 1) == Outlook::Matches {
                self.word.push(ch);
                self.path.push(Step {
                    state: target,
                    next: 0,
                });
            }
        }
    }
}
