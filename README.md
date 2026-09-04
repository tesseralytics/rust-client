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

**Under active development.** This repository currently holds the crate skeleton; the client
surface lands with the first crates.io release.

```rust
fn main() {
    println!("{}, v{}", tessera::hello(), tessera::VERSION);
}
```

## Roadmap

- Async `TesseraClient` / `AsyncTesseraClient` mirroring the Python SDK
- The `TesseraError` error taxonomy (`NotFoundError`, `PresignExpiredError`, …)
- Catalog models: `DatasetSummary`, `MonthSpan`, `Partition`, …
- Polars & DuckDB readers over presigned Parquet URLs

## Install

Once published:

```bash
cargo add tessera-api
```

Grab a free API key (no card required) at **[tesseralytics.dev](https://tesseralytics.dev)**.

## Documentation

- 📚 Python SDK (full docs today): <https://tesseralytics.dev/python-client>
- 🦀 docs.rs: <https://docs.rs/tessera-api>
- 🌐 **Product & pricing:** <https://tesseralytics.dev>

## License

GPL-3.0 © Tessera. See [LICENSE](LICENSE).
