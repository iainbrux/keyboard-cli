//! The typed confirmation guarding the two commands that destroy every keyset on the board.
//! One implementation, shared: two copies would drift, and the laxer one would win by accident.

use anyhow::Result;
use std::io::{BufRead, Write};

/// Prints `prompt`, reads one line, and returns true only for the whole word `yes` in any case.
/// A prefix like `y`, an extension like `yess`, anything else, and EOF are all false.
pub(crate) fn confirm(
    out: &mut impl Write,
    prompt: &str,
    input: &mut impl BufRead,
) -> Result<bool> {
    writeln!(out, "{prompt}")?;
    write!(out, "type yes to continue: ")?;
    out.flush()?;
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        return Ok(false);
    }
    Ok(line.trim().eq_ignore_ascii_case("yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact set the operator ruled on: any capitalisation of the whole word passes, and no
    /// prefix or extension of it does. Table-driven so a rewrite that accepted a prefix, or that
    /// compared case-sensitively, fails on the specific input it got wrong rather than on a
    /// single representative case.
    #[test]
    fn confirm_accepts_only_the_whole_word_in_any_case() {
        for (input, want) in [
            ("yes\n", true),
            ("YES\n", true),
            ("Yes\n", true),
            ("yEs\n", true),
            ("  yes  \n", true),
            ("y\n", false),
            ("ye\n", false),
            ("yess\n", false),
            ("yes please\n", false),
            ("no\n", false),
            ("\n", false),
            ("", false),
        ] {
            let mut out = Vec::new();
            let got = confirm(&mut out, "destroy everything?", &mut input.as_bytes()).unwrap();
            assert_eq!(got, want, "input {input:?} should give {want}");
        }
    }

    /// The prompt reaches the operator, and reaches the writer the caller passed rather than
    /// being printed directly, so a caller can capture it.
    #[test]
    fn confirm_writes_the_prompt_it_was_given() {
        let mut out = Vec::new();
        confirm(
            &mut out,
            "keysets 2, 7 will cease to exist",
            &mut "no\n".as_bytes(),
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("keysets 2, 7 will cease to exist"),
            "got: {text}"
        );
    }
}
