//! Logging and debug utilities.
//!
//! This module provides helpers for development and troubleshooting support.

/// Print debug messages to stderr if DEBUG environment variable is set.
///
/// This macro checks for the presence of the `DEBUG` environment variable
/// and only outputs the message if it's set. This allows for conditional
/// debug output without performance overhead in production builds.
///
/// See the formatting documentation in [`std::fmt`](std::fmt)
/// for details of the macro argument syntax.
///
/// # Examples
///
/// ```
/// # use lumos::debug;
/// debug!("Query response received");
/// debug!("Parsed RGB: {:?}", (255, 128, 0));
/// ```
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if std::env::var("DEBUG").is_ok() {
            eprintln!($($arg)*);
        }
    };
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_debug_functions_dont_panic() {
        // These should not panic regardless of DEBUG setting
        debug!("Test debug message");
        debug!("Test {} message", "debug");
    }
}
