//! Logging and debug utilities.
//!
//! This module provides functions for development and troubleshooting support.

use std::env;

/// Print debug messages to stderr if DEBUG environment variable is set.
///
/// This function checks for the presence of the `DEBUG` environment variable
/// and only outputs the message if it's set. This allows for conditional
/// debug output without performance overhead in production builds.
///
/// # Arguments
///
/// * `message` - The debug message to print
///
/// # Examples
///
/// ```
/// # use lumos::debug;
/// debug("Query response received");
/// debug(&format!("Parsed RGB: {:?}", (255, 128, 0)));
/// ```
pub fn debug(message: &str) {
    if env::var("DEBUG").is_ok() {
        eprintln!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_functions_dont_panic() {
        // These should not panic regardless of DEBUG setting
        debug("Test debug message");
    }
}
