//! Terminal background color detection utility.
//!
//! This program queries the terminal for its background color using OSC 11 escape sequences,
//! and determines whether it's a dark or light theme based on the relative luminance.

use std::process;

use lumos::detect_background;
use lumos::logs::debug;

/// Main entry point for the lumos terminal background color detection utility.
///
/// # Environment Variables
///
/// - `DEBUG`: When set to any value, enables debug output to stderr.
///
/// # Output
///
/// Prints to stdout one of:
/// - `dark` for dark backgrounds
/// - `light` for light backgrounds
/// - `unknown` when the background cannot be determined
///
/// # Exit Codes
///
/// - `0`: Successfully determined background color
/// - `2`: Unable to determine background color
fn main() {
    match detect_background() {
        Ok(background) => {
            print!("{background}");
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
