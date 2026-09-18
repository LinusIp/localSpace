//! Starting another program from Core.
//!
//! The desktop app has no console. On Windows a console program started from
//! such a process gets a console window of its own unless it is told not to:
//! a black window for as long as the model runs, and a flash for every
//! question put to the machine. Every program Core starts goes through here.

use std::ffi::OsStr;
use std::process::Command;

/// A `Command` for `program` that opens no window of its own.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// `CREATE_NO_WINDOW`: the child has a console, which is never shown.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
