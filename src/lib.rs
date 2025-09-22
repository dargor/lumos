//! Terminal background color detection library.

mod color;
mod logs;
mod osc;
mod terminal;

use anyhow::{Context, Result};

use color::{classify_color, luminance, parse_rgb};
use osc::query_bg_from_terminal;

/// Detect terminal background color and determine if it's dark or light.
///
/// This function orchestrates the entire process:
/// 1. Query the terminal for its background color
/// 2. Parse the response into RGB values
/// 3. Calculate the relative luminance
/// 4. Determine if the background is dark or light
///
/// # Errors
///
/// Returns an error if:
/// - The terminal cannot be queried for its background color
/// - The terminal's response cannot be parsed into valid RGB values
/// - The luminance calculation fails (though this is unlikely given valid RGB input)
///
/// # Returns
///
/// - `Ok("dark")` for dark backgrounds
/// - `Ok("light")` for light backgrounds
/// - `Err` if the background color cannot be determined
pub fn detect_background() -> Result<&'static str> {
    let reply = query_bg_from_terminal().context("Failed to query terminal background color")?;
    debug!("reply={reply:?}");

    let rgb = parse_rgb(&reply).context("Failed to parse color response from terminal")?;
    debug!("rgb={rgb:?}");

    let lum = luminance(&rgb);
    debug!("lum={lum}");

    Ok(classify_color(&rgb))
}
