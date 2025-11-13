//! Global panic handler for graceful shutdown
//!
//! Provides a panic hook that:
//! - Logs panic location and message
//! - Attempts to flush logs
//! - Allows cleanup before process termination
//!
//! # Usage
//!
//! Call `install_panic_handler()` early in main():
//!
//! ```no_run
//! use bog_core::resilience::install_panic_handler;
//!
//! fn main() {
//!     install_panic_handler();
//!     // ... rest of application
//! }
//! ```

use std::panic;
use std::process;
use tracing::error;

/// Install a global panic handler that attempts graceful shutdown
///
/// This panic handler:
/// 1. Logs the panic location and message using tracing
/// 2. Attempts to flush tracing subscribers
/// 3. Exits with non-zero status code
///
/// # Note
///
/// This does NOT catch panics - it only provides better logging when they occur.
/// The process will still terminate after a panic.
pub fn install_panic_handler() {
    // Store the default panic hook for delegation
    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        // Extract panic location
        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "<unknown location>".to_string());

        // Extract panic message
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "<no message>".to_string()
        };

        // Log the panic using tracing (will be captured by all subscribers)
        error!(
            location = %location,
            message = %message,
            "PANIC: Bot crashed. Attempting graceful shutdown."
        );

        // Also print to stderr as a backup (in case tracing is misconfigured)
        eprintln!("═══════════════════════════════════════════════════════════");
        eprintln!("FATAL PANIC: Trading bot crashed");
        eprintln!("Location: {}", location);
        eprintln!("Message:  {}", message);
        eprintln!("═══════════════════════════════════════════════════════════");

        // Call the default panic hook (prints full backtrace if RUST_BACKTRACE=1)
        default_hook(panic_info);

        // Give tracing time to flush logs
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Exit with error code (don't rely on default panic behavior)
        process::exit(1);
    }));

    tracing::info!("Panic handler installed - panics will be logged before shutdown");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Only run manually - this test panics by design
    fn test_panic_handler() {
        install_panic_handler();
        panic!("Test panic - should be logged gracefully");
    }

    #[test]
    fn test_panic_handler_installation() {
        // Just verify we can install without crashing
        install_panic_handler();
        // Install again - should work (replaces previous hook)
        install_panic_handler();
    }
}
