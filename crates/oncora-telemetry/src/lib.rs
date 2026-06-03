//! # oncora-telemetry
//!
//! Structured run traces for the platform. In production this wires
//! `tracing-opentelemetry` to an OpenTelemetry collector (see
//! `docs/08-roadmap.md`); here we provide a simple env-filtered subscriber so
//! every crate can emit spans/events consistently.

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Initialise global tracing once. Honors `RUST_LOG`; defaults to `info`.
///
/// Safe to call multiple times — a second call is a no-op rather than a panic.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .try_init();
}
