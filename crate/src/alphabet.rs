/// The finite set of characters a [`crate::RegexExt::strings`] iterator may emit.
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
