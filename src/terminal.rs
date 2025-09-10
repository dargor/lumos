//! Terminal device operations and low-level terminal control.
//!
//! This module provides functions for direct terminal access, including:
//! - Opening the terminal device (`/dev/tty`)
//! - Setting up raw mode for direct character input
//! - Configuring non-blocking I/O for timeout handling
//! - Proper cleanup and restoration of terminal state

use anyhow::{Context, Result};
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use termios::{ECHO, ICANON, TCSANOW, Termios, tcsetattr};

/// Opens the terminal device for direct access.
///
/// This function opens `/dev/tty` with both read and write permissions,
/// which allows direct communication with the terminal regardless of
/// how stdin/stdout are redirected.
///
/// # Returns
///
/// - `Ok(File)` handle to the terminal device
/// - `Err` if `/dev/tty` cannot be opened
pub fn open_terminal_device() -> Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .context("Failed to open /dev/tty")
}

/// Sets up the terminal in raw mode for direct character input.
///
/// Raw mode disables canonical input processing and echo, allowing
/// the program to read terminal responses directly without interference
/// from the shell or terminal driver.
///
/// # Arguments
///
/// - `file` - Terminal device file handle
///
/// # Returns
///
/// - `Ok(Termios)` containing the original terminal attributes that should be restored
/// - `Err` if terminal attributes cannot be retrieved or set
pub fn setup_raw_mode(file: &File) -> Result<Termios> {
    let fd = file.as_raw_fd();
    let old_termios = Termios::from_fd(fd).context("Failed to get terminal attributes")?;

    let mut new_termios = old_termios;
    new_termios.c_lflag &= !(ICANON | ECHO);
    tcsetattr(fd, TCSANOW, &new_termios).context("Failed to set terminal to raw mode")?;

    Ok(old_termios)
}

/// Sets the terminal file descriptor to non-blocking mode.
///
/// Non-blocking mode prevents read operations from hanging indefinitely
/// if no data is available, allowing the program to implement timeouts
/// and polling for terminal responses.
///
/// # Arguments
///
/// - `file` - Terminal device file handle
///
/// # Returns
///
/// - `Ok(OFlag)` containing the original file descriptor flags that should be restored
/// - `Err` if file descriptor flags cannot be retrieved or set
pub fn setup_non_blocking(file: &File) -> Result<OFlag> {
    let flags = fcntl(file, FcntlArg::F_GETFL).context("Failed to get file descriptor flags")?;
    let original_flags = OFlag::from_bits_truncate(flags);

    let new_flags = original_flags | OFlag::O_NONBLOCK;
    fcntl(file, FcntlArg::F_SETFL(new_flags))
        .context("Failed to set file descriptor to non-blocking")?;

    Ok(original_flags)
}

/// Restores the terminal to its original state.
///
/// This function should be called to clean up terminal settings before
/// the program exits or when an error occurs.
///
/// # Arguments
///
/// - `file` - Terminal device file handle
/// - `termios` - Original terminal attributes to restore
///
/// # Returns
///
/// - `Ok(())` if restoration was successful
/// - `Err` if terminal attributes cannot be restored
pub fn restore_terminal(file: &File, termios: &Termios) -> Result<()> {
    let fd = file.as_raw_fd();
    tcsetattr(fd, TCSANOW, termios).context("Failed to restore terminal attributes")
}

/// Restores file descriptor flags to their original state.
///
/// # Arguments
///
/// - `file` - Terminal device file handle
/// - `flags` - Original file descriptor flags to restore
///
/// # Returns
///
/// - `Ok(())` if restoration was successful
/// - `Err` if file descriptor flags cannot be restored
pub fn restore_flags(file: &File, flags: OFlag) -> Result<()> {
    fcntl(file, FcntlArg::F_SETFL(flags)).context("Failed to restore file descriptor flags")?;
    Ok(())
}
