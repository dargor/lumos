//! Logging and debug utilities.
//!
//! This module provides helpers for development and troubleshooting support.

use std::sync::LazyLock;
use std::sync::atomic::AtomicBool;

/// A flag indicating whether debug logging is enabled.
/// This is determined by the presence of the `DEBUG` environment variable.
pub static DEBUG_ENABLED: LazyLock<AtomicBool> =
    LazyLock::new(|| AtomicBool::new(std::env::var("DEBUG").is_ok()));

/// Print debug messages to stderr if `DEBUG` environment variable is set.
///
/// See the formatting documentation in [`std::fmt`](std::fmt)
/// for more details on the macro argument syntax.
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
        if $crate::logs::DEBUG_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
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
