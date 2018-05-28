//! Minimalistic shell-like functionality:
//!
//! -   Command dispatching
//! -   CLI lexing and expansions (`~`, `$VAR`)

use std::cmp::Ordering;
use std::{io, process};

use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};

/// Communication between dispatch processes using exit codes.
/// Possible exit codes and their meaning:
///
/// - `0`: Dispatch successful.
/// - `> 0`: Error, code == errno.
/// - `< 0`: Unknown/other error.
#[derive(Debug)]
struct ExitProtocol(i32);

impl From<io::Error> for ExitProtocol {
    fn from(error: io::Error) -> ExitProtocol {
        match error.raw_os_error() {
            Some(errno) => ExitProtocol(errno),
            None => ExitProtocol(-1),
        }
    }
}
impl Into<i32> for ExitProtocol {
    fn into(self) -> i32 {
        self.0
    }
}
impl Into<::nix::Result<()>> for ExitProtocol {
    fn into(self) -> ::nix::Result<()> {
        use nix::{Error, errno::{Errno, from_i32}};

        match self.0.cmp(&0) {
            Ordering::Less => Err(Error::Sys(Errno::UnknownErrno)),
            Ordering::Equal => Ok(()),
            Ordering::Greater => Err(Error::Sys(from_i32(self.0))),
        }
    }
}

/// Dispatch in a child process.
///
/// Spawns the passed command and use exit code to indicate status.
fn dispatch_child(mut command: process::Command) -> ! {
    match command.spawn() {
        Ok(_) => process::exit(ExitProtocol(0).into()),
        Err(error) => process::exit(ExitProtocol::from(error).into()),
    }
}

/// Dispatch in the parent process.
///
/// Waits for the child to return its exit code and turn it into result.
fn dispatch_parent(child: Pid) -> ::nix::Result<()> {
    use nix::{Error, errno::Errno};

    const INTERRUPTED: ::nix::Result<()> = Err(Error::Sys(Errno::EINTR));

    loop {
        match waitpid(child, None)? {
            WaitStatus::Exited(_, ec) => break ExitProtocol(ec).into(),
            WaitStatus::Signaled(..) => break INTERRUPTED,
            _ => continue,
        }
    }
}

/// Starts and detaches a command.
///
/// # Examples
///
/// Basic usage:
///
/// ```
/// use urldispatch::sh::dispatch;
/// let mut command = std::process::Command::new("true");
/// dispatch(command).expect("Failed to execute!");
/// ```
pub fn dispatch(command: process::Command) -> ::nix::Result<()> {
    match fork()? {
        ForkResult::Child => dispatch_child(command),
        ForkResult::Parent { child, .. } => dispatch_parent(child),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // Fail execution with non-existing command name
    #[test]
    #[should_panic]
    fn dispatch_reports_failure() {
        dispatch(process::Command::new("asdfghjkl")).unwrap()
    }
}
