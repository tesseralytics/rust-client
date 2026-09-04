//! # tessera
//!
//! The official Rust client for [Tessera](https://tesseralytics.dev) — order-flow-enriched
//! OHLCV, funding-rate, and positioning datasets built from raw Hyperliquid trade data,
//! delivered as Parquet over a REST API.
//!
//! This crate is the Rust companion to the
//! [`tessera-api`](https://pypi.org/project/tessera-api/) Python SDK. It is under active
//! development: the full client surface (async `TesseraClient`, the error taxonomy, and
//! Polars/DuckDB readers) lands with the first release.
//!
//! ## Current surface
//!
//! ```
//! assert_eq!(tessera::hello(), "hello from tessera");
//! ```

/// The crate version, as published on crates.io.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the crate's hello-world greeting.
#[must_use]
pub fn hello() -> &'static str {
    "hello from tessera"
}
