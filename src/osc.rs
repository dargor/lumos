//! OSC (Operating System Command) query handling for terminal communication.
//!
//! This module provides functions for:
//! - Sending OSC 11 queries to request terminal background color
//! - Reading and parsing terminal responses
//! - Handling timeouts and non-blocking I/O
//! - Parsing OSC response formats

use anyhow::{Context, Result, anyhow};
use nix::poll::{PollFd, PollFlags, poll};
use regex::Regex;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use crate::logs::debug;

/// Sends an OSC 11 query to request the terminal's background color.
///
/// OSC (Operating System Command) 11 is a standard escape sequence
/// that queries the terminal for its background color. The sequence
/// `\x1b]11;?\x07` asks the terminal to respond with its current
/// background color in RGB format.
///
/// # Arguments
///
/// - `file` - Mutable reference to the terminal device file handle
///
/// # Returns
///
/// - `Ok(())` if the query was sent successfully
/// - `Err` if writing to the terminal fails
pub fn send_osc_query(file: &mut File) -> Result<()> {
    file.write_all(b"\x1b]11;?\x07")
        .context("Failed to write OSC 11 query to terminal")
}

/// Reads the terminal's response to the OSC 11 query with timeout.
///
/// This function polls the terminal for data with a 2-second timeout,
/// reading the response in chunks and looking for proper termination
/// sequences (BEL `\x07` or ST `\x1b\\`). It uses non-blocking I/O
/// to prevent hanging if the terminal doesn't respond.
///
/// # Arguments
///
/// - `file` - Mutable reference to the terminal device file handle
///
/// # Returns
///
/// - `Ok(Vec<u8>)` containing the raw terminal response
/// - `Err` if polling fails, read operations fail, or timeout occurs
pub fn read_terminal_response(file: &mut File) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let start_time = Instant::now();
    let timeout_duration = Duration::from_secs(2);

    while start_time.elapsed() < timeout_duration {
        let pollfd = PollFd::new(file.as_fd(), PollFlags::POLLIN);
        match poll(&mut [pollfd], 250_u8) {
            Ok(0) => {
                // No data available, continue polling
            }
            Ok(_) => {
                let mut temp_buf = [0u8; 64];
                match file.read(&mut temp_buf) {
                    Ok(0) => {
                        debug("got EOF");
                        break;
                    }
                    Ok(n) => {
                        buf.extend_from_slice(&temp_buf[..n]);
                        // Check for terminator (BEL or ST)
                        if buf.contains(&b'\x07') || buf.windows(2).any(|w| w == b"\x1b\\") {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No data available, continue polling
                    }
                    Err(e) => return Err(anyhow!("Error reading from terminal: {}", e)),
                }
            }
            Err(e) => return Err(anyhow!("Error polling terminal: {}", e)),
        }
    }

    Ok(buf)
}

/// Parses the terminal's OSC 11 response to extract color information.
///
/// The terminal response typically looks like `\x1b]11;rgb:RRRR/GGGG/BBBB\x07`
/// where RRRR, GGGG, BBBB are hexadecimal color values. This function uses
/// a regular expression to extract the color string portion.
///
/// # Arguments
///
/// - `buf` - Raw bytes from the terminal response
///
/// # Returns
///
/// - `Ok(String)` containing the color specification (e.g., "rgb:0000/0000/0000")
/// - `Err` if the response contains invalid UTF-8 or doesn't match expected format
pub fn parse_color_response(buf: Vec<u8>) -> Result<String> {
    debug(&format!("buf={buf:?}"));
    let response = String::from_utf8(buf).context("Terminal response contained invalid UTF-8")?;
    debug(&format!("response={response:?}"));

    let re = Regex::new(r"]\s*11;([^\x07\x1b]*)").context("Failed to compile regex")?;
    let color_str = re
        .captures(&response)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| anyhow!("No color information found in terminal response"))?;

    Ok(color_str)
}

/// Query the terminal for its background color using OSC 11 escape sequence.
///
/// This function orchestrates the complete process of querying a terminal
/// for its background color by:
/// 1. Opening the terminal device for direct access
/// 2. Setting up raw mode and non-blocking I/O
/// 3. Sending the OSC 11 query
/// 4. Reading and parsing the terminal's response
/// 5. Cleaning up terminal state
///
/// The function sends an OSC 11 query (`\x1b]11;?\x07`) to the terminal and waits
/// for a response. The terminal should respond with the current background color
/// in a format like `rgb:RRRR/GGGG/BBBB` or similar.
///
/// The function implements a timeout mechanism (approximately 2 seconds) to avoid
/// hanging indefinitely if the terminal doesn't support this query.
///
/// # Returns
///
/// - `Ok(String)` containing the color response from the terminal
/// - `Err` if the query fails, times out, or the terminal doesn't support OSC 11
pub fn query_bg_from_terminal() -> Result<String> {
    use crate::terminal::{
        open_terminal_device, restore_flags, restore_terminal, setup_non_blocking, setup_raw_mode,
    };

    let mut file = open_terminal_device()?;
    let old_termios = setup_raw_mode(&file)?;

    let result = (|| -> Result<String> {
        let original_flags = setup_non_blocking(&file)?;

        send_osc_query(&mut file)?;
        let buf = read_terminal_response(&mut file)?;
        let color_str = parse_color_response(buf)?;

        // Restore blocking mode before returning
        restore_flags(&file, original_flags)?;

        Ok(color_str)
    })();

    // Always restore terminal attributes
    restore_terminal(&file, &old_termios).context("Failed to restore terminal attributes")?;

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_response() -> Result<()> {
        // Test standard OSC 11 response
        let response = b"\x1b]11;rgb:0000/0000/0000\x07".to_vec();
        assert_eq!(parse_color_response(response)?, "rgb:0000/0000/0000");

        // Test with whitespace
        let response = b"\x1b] 11;rgb:ffff/8000/0000\x07".to_vec();
        assert_eq!(parse_color_response(response)?, "rgb:ffff/8000/0000");

        // Test with ST terminator
        let response = b"\x1b]11;rgb:1234/5678/9abc\x1b\\".to_vec();
        assert_eq!(parse_color_response(response)?, "rgb:1234/5678/9abc");

        // Test hex format
        let response = b"\x1b]11;#ff8000\x07".to_vec();
        assert_eq!(parse_color_response(response)?, "#ff8000");

        // Test invalid response
        let response = b"\x1b]10;rgb:0000/0000/0000\x07".to_vec(); // Wrong OSC number
        assert!(parse_color_response(response).is_err());

        Ok(())
    }

    #[test]
    fn test_parse_color_response_edge_cases() -> Result<()> {
        // Test empty response
        let response = b"".to_vec();
        assert!(parse_color_response(response).is_err());

        // Test invalid UTF-8
        let response = vec![0xff, 0xfe, 0xfd];
        assert!(parse_color_response(response).is_err());

        // Test malformed response
        let response = b"garbage data".to_vec();
        assert!(parse_color_response(response).is_err());

        Ok(())
    }
}
