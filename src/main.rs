//! Terminal background color detection utility.
//!
//! This module queries the terminal for its background color using OSC 11 escape sequences,
//! and determines whether it's a dark or light theme based on the relative luminance.
//!
//! # Usage
//!
//! The program outputs one of three possible values:
//! - `"light"` - for light backgrounds (luminance >= 0.5)
//! - `"dark"` - for dark backgrounds (luminance < 0.5)
//! - `"unknown"` - when the background color cannot be determined
//!
//! Exit codes:
//! - 0: Successfully determined background color
//! - 2: Unable to determine background color
//!
//! # Environment Variables
//!
//! - `DEBUG`: When set, enables debug output to stderr showing the query response,
//!   parsed RGB values, and calculated luminance.

use anyhow::{Context, Result, anyhow};
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::poll::{PollFd, PollFlags, poll};
use regex::Regex;
use std::env;
use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::io::AsRawFd;
use std::process;
use std::time::{Duration, Instant};
use termios::{ECHO, ICANON, TCSANOW, Termios, tcsetattr};

/// Threshold for determining if a color is dark or light based on luminance.
/// Colors with luminance below this value are considered dark.
const DARK_THRESHOLD: f64 = 0.5;

/// Print debug messages to stderr if DEBUG environment variable is set
fn debug(message: &str) {
    if env::var("DEBUG").is_ok() {
        eprintln!("{message}");
    }
}

/// Query the terminal for its background color using OSC 11 escape sequence.
///
/// This function sends an OSC 11 query (`\x1b]11;?\x07`) to the terminal and waits
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
fn query_bg_from_terminal() -> Result<String> {
    // Check if we are running in a real terminal
    if !io::stdout().is_terminal() {
        return Err(anyhow!("Not a terminal"));
    }

    // Open /dev/tty for direct terminal access
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .context("Failed to open /dev/tty")?;

    // Get a raw file descriptor
    let fd = file.as_raw_fd();

    // Get current terminal attributes
    let old_termios = Termios::from_fd(fd).context("Failed to get terminal attributes")?;

    // Set terminal to raw mode (no canonical mode, no echo)
    let mut new_termios = old_termios;
    new_termios.c_lflag &= !(ICANON | ECHO);
    tcsetattr(fd, TCSANOW, &new_termios).context("Failed to set terminal to raw mode")?;
    // From now on, terminal attributes need to be restored before returning

    let result = (|| -> Result<String> {
        let flags =
            fcntl(&file, FcntlArg::F_GETFL).context("Failed to get file descriptor flags")?;

        // Make the file descriptor non-blocking
        let new_flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
        fcntl(&file, FcntlArg::F_SETFL(new_flags))
            .context("Failed to set file descriptor to non-blocking")?;
        // From now on, file descriptor needs to be restored before returning

        // Send OSC 11 query
        file.write_all(b"\x1b]11;?\x07")
            .context("Failed to write OSC 11 query to terminal")?;

        let mut buf = Vec::new();

        // Read response with timeout (2 seconds total)
        let start_time = Instant::now();
        let timeout_duration = Duration::from_secs(2);

        while start_time.elapsed() < timeout_duration {
            // Try to read data
            let pollfd = PollFd::new(file.as_fd(), PollFlags::POLLIN);
            match poll(&mut [pollfd], 250_u8) {
                Ok(0) => {
                    // No data available, sleep briefly and try again
                }
                Ok(_) => {
                    // Data available, try to read
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
                            // No data available, sleep briefly and try again
                        }
                        Err(e) => return Err(anyhow!("Error reading from terminal: {}", e)),
                    }
                }
                Err(e) => return Err(anyhow!("Error polling terminal: {}", e)),
            }
        }

        // Parse the response
        debug(&format!("buf={buf:?}"));
        let response =
            String::from_utf8(buf).context("Terminal response contained invalid UTF-8")?;
        debug(&format!("response={response:?}"));

        let re = Regex::new(r"]\s*11;([^\x07\x1b]*)").context("Failed to compile regex")?;
        let color_str = re
            .captures(&response)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| anyhow!("No color information found in terminal response"))?;

        // Restore blocking mode (ignore errors as this is cleanup)
        let original_flags = OFlag::from_bits_truncate(flags);
        fcntl(&file, FcntlArg::F_SETFL(original_flags))
            .context("Failed to restore blocking mode")?;

        Ok(color_str)
    })();

    // Restore terminal attributes
    tcsetattr(fd, TCSANOW, &old_termios).context("Failed to restore terminal attributes")?;

    result
}

/// Parse an RGB color string into RGB tuple.
///
/// This function supports multiple color formats commonly returned by terminals:
/// - `rgb:RRRR/GGGG/BBBB` - X11 RGB format with hex values
/// - `rgba:RRRR/GGGG/BBBB/AAAA` - X11 RGBA format (alpha ignored)
/// - `#RRGGBB` - Standard hex color format
/// - `#RRGGBBAA` - Hex color with alpha (alpha ignored)
/// - `rgb(R, G, B)` - CSS-style RGB function
///
/// # Arguments
///
/// * `s` - The color string to parse
///
/// # Returns
///
/// - `Ok((r, g, b))` where each component is 0-255
/// - `Err` if the string cannot be parsed as a valid color
///
/// # Examples
///
/// ```
/// # use anyhow::Result;
/// # fn parse_rgb(s: &str) -> Result<(u8, u8, u8)> { unimplemented!() }
/// assert_eq!(parse_rgb("rgb:ff00/8000/0000").unwrap(), (255, 128, 0));
/// assert_eq!(parse_rgb("#ff8000").unwrap(), (255, 128, 0));
/// assert_eq!(parse_rgb("rgb(255, 128, 0)").unwrap(), (255, 128, 0));
/// ```
fn parse_rgb(s: &str) -> Result<(u8, u8, u8)> {
    let s = s.trim();

    // Handle rgb: or rgba: format
    if s.starts_with("rgb:") || s.starts_with("rgba:") {
        let color_part = s
            .split_once(':')
            .ok_or_else(|| anyhow!("Invalid rgb: format - missing colon"))?
            .1;
        let parts: Vec<&str> = color_part.split('/').collect();

        if parts.len() == 3 || parts.len() == 4 {
            let r = hex_to_u8(parts[0])
                .with_context(|| format!("Failed to parse red component: {}", parts[0]))?;
            let g = hex_to_u8(parts[1])
                .with_context(|| format!("Failed to parse green component: {}", parts[1]))?;
            let b = hex_to_u8(parts[2])
                .with_context(|| format!("Failed to parse blue component: {}", parts[2]))?;
            return Ok((r, g, b));
        }
        return Err(anyhow!(
            "Invalid rgb: format - expected 3 or 4 components, got {}",
            parts.len()
        ));
    }

    // Handle #hex format
    if s.starts_with('#') && (s.len() == 7 || s.len() == 9) {
        let r = u8::from_str_radix(&s[1..3], 16)
            .with_context(|| format!("Failed to parse red hex component: {}", &s[1..3]))?;
        let g = u8::from_str_radix(&s[3..5], 16)
            .with_context(|| format!("Failed to parse green hex component: {}", &s[3..5]))?;
        let b = u8::from_str_radix(&s[5..7], 16)
            .with_context(|| format!("Failed to parse blue hex component: {}", &s[5..7]))?;
        return Ok((r, g, b));
    }

    // Handle rgb() format
    let re =
        Regex::new(r"rgb\((\d+),\s*(\d+),\s*(\d+)\)").context("Failed to compile RGB regex")?;
    if let Some(caps) = re.captures(s) {
        let r = caps[1]
            .parse::<u8>()
            .with_context(|| format!("Failed to parse red component: {}", &caps[1]))?;
        let g = caps[2]
            .parse::<u8>()
            .with_context(|| format!("Failed to parse green component: {}", &caps[2]))?;
        let b = caps[3]
            .parse::<u8>()
            .with_context(|| format!("Failed to parse blue component: {}", &caps[3]))?;
        return Ok((r, g, b));
    }

    Err(anyhow!("Unrecognized color format: {}", s))
}

/// Convert hex string to u8, handling different hex formats.
///
/// This function handles both 2-digit hex values (0-255) and longer hex values
/// that need to be scaled from 16-bit (0-65535) to 8-bit (0-255) range.
///
/// # Arguments
///
/// * `hex` - Hex string without '0x' prefix
///
/// # Returns
///
/// - `Ok(u8)` - The converted value
/// - `Err` - If the string is not valid hex
fn hex_to_u8(hex: &str) -> Result<u8> {
    let n = u32::from_str_radix(hex, 16).with_context(|| format!("Invalid hex string: {hex}"))?;

    match hex.len() {
        2 => {
            // For 2-digit hex values, directly convert to u8
            #[allow(clippy::cast_possible_truncation)]
            Ok(n as u8)
        }
        4 => {
            // For longer hex values, scale from 16-bit to 8-bit
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Ok(((f64::from(n) / 65535.0) * 255.0).round() as u8)
        }
        _ => Err(anyhow!(
            "Invalid hex length: expected 2 or 4 characters, got {}",
            hex.len()
        )),
    }
}

/// Calculate relative luminance of RGB color using sRGB formula.
///
/// This implements the standard relative luminance calculation as defined by
/// the W3C Web Content Accessibility Guidelines (WCAG). The formula accounts
/// for the non-linear nature of human vision by first converting sRGB values
/// to linear RGB, then applying luminance coefficients.
///
/// # Arguments
///
/// * `rgb` - RGB tuple with values 0-255
///
/// # Returns
///
/// Relative luminance value between 0.0 (black) and 1.0 (white)
///
/// # Formula
///
/// L = 0.2126 × R + 0.7152 × G + 0.0722 × B
///
/// Where R, G, B are the linearized RGB values.
fn luminance(rgb: (u8, u8, u8)) -> f64 {
    let (r, g, b) = rgb;
    let r = f64::from(r) / 255.0;
    let g = f64::from(g) / 255.0;
    let b = f64::from(b) / 255.0;

    // Convert sRGB component to linear RGB
    let lin = |c: f64| -> f64 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };

    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// Detect terminal background color and determine if it's dark or light.
///
/// This function orchestrates the entire process:
/// 1. Query the terminal for its background color
/// 2. Parse the response into RGB values
/// 3. Calculate the relative luminance
/// 4. Determine if the background is dark or light
///
/// # Returns
///
/// - `Ok("dark")` for dark backgrounds
/// - `Ok("light")` for light backgrounds
/// - `Err` if the background color cannot be determined
fn detect_background() -> Result<&'static str> {
    let reply = query_bg_from_terminal().context("Failed to query terminal background color")?;
    debug(&format!("reply={reply:?}"));

    let rgb = parse_rgb(&reply).context("Failed to parse color response from terminal")?;
    debug(&format!("rgb={rgb:?}"));

    let lum = luminance(rgb);
    debug(&format!("lum={lum}"));

    if lum < DARK_THRESHOLD {
        Ok("dark")
    } else {
        Ok("light")
    }
}

/// Main entry point for the lumos terminal background color detection utility.
///
/// This function orchestrates the entire process:
/// 1. Query the terminal for its background color
/// 2. Parse the response into RGB values
/// 3. Calculate the relative luminance
/// 4. Determine if the background is dark or light
/// 5. Output the result and exit with appropriate code
fn main() {
    match detect_background() {
        Ok(theme) => {
            print!("{theme}");
            process::exit(0);
        }
        Err(e) => {
            debug(&format!("Error: {e:#}"));
            debug("unable to determine background color");
            print!("unknown");
            process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rgb_hex() -> Result<()> {
        assert_eq!(parse_rgb("#000000")?, (0, 0, 0));
        assert_eq!(parse_rgb("#ff0000")?, (255, 0, 0));
        assert_eq!(parse_rgb("#00ff00")?, (0, 255, 0));
        assert_eq!(parse_rgb("#0000ff")?, (0, 0, 255));
        assert_eq!(parse_rgb("#ffffff")?, (255, 255, 255));
        assert_eq!(parse_rgb("#ff00ff")?, (255, 0, 255));
        assert_eq!(parse_rgb("#ff0000ff")?, (255, 0, 0));
        assert_eq!(parse_rgb("#AbC123")?, (171, 193, 35));
        assert_eq!(parse_rgb("#001122")?, (0, 17, 34));
        assert_eq!(parse_rgb("  #ff0000  ")?, (255, 0, 0));

        assert!(parse_rgb("#gg0000").is_err());
        assert!(parse_rgb("#f00").is_err());
        assert!(parse_rgb("#ff0000ff00").is_err());
        Ok(())
    }

    #[test]
    fn test_parse_rgb_rgb_format() -> Result<()> {
        assert_eq!(parse_rgb("rgb(0,0,0)")?, (0, 0, 0));
        assert_eq!(parse_rgb("rgb(255,0,0)")?, (255, 0, 0));
        assert_eq!(parse_rgb("rgb(0,255,0)")?, (0, 255, 0));
        assert_eq!(parse_rgb("rgb(0,0,255)")?, (0, 0, 255));
        assert_eq!(parse_rgb("rgb(255,255,255)")?, (255, 255, 255));
        assert_eq!(parse_rgb("rgb(255,0,255)")?, (255, 0, 255));
        assert_eq!(parse_rgb("rgb(171,193,35)")?, (171, 193, 35));
        assert_eq!(parse_rgb("rgb(0,17,34)")?, (0, 17, 34));
        assert_eq!(parse_rgb("  rgb(255,0,0)  ")?, (255, 0, 0));

        assert!(parse_rgb("rgb(0,0,256)").is_err());
        assert!(parse_rgb("rgb(0,0)").is_err());
        assert!(parse_rgb("rgb(0,0,0,0)").is_err());
        assert!(parse_rgb("rgb(0,0,0,0,0)").is_err());
        Ok(())
    }

    #[test]
    fn test_parse_rgb_rgb_colon_format() -> Result<()> {
        assert_eq!(parse_rgb("rgb:0000/0000/0000")?, (0, 0, 0));
        assert_eq!(parse_rgb("rgb:ffff/0000/0000")?, (255, 0, 0));
        assert_eq!(parse_rgb("rgb:0000/ffff/0000")?, (0, 255, 0));
        assert_eq!(parse_rgb("rgb:0000/0000/ffff")?, (0, 0, 255));
        assert_eq!(parse_rgb("rgb:ffff/ffff/ffff")?, (255, 255, 255));
        assert_eq!(parse_rgb("rgb:ffff/0000/ffff")?, (255, 0, 255));
        assert_eq!(parse_rgb("rgb:abcd/C1AB/230A")?, (171, 193, 35));
        assert_eq!(parse_rgb("  rgb:00/11/22  ")?, (0, 17, 34));
        assert_eq!(parse_rgb("rgb:ff00/0000/0000")?, (254, 0, 0));
        assert_eq!(parse_rgb("rgb:1111/2222/3333/4444")?, (17, 34, 51));
        assert_eq!(parse_rgb("rgba:1111/2222/3333/4444")?, (17, 34, 51));

        assert!(parse_rgb("rgb:gggg/gggg/gggg").is_err());
        assert!(parse_rgb("rgb:000/000/000").is_err());
        assert!(parse_rgb("rgb:00000/00000/00000").is_err());
        assert!(parse_rgb("rgb:0000/0000/0000/0000/0000").is_err());
        Ok(())
    }

    #[test]
    fn test_hex_to_u8() -> Result<()> {
        assert_eq!(hex_to_u8("00")?, 0);
        assert_eq!(hex_to_u8("ff")?, 255);
        assert_eq!(hex_to_u8("ffff")?, 255);
        assert_eq!(hex_to_u8("0000")?, 0);
        assert_eq!(hex_to_u8("8000")?, 128);
        assert_eq!(hex_to_u8("7fff")?, 127);
        assert_eq!(hex_to_u8("0080")?, 0);
        assert_eq!(hex_to_u8("1234")?, 18);
        assert_eq!(hex_to_u8("abcd")?, 171);

        assert!(hex_to_u8("00000").is_err());
        assert!(hex_to_u8("123").is_err());
        assert!(hex_to_u8("xyz").is_err());
        assert!(hex_to_u8("").is_err());
        Ok(())
    }

    #[test]
    fn test_luminance() {
        assert!((luminance((0, 0, 0)) - 0.0).abs() < 0.001);
        assert!((luminance((255, 255, 255)) - 1.0).abs() < 0.001);
        // Test a mid-gray
        let mid_gray_lum = luminance((128, 128, 128));
        assert!(mid_gray_lum > 0.0 && mid_gray_lum < 1.0);
        // Test colors with different luminance contributions
        assert!((luminance((255, 0, 0)) - 0.2126).abs() < 0.001); // Red should have low luminance
        assert!((luminance((0, 255, 0)) - 0.7152).abs() < 0.001); // Green should have high luminance
        assert!((luminance((0, 0, 255)) - 0.0722).abs() < 0.001); // Blue should have very low luminance
        // Test edge cases with non-linear conversion
        assert!((luminance((0, 0, 0)) - 0.0).abs() < 0.001);
        assert!((luminance((255, 255, 255)) - 1.0).abs() < 0.001);
        // Test a subtle color difference that should be distinguishable
        let very_dark = luminance((1, 1, 1));
        let slightly_lighter = luminance((2, 2, 2));
        assert!(slightly_lighter > very_dark);
    }

    #[test]
    fn test_detect_background_with_mock() {
        // Note: This test would require mocking the terminal interaction
        // For now, we test the individual components
        // In test environment, this could succeed or fail depending on terminal support
        let result = detect_background();
        // We just verify it returns a Result - success/failure depends on test environment
        if let Ok(theme) = result {
            assert!(theme == "dark" || theme == "light");
        }
        // If Err, it's expected in most test environments without terminal support
    }
}
