use js_sys::{Object, Reflect};
use regex::Regex;
use regex_strings::RegexExt;
use wasm_bindgen::prelude::*;

/// Generates newline-separated matches for the browser demo.
///
/// `word_limit` bounds the number of emitted strings. When it is `None`, the
/// iterator runs until the regex language is exhausted, so callers should
/// provide `max_len` for patterns that can match infinitely many strings.
///
/// # Errors
///
/// Returns a JavaScript error when the pattern is invalid, the alphabet is
/// empty, or the result object cannot be populated.
#[wasm_bindgen]
pub fn generate(
    pattern: &str,
    alphabet: &str,
    max_len: Option<usize>,
    word_limit: Option<usize>,
) -> Result<JsValue, JsValue> {
    let result = generate_matches(pattern, alphabet, max_len, word_limit)
        .map_err(|error| JsValue::from_str(&error))?;
    let object = Object::new();
    Reflect::set(
        &object,
        &JsValue::from_str("text"),
        &JsValue::from_str(&result.text),
    )?;
    Reflect::set(
        &object,
        &JsValue::from_str("count"),
        &JsValue::from_f64(result.count as f64),
    )?;
    Ok(object.into())
}

#[derive(Debug)]
struct Generation {
    text: String,
    count: usize,
}

fn generate_matches(
    pattern: &str,
    alphabet: &str,
    max_len: Option<usize>,
    word_limit: Option<usize>,
) -> Result<Generation, String> {
    if alphabet.is_empty() {
        return Err("Alphabet must contain at least one character.".to_owned());
    }

    let regex =
        Regex::new(pattern).map_err(|error| format!("Invalid regular expression: {error}"))?;
    let mut strings = regex.strings(alphabet);
    if let Some(max_len) = max_len {
        strings = strings.max_len(max_len);
    }

    let mut output = String::new();
    let mut count = 0usize;
    for (index, word) in strings.enumerate() {
        if word_limit.is_some_and(|limit| index >= limit) {
            break;
        }
        if count > 0 {
            output.push('\n');
        }
        output.push_str(&word);
        count += 1;
    }
    if count > 0 {
        output.push('\n');
    }

    Ok(Generation {
        text: output,
        count,
    })
}

#[cfg(test)]
mod tests {
    use super::generate_matches;

    #[test]
    fn generates_matches_with_a_trailing_newline() {
        let result = generate_matches("^a(b|c)*d$", "abcd", None, Some(4)).unwrap();
        assert_eq!(result.text, "ad\nabd\nacd\nabbd\n");
        assert_eq!(result.count, 4);
    }

    #[test]
    fn preserves_an_empty_match() {
        let result = generate_matches("^$", "ab", None, None).unwrap();
        assert_eq!(result.text, "\n");
        assert_eq!(result.count, 1);
    }

    #[test]
    fn reports_invalid_input() {
        assert!(
            generate_matches("[", "ab", None, Some(1))
                .unwrap_err()
                .starts_with("Invalid regular expression:")
        );
        assert_eq!(
            generate_matches("a", "", None, Some(1)).unwrap_err(),
            "Alphabet must contain at least one character."
        );
    }

    #[test]
    fn emits_output_larger_than_the_old_browser_cap() {
        let result = generate_matches("^a*[bc]$", "abc", Some(1_000), None).unwrap();
        assert_eq!(result.count, 2_000);
        assert!(result.text.len() > 1_000_000);
    }
}
