use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize tracing logger
pub fn init_logger(log_level: &str, json_logs: bool) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

    if json_logs {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().with_target(true).with_thread_ids(true))
            .init();
    }
}
