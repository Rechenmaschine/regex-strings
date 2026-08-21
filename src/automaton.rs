use std::collections::HashMap;
use std::collections::hash_map::Entry;

use regex_syntax::hir::{Class, Hir, HirKind, Look};

/// The NFA determinized on demand.
///
/// A state is a set of NFA states *plus the character just consumed*, since
/// that is what a `$` or `\b` one step further on will look back at. Both are
/// needed to decide what happens next, so both make up the identity.
pub(crate) struct Dfa {
    nfa: Nfa,
    alphabet: Vec<char>,
    states: Vec<DfaState>,
    lookup: HashMap<(Vec<u32>, Option<char>), u32>,
}

struct DfaState {
    set: Vec<u32>,
    prev: Option<char>,
    accepting: bool,
    /// Outgoing transitions as `(character, state)`, filled lazily.
    onward: Option<Vec<(char, u32)>>,
}

impl Dfa {
    pub(crate) fn new(nfa: Nfa, alphabet: Vec<char>) -> Dfa {
        Dfa {
            nfa,
            alphabet,
            states: Vec::new(),
            lookup: HashMap::new(),
        }
    }

    /// Interns a state, or `None` if no match is reachable from it.
    pub(crate) fn intern(&mut self, mut set: Vec<u32>, prev: Option<char>) -> Option<u32> {
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

    pub(crate) fn accepting(&self, id: u32) -> bool {
        self.states[id as usize].accepting
    }

    pub(crate) fn state_count(&self) -> usize {
        self.states.len()
    }

    pub(crate) fn start_state(&self) -> u32 {
        self.nfa.start
    }

    pub(crate) fn onward(&mut self, id: u32) -> &[(char, u32)] {
        if self.states[id as usize].onward.is_none() {
            let set = self.states[id as usize].set.clone();
            let prev = self.states[id as usize].prev;

            let mut onward = Vec::new();
            for i in 0..self.alphabet.len() {
                let ch = self.alphabet[i];
                let reached = self.nfa.step(&self.nfa.closure(&set, prev, Some(ch)), ch);
                if let Some(target) = self.intern(reached, Some(ch)) {
                    onward.push((ch, target));
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
pub(crate) struct Nfa {
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
    pub(crate) fn build(hir: &Hir, alphabet: &[char]) -> Nfa {
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

            HirKind::Repetition(rep) => self.compile_repetition(rep, start, accept, alphabet),
        }
        (start, accept)
    }

    fn compile_repetition(
        &mut self,
        rep: &regex_syntax::hir::Repetition,
        start: u32,
        accept: u32,
        alphabet: &[char],
    ) {
        let mut from = start;
        let mut last = None;
        for _ in 0..rep.min {
            let (sub_start, sub_accept) = self.compile(&rep.sub, alphabet);
            self.eps(from, sub_start);
            last = Some(sub_start);
            from = sub_accept;
        }
        match (rep.max, last) {
            // Reuse the last mandatory copy for `x{n,}` to avoid expanding
            // nested quantifiers.
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
