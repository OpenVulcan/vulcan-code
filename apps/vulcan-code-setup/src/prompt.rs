//! Asking the person at the terminal, and reporting sizes.
//! 询问终端前的用户，以及报告大小。

use std::io::{BufRead, IsTerminal, Write};

/// Asks `question`; `yes` answers it without asking. Without a terminal it fails rather than
/// guessing.
/// 询问 `question`；`yes` 时不询问直接作答。没有终端时直接失败，不做猜测。
pub fn confirm(question: &str, yes: bool) -> Result<bool, String> {
    if yes {
        return Ok(true);
    }
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        return Err(String::from(
            "there is no terminal to confirm on; pass --yes to run without asking",
        ));
    }
    print!("{question} [Y/n] ");
    std::io::stdout()
        .flush()
        .map_err(|error| format!("cannot write to the terminal: {error}"))?;
    let mut answer = String::new();
    stdin
        .lock()
        .read_line(&mut answer)
        .map_err(|error| format!("cannot read the answer: {error}"))?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "" | "y" | "yes"
    ))
}

/// A byte count as megabytes with one decimal.
/// 以一位小数的 MB 表示的字节数。
pub fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yes_answers_without_a_terminal() {
        assert_eq!(confirm("Continue?", true), Ok(true));
        assert_eq!(megabytes(12_340_000), "12.3 MB");
    }
}
