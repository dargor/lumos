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

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::poll::{PollFd, PollFlags, poll};
use regex::Regex;
use std::env;
use std::fs::OpenOptions;
use std::io::{Read, Write};
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
/// - `Some(String)` containing the color response from the terminal
/// - `None` if the query fails, times out, or the terminal doesn't support OSC 11
fn query_bg_from_terminal() -> Option<String> {
    // Open /dev/tty for direct terminal access
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;

    // Get a raw file descriptor
    let fd = file.as_raw_fd();

    // Get current terminal attributes
    let old_termios = Termios::from_fd(fd).ok()?;

    // Set terminal to raw mode (no canonical mode, no echo)
    let mut new_termios = old_termios;
    new_termios.c_lflag &= !(ICANON | ECHO);
    if tcsetattr(fd, TCSANOW, &new_termios).is_err() {
        // No need to restore terminal attributes before returning
        return None;
    }
    // From now on, terminal attributes need to be restored before returning

    let result = {
        let Ok(flags) = fcntl(&file, FcntlArg::F_GETFL) else {
            // Restore terminal attributes before returning
            let _ = tcsetattr(fd, TCSANOW, &old_termios);
            return None;
        };

        // Make the file descriptor non-blocking
        let new_flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
        if fcntl(&file, FcntlArg::F_SETFL(new_flags)).is_err() {
            // Restore terminal attributes before returning
            let _ = tcsetattr(fd, TCSANOW, &old_termios);
            return None;
        }
        // From now on, file descriptor needs to be restored before returning

        // Send OSC 11 query
        let query_result = if file.write_all(b"\x1b]11;?\x07").is_err() {
            None
        } else {
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
                                if buf.contains(&b'\x07') || buf.windows(2).any(|w| w == b"\x1b\\")
                                {
                                    break;
                                }
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                // No data available, sleep briefly and try again
                            }
                            Err(_) => break,
                        }
                    }
                    Err(_) => break,
                }
            }

            // Parse the response
            let response = String::from_utf8(buf).ok()?;
            let re = Regex::new(r"]\s*11;([^\x07\x1b]*)").unwrap();
            re.captures(&response)
                .and_then(|caps| caps.get(1))
                .map(|m| m.as_str().to_string())
        };

        // Restore blocking mode (ignore errors as this is cleanup)
        let original_flags = OFlag::from_bits_truncate(flags);
        let _ = fcntl(&file, FcntlArg::F_SETFL(original_flags));

        query_result
    };

    // Restore terminal attributes
    let _ = tcsetattr(fd, TCSANOW, &old_termios);

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
/// - `Some((r, g, b))` where each component is 0-255
/// - `None` if the string cannot be parsed as a valid color
///
/// # Examples
///
/// ```
/// assert_eq!(parse_rgb("rgb:ff00/8000/0000"), Some((255, 128, 0)));
/// assert_eq!(parse_rgb("#ff8000"), Some((255, 128, 0)));
/// assert_eq!(parse_rgb("rgb(255, 128, 0)"), Some((255, 128, 0)));
/// ```
fn parse_rgb(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();

    // Handle rgb: or rgba: format
    if s.starts_with("rgb:") || s.starts_with("rgba:") {
        let parts: Vec<&str> = s.split_once(':')?.1.split('/').collect();
        if parts.len() == 3 || parts.len() == 4 {
            let r = hex_to_u8(parts[0])?;
            let g = hex_to_u8(parts[1])?;
            let b = hex_to_u8(parts[2])?;
            return Some((r, g, b));
        }
    }

    // Handle #hex format
    if s.starts_with('#') && (s.len() == 7 || s.len() == 9) {
        let r = u8::from_str_radix(&s[1..3], 16).ok()?;
        let g = u8::from_str_radix(&s[3..5], 16).ok()?;
        let b = u8::from_str_radix(&s[5..7], 16).ok()?;
        return Some((r, g, b));
    }

    // Handle rgb() format
    let re = Regex::new(r"rgb\((\d+),\s*(\d+),\s*(\d+)\)").unwrap();
    if let Some(caps) = re.captures(s) {
        let r = caps[1].parse().ok()?;
        let g = caps[2].parse().ok()?;
        let b = caps[3].parse().ok()?;
        return Some((r, g, b));
    }

    None
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
/// - `Some(u8)` - The converted value
/// - `None` - If the string is not valid hex
fn hex_to_u8(hex: &str) -> Option<u8> {
    let n = u32::from_str_radix(hex, 16).ok()?;
    if hex.len() == 2 {
        // For 2-digit hex values, directly convert to u8
        #[allow(clippy::cast_possible_truncation)]
        Some(n as u8)
    } else if hex.len() == 4 {
        // For longer hex values, scale from 16-bit to 8-bit
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(((f64::from(n) / 65535.0) * 255.0).round() as u8)
    } else {
        None
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

/// Main entry point for the lumos terminal background color detection utility.
///
/// This function orchestrates the entire process:
/// 1. Query the terminal for its background color
/// 2. Parse the response into RGB values
/// 3. Calculate the relative luminance
/// 4. Determine if the background is dark or light
/// 5. Output the result and exit with appropriate code
fn main() {
    let reply = query_bg_from_terminal();
    debug(&format!("reply={reply:?}"));

    let rgb = reply.and_then(|r| parse_rgb(&r));
    debug(&format!("rgb={rgb:?}"));

    if let Some(rgb) = rgb {
        let lum = luminance(rgb);
        debug(&format!("lum={lum}"));

        if lum < DARK_THRESHOLD {
            print!("dark");
        } else {
            print!("light");
        }
        process::exit(0);
    }

    debug("unable to determine background color");
    print!("unknown");
    process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rgb_hex() {
        assert_eq!(parse_rgb("#000000"), Some((0, 0, 0)));
        assert_eq!(parse_rgb("#ff0000"), Some((255, 0, 0)));
        assert_eq!(parse_rgb("#00ff00"), Some((0, 255, 0)));
        assert_eq!(parse_rgb("#0000ff"), Some((0, 0, 255)));
        assert_eq!(parse_rgb("#ffffff"), Some((255, 255, 255)));
        assert_eq!(parse_rgb("#ff00ff"), Some((255, 0, 255)));
        assert_eq!(parse_rgb("#ff0000ff"), Some((255, 0, 0)));
        assert_eq!(parse_rgb("#AbC123"), Some((171, 193, 35)));
        assert_eq!(parse_rgb("#001122"), Some((0, 17, 34)));
        assert_eq!(parse_rgb("  #ff0000  "), Some((255, 0, 0)));
        assert_eq!(parse_rgb("#gg0000"), None);
        assert_eq!(parse_rgb("#f00"), None);
        assert_eq!(parse_rgb("#ff0000ff00"), None);
    }

    #[test]
    fn test_parse_rgb_rgb_format() {
        assert_eq!(parse_rgb("rgb(0,0,0)"), Some((0, 0, 0)));
        assert_eq!(parse_rgb("rgb(255,0,0)"), Some((255, 0, 0)));
        assert_eq!(parse_rgb("rgb(0,255,0)"), Some((0, 255, 0)));
        assert_eq!(parse_rgb("rgb(0,0,255)"), Some((0, 0, 255)));
        assert_eq!(parse_rgb("rgb(255,255,255)"), Some((255, 255, 255)));
        assert_eq!(parse_rgb("rgb(255,0,255)"), Some((255, 0, 255)));
        assert_eq!(parse_rgb("rgb(171,193,35)"), Some((171, 193, 35)));
        assert_eq!(parse_rgb("rgb(0,17,34)"), Some((0, 17, 34)));
        assert_eq!(parse_rgb("  rgb(255,0,0)  "), Some((255, 0, 0)));
        assert_eq!(parse_rgb("rgb(0,0,256)"), None);
        assert_eq!(parse_rgb("rgb(0,0)"), None);
        assert_eq!(parse_rgb("rgb(0,0,0,0)"), None);
        assert_eq!(parse_rgb("rgb(0,0,0,0,0)"), None);
        assert_eq!(parse_rgb("rgb(0,0,0,0)"), None);
    }

    #[test]
    fn test_parse_rgb_rgb_colon_format() {
        assert_eq!(parse_rgb("rgb:0000/0000/0000"), Some((0, 0, 0)));
        assert_eq!(parse_rgb("rgb:ffff/0000/0000"), Some((255, 0, 0)));
        assert_eq!(parse_rgb("rgb:0000/ffff/0000"), Some((0, 255, 0)));
        assert_eq!(parse_rgb("rgb:0000/0000/ffff"), Some((0, 0, 255)));
        assert_eq!(parse_rgb("rgb:ffff/ffff/ffff"), Some((255, 255, 255)));
        assert_eq!(parse_rgb("rgb:ffff/0000/ffff"), Some((255, 0, 255)));
        assert_eq!(parse_rgb("rgb:abcd/C1AB/230A"), Some((171, 193, 35)));
        assert_eq!(parse_rgb("  rgb:00/11/22  "), Some((0, 17, 34)));
        assert_eq!(parse_rgb("rgb:ff00/0000/0000"), Some((254, 0, 0)));
        assert_eq!(parse_rgb("rgb:gggg/gggg/gggg"), None);
        assert_eq!(parse_rgb("rgb:000/000/000"), None);
        assert_eq!(parse_rgb("rgb:00000/00000/00000"), None);
        assert_eq!(parse_rgb("rgb:1111/2222/3333/4444"), Some((17, 34, 51)));
        assert_eq!(parse_rgb("rgba:1111/2222/3333/4444"), Some((17, 34, 51)));
        assert_eq!(parse_rgb("rgb:0000/0000/0000/0000/0000"), None);
    }

    #[test]
    fn test_hex_to_u8() {
        assert_eq!(hex_to_u8("00"), Some(0));
        assert_eq!(hex_to_u8("ff"), Some(255));
        assert_eq!(hex_to_u8("ffff"), Some(255));
        assert_eq!(hex_to_u8("0000"), Some(0));
        assert_eq!(hex_to_u8("8000"), Some(128));
        assert_eq!(hex_to_u8("7fff"), Some(127));
        assert_eq!(hex_to_u8("0080"), Some(0));
        assert_eq!(hex_to_u8("1234"), Some(18));
        assert_eq!(hex_to_u8("abcd"), Some(171));
        assert_eq!(hex_to_u8("00000"), None);
        assert_eq!(hex_to_u8("123"), None);
        assert_eq!(hex_to_u8("xyz"), None);
        assert_eq!(hex_to_u8(""), None);
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
}
