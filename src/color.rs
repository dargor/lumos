//! Color parsing and luminance calculation utilities.
//!
//! This module provides functions for:
//! - Parsing various color formats (hex, RGB, X11 format)
//! - Converting between color representations
//! - Calculating relative luminance for accessibility
//! - Determining if colors are dark or light

use anyhow::{Context, Result, anyhow};
use regex::Regex;

/// Threshold for determining if a color is dark or light based on luminance.
/// Colors with luminance below this value are considered dark.
const DARK_THRESHOLD: f64 = 0.5;

/// RGB color representation with red, green, and blue components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RGB {
    /// Red component (0-255)
    pub r: u8,
    /// Green component (0-255)
    pub g: u8,
    /// Blue component (0-255)
    pub b: u8,
}

impl RGB {
    /// Create a new RGB color from individual components.
    ///
    /// # Arguments
    ///
    /// * `r` - Red component (0-255)
    /// * `g` - Green component (0-255)
    /// * `b` - Blue component (0-255)
    ///
    /// # Returns
    ///
    /// A new RGB struct with the specified components.
    #[must_use]
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Create a new RGB color from a tuple.
    ///
    /// # Arguments
    ///
    /// * `rgb` - Tuple containing (r, g, b) components
    ///
    /// # Returns
    ///
    /// A new RGB struct with the specified components.
    #[must_use]
    pub fn from_tuple(rgb: (u8, u8, u8)) -> Self {
        Self {
            r: rgb.0,
            g: rgb.1,
            b: rgb.2,
        }
    }

    /// Convert RGB color to a tuple.
    ///
    /// # Returns
    ///
    /// A tuple containing (r, g, b) components.
    #[must_use]
    pub fn to_tuple(self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }
}

/// Parse an RGB color string into RGB struct.
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
/// - `Ok(RGB)` with each component in range 0-255
/// - `Err` if the string cannot be parsed as a valid color
///
/// # Errors
///
/// This function returns an error in the following cases:
/// - The string is not in a recognized color format
/// - A component value is invalid (e.g., non-hex characters, out of range)
/// - The hex string has an invalid length (not 2 or 4 digits for hex values)
/// - The RGB values are out of range (0-255)
///
/// # Examples
///
/// ```
/// # use lumos::color::{RGB,parse_rgb};
/// assert_eq!(parse_rgb("rgb:ffff/8080/0000").unwrap(), RGB::new(255, 128, 0));
/// assert_eq!(parse_rgb("#ff8000").unwrap(), RGB::new(255, 128, 0));
/// assert_eq!(parse_rgb("rgb(255, 128, 0)").unwrap(), RGB::new(255, 128, 0));
/// ```
pub fn parse_rgb(s: &str) -> Result<RGB> {
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
            return Ok(RGB::new(r, g, b));
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
        return Ok(RGB::new(r, g, b));
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
        return Ok(RGB::new(r, g, b));
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
/// * `rgb` - RGB struct with values 0-255
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
#[must_use]
pub fn luminance(rgb: RGB) -> f64 {
    let r = f64::from(rgb.r) / 255.0;
    let g = f64::from(rgb.g) / 255.0;
    let b = f64::from(rgb.b) / 255.0;

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

/// Determine if a color is dark or light based on its luminance.
///
/// # Arguments
///
/// * `rgb` - RGB struct with values 0-255
///
/// # Returns
///
/// - `"dark"` if luminance < `DARK_THRESHOLD`
/// - `"light"` if luminance >= `DARK_THRESHOLD`
#[must_use]
pub fn classify_color(rgb: RGB) -> &'static str {
    let lum = luminance(rgb);
    if lum < DARK_THRESHOLD {
        "dark"
    } else {
        "light"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rgb_hex() -> Result<()> {
        assert_eq!(parse_rgb("#000000")?, RGB::new(0, 0, 0));
        assert_eq!(parse_rgb("#ff0000")?, RGB::new(255, 0, 0));
        assert_eq!(parse_rgb("#00ff00")?, RGB::new(0, 255, 0));
        assert_eq!(parse_rgb("#0000ff")?, RGB::new(0, 0, 255));
        assert_eq!(parse_rgb("#ffffff")?, RGB::new(255, 255, 255));
        assert_eq!(parse_rgb("#ff00ff")?, RGB::new(255, 0, 255));
        assert_eq!(parse_rgb("#ff0000ff")?, RGB::new(255, 0, 0));
        assert_eq!(parse_rgb("#AbC123")?, RGB::new(171, 193, 35));
        assert_eq!(parse_rgb("#001122")?, RGB::new(0, 17, 34));
        assert_eq!(parse_rgb("  #ff0000  ")?, RGB::new(255, 0, 0));

        assert!(parse_rgb("#gg0000").is_err());
        assert!(parse_rgb("#f00").is_err());
        assert!(parse_rgb("#ff0000ff00").is_err());
        Ok(())
    }

    #[test]
    fn test_parse_rgb_rgb_format() -> Result<()> {
        assert_eq!(parse_rgb("rgb(0,0,0)")?, RGB::new(0, 0, 0));
        assert_eq!(parse_rgb("rgb(255,0,0)")?, RGB::new(255, 0, 0));
        assert_eq!(parse_rgb("rgb(0,255,0)")?, RGB::new(0, 255, 0));
        assert_eq!(parse_rgb("rgb(0,0,255)")?, RGB::new(0, 0, 255));
        assert_eq!(parse_rgb("rgb(255,255,255)")?, RGB::new(255, 255, 255));
        assert_eq!(parse_rgb("rgb(255,0,255)")?, RGB::new(255, 0, 255));
        assert_eq!(parse_rgb("rgb(171,193,35)")?, RGB::new(171, 193, 35));
        assert_eq!(parse_rgb("rgb(0,17,34)")?, RGB::new(0, 17, 34));
        assert_eq!(parse_rgb("  rgb(255,0,0)  ")?, RGB::new(255, 0, 0));

        assert!(parse_rgb("rgb(0,0,256)").is_err());
        assert!(parse_rgb("rgb(0,0)").is_err());
        assert!(parse_rgb("rgb(0,0,0,0)").is_err());
        assert!(parse_rgb("rgb(0,0,0,0,0)").is_err());
        Ok(())
    }

    #[test]
    fn test_parse_rgb_rgb_colon_format() -> Result<()> {
        assert_eq!(parse_rgb("rgb:0000/0000/0000")?, RGB::new(0, 0, 0));
        assert_eq!(parse_rgb("rgb:ffff/0000/0000")?, RGB::new(255, 0, 0));
        assert_eq!(parse_rgb("rgb:0000/ffff/0000")?, RGB::new(0, 255, 0));
        assert_eq!(parse_rgb("rgb:0000/0000/ffff")?, RGB::new(0, 0, 255));
        assert_eq!(parse_rgb("rgb:ffff/ffff/ffff")?, RGB::new(255, 255, 255));
        assert_eq!(parse_rgb("rgb:ffff/0000/ffff")?, RGB::new(255, 0, 255));
        assert_eq!(parse_rgb("rgb:abcd/C1AB/230A")?, RGB::new(171, 193, 35));
        assert_eq!(parse_rgb("  rgb:00/11/22  ")?, RGB::new(0, 17, 34));
        assert_eq!(parse_rgb("rgb:ff00/0000/0000")?, RGB::new(254, 0, 0));
        assert_eq!(parse_rgb("rgb:1111/2222/3333/4444")?, RGB::new(17, 34, 51));
        assert_eq!(parse_rgb("rgba:1111/2222/3333/4444")?, RGB::new(17, 34, 51));

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
        assert!((luminance(RGB::new(0, 0, 0)) - 0.0).abs() < 0.001);
        assert!((luminance(RGB::new(255, 255, 255)) - 1.0).abs() < 0.001);
        // Test a mid-gray
        let mid_gray_lum = luminance(RGB::new(128, 128, 128));
        assert!(mid_gray_lum > 0.0 && mid_gray_lum < 1.0);
        // Test colors with different luminance contributions
        assert!((luminance(RGB::new(255, 0, 0)) - 0.2126).abs() < 0.001); // Red should have low luminance
        assert!((luminance(RGB::new(0, 255, 0)) - 0.7152).abs() < 0.001); // Green should have high luminance
        assert!((luminance(RGB::new(0, 0, 255)) - 0.0722).abs() < 0.001); // Blue should have very low luminance
        // Test edge cases with non-linear conversion
        assert!((luminance(RGB::new(0, 0, 0)) - 0.0).abs() < 0.001);
        assert!((luminance(RGB::new(255, 255, 255)) - 1.0).abs() < 0.001);
        // Test a subtle color difference that should be distinguishable
        let very_dark = luminance(RGB::new(1, 1, 1));
        let slightly_lighter = luminance(RGB::new(2, 2, 2));
        assert!(slightly_lighter > very_dark);
    }

    #[test]
    fn test_classify_color() {
        assert_eq!(classify_color(RGB::new(0, 0, 0)), "dark");
        assert_eq!(classify_color(RGB::new(255, 255, 255)), "light");
        assert_eq!(classify_color(RGB::new(128, 128, 128)), "dark"); // Mid-gray is below threshold
        assert_eq!(classify_color(RGB::new(200, 200, 200)), "light");
        assert_eq!(classify_color(RGB::new(50, 50, 50)), "dark");
    }

    #[test]
    fn test_rgb_struct() {
        let rgb = RGB::new(100, 150, 200);
        assert_eq!(rgb.r, 100);
        assert_eq!(rgb.g, 150);
        assert_eq!(rgb.b, 200);

        let rgb_from_tuple = RGB::from_tuple((255, 128, 0));
        assert_eq!(rgb_from_tuple, RGB::new(255, 128, 0));

        let tuple = rgb.to_tuple();
        assert_eq!(tuple, (100, 150, 200));
    }
}
