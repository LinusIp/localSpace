//! The one place the command-line binary writes to the user (`CLAUDE.md`,
//! "Coding standards": user-facing CLI output is legitimate, but it lives in
//! one module; everything else goes through `tracing`).

use std::io::Write;

/// A line of user-facing output on stdout.
pub fn line(text: impl AsRef<str>) {
    let mut out = std::io::stdout().lock();
    // A closed pipe is the reader's business, not an error worth panicking over.
    let _ = writeln!(out, "{}", text.as_ref());
}

/// A line of user-facing output on stderr: a refusal, a failure, a warning.
pub fn error(text: impl AsRef<str>) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{}", text.as_ref());
}

/// `out!("{} things", n)`: `line`, formatted.
macro_rules! out {
    () => {
        $crate::output::line("")
    };
    ($($arg:tt)*) => {
        $crate::output::line(format!($($arg)*))
    };
}

/// `err!("...")`: `error`, formatted.
macro_rules! err {
    ($($arg:tt)*) => {
        $crate::output::error(format!($($arg)*))
    };
}

pub(crate) use {err, out};
