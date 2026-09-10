//! Split one command string into argv without a shell.
//!
//! A workflow writes the failing command the way it appears in CI, quotes and
//! all. Handing that string to a shell would make the argv depend on the
//! runner: word splitting, globbing, and variable expansion all differ between
//! hosts, and none of them belong in a command that is only ever re-run
//! verbatim. This splitter recognizes quoting and escaping, and nothing else.

/// Why a command string does not describe an argv.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandLineError {
    /// A quote opened and the string ended before it closed.
    UnterminatedQuote,
    /// A backslash escaped the end of the string.
    DanglingEscape,
    /// The string holds no argument at all.
    Empty,
}

impl CommandLineError {
    /// Message for the operator who wrote the command.
    pub const fn message(self) -> &'static str {
        match self {
            Self::UnterminatedQuote => {
                "the command has an unterminated quote; close it or escape the quote character"
            }
            Self::DanglingEscape => {
                "the command ends with a backslash that escapes nothing; double it to pass one"
            }
            Self::Empty => "the command is empty",
        }
    }
}

/// Split `input` into argv.
///
/// Whitespace separates arguments. Single quotes take everything literally;
/// double quotes take everything literally except a backslash before `"` or
/// another backslash. Outside quotes a backslash escapes the next character.
/// Nothing is expanded: `$HOME`, `*`, and `` `cmd` `` reach the program as the
/// bytes that were written.
pub fn split(input: &str) -> Result<Vec<String>, CommandLineError> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut characters = input.chars();

    while let Some(character) = characters.next() {
        match character {
            c if c.is_whitespace() => {
                if started {
                    arguments.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                loop {
                    match characters.next() {
                        Some('\'') => break,
                        Some(inner) => current.push(inner),
                        None => return Err(CommandLineError::UnterminatedQuote),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match characters.next() {
                        Some('"') => break,
                        // Only the two characters that would otherwise end or
                        // escape the quote are special; every other backslash
                        // is a literal one, as a shell would also pass it.
                        Some('\\') => match characters.next() {
                            Some(escaped @ ('"' | '\\')) => current.push(escaped),
                            Some(other) => {
                                current.push('\\');
                                current.push(other);
                            }
                            None => return Err(CommandLineError::UnterminatedQuote),
                        },
                        Some(inner) => current.push(inner),
                        None => return Err(CommandLineError::UnterminatedQuote),
                    }
                }
            }
            '\\' => {
                started = true;
                match characters.next() {
                    Some(escaped) => current.push(escaped),
                    None => return Err(CommandLineError::DanglingEscape),
                }
            }
            other => {
                started = true;
                current.push(other);
            }
        }
    }

    if started {
        arguments.push(current);
    }

    if arguments.is_empty() {
        return Err(CommandLineError::Empty);
    }

    Ok(arguments)
}

#[cfg(test)]
mod tests {
    use super::{split, CommandLineError};

    #[test]
    fn splits_a_plain_command_on_whitespace() {
        assert_eq!(split("python bug.py").unwrap(), ["python", "bug.py"]);
    }

    #[test]
    fn keeps_a_quoted_argument_whole_and_drops_its_quotes() {
        // The defect this splitter exists for: the shell expansion it replaced
        // passed the quote characters through as argument bytes.
        assert_eq!(
            split(r#"python -c "print(123)""#).unwrap(),
            ["python", "-c", "print(123)"]
        );
        assert_eq!(
            split(r#"pytest -k "foo or bar""#).unwrap(),
            ["pytest", "-k", "foo or bar"]
        );
    }

    #[test]
    fn treats_a_path_with_spaces_as_one_argument() {
        assert_eq!(
            split(r#""C:\Program Files\python.exe" bug.py"#).unwrap(),
            [r"C:\Program Files\python.exe", "bug.py"]
        );
    }

    #[test]
    fn never_expands_a_glob_or_a_variable() {
        assert_eq!(split("ls *.py $HOME").unwrap(), ["ls", "*.py", "$HOME"]);
    }

    #[test]
    fn takes_a_single_quoted_run_literally() {
        assert_eq!(
            split(r#"sh -c 'echo "$HOME" \n'"#).unwrap(),
            ["sh", "-c", r#"echo "$HOME" \n"#]
        );
    }

    #[test]
    fn unescapes_only_a_quote_or_a_backslash_inside_double_quotes() {
        assert_eq!(split(r#""a\"b""#).unwrap(), [r#"a"b"#]);
        assert_eq!(split(r#""a\\b""#).unwrap(), [r"a\b"]);
        assert_eq!(split(r#""a\nb""#).unwrap(), [r"a\nb"]);
    }

    #[test]
    fn keeps_an_empty_quoted_argument() {
        assert_eq!(split(r#"cmd "" x"#).unwrap(), ["cmd", "", "x"]);
    }

    #[test]
    fn collapses_runs_of_whitespace_between_arguments() {
        assert_eq!(split("  a \t b \n c  ").unwrap(), ["a", "b", "c"]);
    }

    #[test]
    fn rejects_a_command_that_cannot_be_read_as_argv() {
        assert_eq!(
            split(r#"python -c "print("#),
            Err(CommandLineError::UnterminatedQuote)
        );
        assert_eq!(
            split("python 'bug.py"),
            Err(CommandLineError::UnterminatedQuote)
        );
        assert_eq!(
            split(r"python bug.py \"),
            Err(CommandLineError::DanglingEscape)
        );
        assert_eq!(split("   "), Err(CommandLineError::Empty));
    }
}
