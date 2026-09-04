<div align="center">

# tessera-api

**Clean Hyperliquid market data — straight into Polars & DuckDB.**

[![Crates.io](https://img.shields.io/crates/v/tessera-api.svg)](https://crates.io/crates/tessera-api)
[![docs.rs](https://img.shields.io/docsrs/tessera-api)](https://docs.rs/tessera-api)
[![CI](https://github.com/tesseralytics/rust-client/actions/workflows/ci.yml/badge.svg)](https://github.com/tesseralytics/rust-client/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

The official Rust client for [**Tessera**](https://tesseralytics.dev) — order-flow-enriched
OHLCV, funding-rate, and positioning datasets built from raw Hyperliquid trade data,
delivered as Parquet over a REST API.

</div>

---

`tessera-api` is the Rust companion to the Python SDK ([`tessera-api` on PyPI](https://pypi.org/project/tessera-api/),
import name `tessera`). The goal: point it at a `(dataset, coin, month)` and get Parquet-backed
data into [Polars](https://pypi.org/project/polars) / [DuckDB](https://duckdb.org) — read
straight from object storage over range requests, with predicate and projection pushdown.
No temp files, no glue code.

## Status

**Functional.** The client surface mirrors the Python SDK:

```rust,no_run
fn main() -> Result<(), tessera::TesseraError> {
    let client = tessera::TesseraClient::new(None)?;
    // Pick the newest available month
    let latest = client
        .partitions("gold_ohlcv_1m", Some("BTC"), None)?
        .partitions
        .pop()
        .expect("at least one partition");
    let df = client.read("gold_ohlcv_1m", "BTC", &latest.month, None)?;
    println!("{} rows for {}", df.height(), latest.month);
    Ok(())
}
```


- Sync `TesseraClient` and async `AsyncTesseraClient` mirroring the Python SDK
- Response models generated from the vendored OpenAPI spec at build time
- `TesseraError` taxonomy: `Configuration`, `NotFound`, `PresignExpired`, …
- Catalog helpers: `DatasetSummary`, `MonthSpan`, `PartitionRef`, …
- Polars reader (default feature) and DuckDB reader (`duckdb` feature) over
  presigned Parquet URLs

## Install

Once published:

```bash
cargo add tessera-api
```

Grab a free API key (no card required) at **[tesseralytics.dev](https://tesseralytics.dev)**.

## Documentation

- 📚 Python SDK (full docs today): <https://tesseralytics.dev/python-client>
- 🦀 Rust docs: <https://tesseralytics.dev/rust-client>
- 🦀 docs.rs: <https://docs.rs/tessera-api>
- 🌐 **Product & pricing:** <https://tesseralytics.dev>

## License

GPL-3.0 © Tessera. See [LICENSE](LICENSE).
