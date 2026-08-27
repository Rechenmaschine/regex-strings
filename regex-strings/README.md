# regex-strings

Lazily enumerate strings matched by a regular expression over a finite alphabet.

```toml
[dependencies]
regex = "1"
regex-strings = "0.1"
```

```rust
use regex::Regex;
use regex_strings::RegexExt;

let re = Regex::new(r"^a(b|c)*d$").unwrap();
let found: Vec<String> = re.strings("abcd").take(4).collect();

assert_eq!(found, ["ad", "abd", "acd", "abbd"]);
```

Strings are yielded shortest-first, then lexicographically. Use anchors to
enumerate the language described by a pattern; unanchored patterns may produce
an infinite iterator. Use `take` or `max_len` to bound enumeration.

## Demo

The repository includes a [WebAssembly demo](https://github.com/Rechenmaschine/regex-strings).
