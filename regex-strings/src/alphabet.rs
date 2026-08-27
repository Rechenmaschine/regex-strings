/// The finite set of characters a [`crate::RegexExt::strings`] iterator may emit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Alphabet(Vec<char>);

impl Alphabet {
    /// Builds an alphabet, sorting and deduplicating its characters.
    #[must_use]
    pub fn new(characters: impl IntoIterator<Item = char>) -> Self {
        let mut characters: Vec<char> = characters.into_iter().collect();
        characters.sort_unstable();
        characters.dedup();
        Self(characters)
    }

    /// Returns the alphabet's characters in enumeration order.
    #[must_use]
    pub fn as_slice(&self) -> &[char] {
        &self.0
    }

    pub(crate) fn into_vec(self) -> Vec<char> {
        self.0
    }
}

impl From<&str> for Alphabet {
    fn from(value: &str) -> Self {
        Self::new(value.chars())
    }
}

impl From<String> for Alphabet {
    fn from(value: String) -> Self {
        Self::new(value.chars())
    }
}

impl From<&String> for Alphabet {
    fn from(value: &String) -> Self {
        Self::new(value.chars())
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
