use regex::Regex;
use regex_strings::RegexExt;
use std::time::Instant;

fn main() {
    let date = Regex::new(
        r"(?x)
        ^(?:
            (?:19|20)\d{2} - (?:0[13578]|1[02]) - (?:0[1-9]|[12]\d|3[01])  # 31-day months
          | (?:19|20)\d{2} - (?:0[469]|11)      - (?:0[1-9]|[12]\d|30)     # 30-day months
          | (?:19|20)\d{2} - 02                 - (?:0[1-9]|1\d|2[0-8])    # February
          | (?: 19(?:   0[48] | [13579][26] | [2468][048])                 # leap years, minus
              | 20(?:0[048] | [13579][26] | [2468][048]) ) - 02 - 29       # 1900, plus 2000
        )$",
    )
    .unwrap();

    let started = Instant::now();
    let dates: Vec<String> = date.strings("0123456789-").collect();
    println!(
        "{} dates in {:.0?}: {} .. {}",
        dates.len(),
        started.elapsed(),
        dates[0],
        dates.last().unwrap(),
    );

    let email = Regex::new(r"^[a-z]+(\.[a-z]+)?@[a-z]+\.(com|org|de)$").unwrap();
    println!(
        "last email example: {:?}",
        email
            .strings("@abcdefghijklmnopqrstuvwxyz1234567890@.")
            .take(100_000_000)
            .last()
    );
}
