//! # tessera
// Brand names read fine without markup; keep the prose clean.
#![allow(clippy::doc_markdown)]
//!
//! The official Rust client for [Tessera](https://tesseralytics.dev) —
//! order-flow-enriched OHLCV, funding-rate, and positioning datasets built
//! from raw Hyperliquid trade data, delivered as Parquet over a REST API.
//!
//! This crate is the Rust mirror of the
//! [`tessera-api`](https://pypi.org/project/tessera-api/) Python SDK: point it
//! at a `(dataset, coin, month)` and read straight from object storage into
//! [Polars](https://pola.rs) or [DuckDB](https://duckdb.org) over presigned
//! URLs — with predicate/projection pushdown, no temp files.
//!
//! ## Choosing a client
//!
//! - [`TesseraClient`] — blocking API. Owns a private tokio runtime, so do not
//!   construct it from inside an async runtime.
//! - [`AsyncTesseraClient`] — `async fn` surface for tokio applications.
//!
//! ## Data engines
//!
//! - Polars (default feature `polars`): [`TesseraClient::scan`] returns a
//!   `LazyFrame`; [`TesseraClient::read`] collects a `DataFrame`.
//! - DuckDB (opt-in feature `duckdb`): `TesseraClient::to_duckdb` returns an
//!   in-memory connection exposing a `tessera` view.
//!
//! ## Errors
//!
//! Every failure raises [`TesseraError`], mirroring the Python exception
//! taxonomy (`Configuration`, `NotFound`, `PresignExpired`, …).
//!
//! ## Example
//!
//! ```no_run
//! # fn main() -> Result<(), tessera::TesseraError> {
//! let client = tessera::TesseraClient::new(None)?;
//! // Pick the newest available month rather than a hardcoded one:
//! // partitions roll on a 12-month window, so fixed dates go stale.
//! let latest = client
//!     .partitions("gold_ohlcv_1m", Some("BTC"), None)?
//!     .partitions
//!     .pop()
//!     .expect("at least one partition");
//! let frame = client.read("gold_ohlcv_1m", "BTC", &latest.month, None)?;
//! println!("{} rows for {}", frame.height(), latest.month);
//! # Ok::<(), tessera::TesseraError>(())
//! # }
//! ```

/// The crate version, as published on crates.io.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod async_client;
mod base;
mod client;
mod config;
mod error;
mod models;
/// Data-engine plumbing: Parquet readers for Polars and DuckDB.
///
/// Most users go through [`TesseraClient::scan`] / [`TesseraClient::read`] /
/// `TesseraClient::to_duckdb`; the readers are public for advanced
/// composition over pre-resolved URLs.
pub mod readers;
#[cfg(any(feature = "polars", feature = "duckdb"))]
pub mod resolver;

pub use crate::async_client::AsyncTesseraClient;
pub use crate::client::TesseraClient;
pub use crate::config::{API_KEY_ENV_VAR, ClientConfig, DEFAULT_BASE_URL, USER_AGENT};
pub use crate::error::{TesseraError, error_from_response};
pub use crate::models::{
    DatasetSummary, DatasetsResponse, DownloadResponse, ErrorBody, IntoCoins, IntoMonths,
    MonthRange, MonthSpan, Partition, PartitionRef, PartitionsResponse,
};
