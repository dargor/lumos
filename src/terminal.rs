//! Terminal device operations and low-level terminal control.
//!
//! This module provides functions for direct terminal access, including:
//! - Opening the terminal device (`/dev/tty`)
//! - Setting up raw mode for direct character input
//! - Automatic cleanup and restoration of terminal state via RAII guard

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use termios::{ECHO, ICANON, TCSANOW, Termios, tcsetattr};

use crate::debug;

/// RAII guard for terminal raw mode that automatically restores terminal state on drop.
///
/// This guard ensures the terminal is always restored to its original state,
/// even if the program panics or encounters an error. It holds the terminal
/// device file handle and the original terminal attributes.
pub(crate) struct TerminalGuard {
    /// Terminal device file handle.
    file: File,
    /// Original terminal attributes to restore on drop.
    original_termios: Termios,
}

impl TerminalGuard {
    /// Creates a new terminal guard, opening the terminal device and setting raw mode.
    ///
    /// This function:
    /// 1. Opens `/dev/tty` with read and write permissions
    /// 2. Saves the current terminal attributes
    /// 3. Sets the terminal to raw mode (disables canonical input and echo)
    ///
    /// The terminal will be automatically restored when the guard is dropped.
    ///
    /// # Returns
    ///
    /// - `Ok(TerminalGuard)` ready for direct terminal communication
    /// - `Err` if the terminal cannot be opened or configured
    pub(crate) fn new() -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .context("Failed to open /dev/tty")?;

        let fd = file.as_raw_fd();
        let original_termios = Termios::from_fd(fd).context("Failed to get terminal attributes")?;

        let mut new_termios = original_termios;
        new_termios.c_lflag &= !(ICANON | ECHO);
        tcsetattr(fd, TCSANOW, &new_termios).context("Failed to set terminal to raw mode")?;

        Ok(Self {
            file,
            original_termios,
        })
    }
}

impl Drop for TerminalGuard {
    /// Restores the terminal to its original state.
    ///
    /// This is called automatically when the guard goes out of scope.
    /// Errors during restoration are logged but not propagated since
    /// `Drop` cannot return errors.
    fn drop(&mut self) {
        let fd = self.file.as_raw_fd();
        if let Err(e) = tcsetattr(fd, TCSANOW, &self.original_termios) {
            debug!("Failed to restore terminal attributes: {e}");
        }
    }
}

impl Read for TerminalGuard {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl Write for TerminalGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
