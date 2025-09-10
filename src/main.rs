//! Terminal background color detection utility.
//!
//! This program queries the terminal for its background color using OSC 11 escape sequences,
//! and determines whether it's a dark or light theme based on the relative luminance.

mod color;
mod logs;
mod osc;
mod terminal;

use anyhow::{Context, Result};
use std::process;

use color::{classify_color, luminance, parse_rgb};
use logs::debug;
use osc::query_bg_from_terminal;

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

    Ok(classify_color(rgb))
}

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
