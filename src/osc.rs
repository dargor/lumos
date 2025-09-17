//! OSC (Operating System Command) query handling for terminal communication.
//!
//! This module provides functions for:
//! - Sending OSC 11 queries to request terminal background color
//! - Reading and parsing terminal responses
//! - Parsing OSC response formats

use anyhow::{Context, Result, anyhow};
use regex::Regex;
use std::fs::File;
use std::io::{Read, Write};

use crate::logs::debug;
use crate::terminal::{open_terminal_device, restore_terminal, setup_raw_mode};

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
fn send_osc_query(file: &mut File) -> Result<()> {
    file.write_all(b"\x1b]11;?\x07")
        .context("Failed to write OSC 11 query to terminal")
}

/// Reads the terminal's response to the OSC 11 query.
///
/// This function reads the terminal's response to an OSC 11 query. It reads
/// data in chunks of 64 bytes and checks for termination sequences (BEL `\x07`
/// or ST `\x1b\\`). The function returns the complete raw response, including
/// the escape sequence prefix and termination.
///
/// # Arguments
///
/// - `file` - Mutable reference to the terminal device file handle
///
/// # Returns
///
/// - `Ok(Vec<u8>)` containing the raw terminal response
/// - `Err` if reading from the terminal fails
fn read_terminal_response(file: &mut File) -> Result<Vec<u8>> {
    let mut buf = Vec::new();

    loop {
        let mut temp_buf = [0u8; 64];
        match file.read(&mut temp_buf) {
            Ok(0) => {
                debug("got EOF");
                break;
            }
            Ok(n) => {
                debug(&format!("got {n} bytes"));
                buf.extend_from_slice(&temp_buf[..n]);
                // Check for terminator (BEL or ST)
                if buf.contains(&b'\x07') || buf.windows(2).any(|w| w == b"\x1b\\") {
                    debug("got terminator");
                    break;
                }
            }
            Err(e) => return Err(anyhow!("Error reading from terminal: {}", e)),
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
fn parse_color_response(buf: Vec<u8>) -> Result<String> {
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
/// 2. Sending the OSC 11 query
/// 3. Reading and parsing the terminal's response
/// 4. Cleaning up terminal state
///
/// The function sends an OSC 11 query (`\x1b]11;?\x07`) to the terminal and waits
/// for a response. The terminal should respond with the current background color
/// in a format like `rgb:RRRR/GGGG/BBBB` or similar.
///
/// # Returns
///
/// - `Ok(String)` containing the color response from the terminal
/// - `Err` if the query fails, times out, or the terminal doesn't support OSC 11
pub fn query_bg_from_terminal() -> Result<String> {
    let mut file = open_terminal_device()?;
    let old_termios = setup_raw_mode(&file)?;

    let result = (|| -> Result<String> {
        send_osc_query(&mut file)?;
        let buf = read_terminal_response(&mut file)?;
        let color_str = parse_color_response(buf)?;

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
    fn test_parse_color_response_edge_cases() {
        // Test empty response
        let response = b"".to_vec();
        assert!(parse_color_response(response).is_err());

        // Test invalid UTF-8
        let response = vec![0xff, 0xfe, 0xfd];
        assert!(parse_color_response(response).is_err());

        // Test malformed response
        let response = b"garbage data".to_vec();
        assert!(parse_color_response(response).is_err());
    }
}
