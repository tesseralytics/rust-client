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

## Why tessera-api

- **Read where the data lives.** Partitioned Parquet is read directly from object storage
  over HTTPS range requests. Predicate and projection pushdown mean only the row groups and
  columns your query touches cross the wire — no temp files, no glue code.
- **Two engines, one API.** Lazily into Polars `LazyFrame`s (default feature), or into an
  in-memory DuckDB database you query with plain SQL (`duckdb` feature).
- **Sync and async.** `TesseraClient` for scripts and batch jobs; `AsyncTesseraClient` for
  tokio services.
- **Typed end to end.** Response models are generated from the OpenAPI spec at build time,
  every failure is a `TesseraError` variant, and transient failures (429/5xx) are retried
  with exponential backoff and `Retry-After` handling.

## Installation

```bash
cargo add tessera-api
```

Requires Rust 1.85+. Grab a free API key (no card required) at
**[tesseralytics.dev](https://tesseralytics.dev)**:

```bash
export TESSERA_API_KEY="ts_..."
```

The free tier covers BTC, ETH, SOL and HYPE for the trailing month.

## Quickstart

```rust,no_run
use tessera::TesseraClient;

fn main() -> Result<(), tessera::TesseraError> {
    let client = TesseraClient::new(None)?; // reads $TESSERA_API_KEY

    // Partitions roll on a trailing 12-month window, so discover the newest
    // available month instead of hardcoding a date:
    let latest = client
        .partitions("gold_ohlcv_1m", Some("BTC"), None)?
        .partitions
        .pop()
        .expect("at least one partition");

    let df = client.read("gold_ohlcv_1m", "BTC", &latest.month, None)?;
    println!("{} rows for {}", df.height(), latest.month);
    println!("{}", df.head(Some(5)));
    Ok(())
}
```

## Lazy scans

`scan` mints presigned URLs and returns a `LazyFrame`; the Parquet bytes are only read
when you `.collect()`. Filters and column selections push down to object storage:

```rust,no_run
use polars::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = tessera::TesseraClient::new(None)?;

    let lf = client.scan(
        "gold_ohlcv_1m",
        "BTC",
        "2026-08",
        Some(&["time", "close", "volume", "buy_vol", "sell_vol"]),
    )?;

    // Daily bars from 1-minute data — filters and projections are pushed down
    // to object storage; only the columns touched cross the wire.
    let daily = lf
        .filter(col("close").gt(lit(50_000.0)))
        .group_by([col("time").dt().date().alias("day")])
        .agg([
            col("close").last().alias("close"),
            col("volume").sum().alias("volume"),
            (col("buy_vol").sum() / col("volume").sum()).alias("buy_share"),
        ])
        .sort(["day"], SortMultipleOptions::default())
        .collect()?;
    println!("{daily}");
    Ok(())
}
```

Presigned URLs are short-lived (~15 minutes). Collect promptly; for long-lived graphs,
re-run `scan` to mint fresh URLs.

## Multiple coins and months

`coin` and `month` arguments accept a single value or any collection; months also accept
an inclusive `MonthSpan`. Multi-partition reads append `coin`/`month` columns so rows stay
attributable to their source partition:

```rust,no_run
use tessera::{MonthSpan, TesseraClient};

fn main() -> Result<(), tessera::TesseraError> {
    let client = TesseraClient::new(None)?;

    let df = client.read(
        "gold_ohlcv_1m",
        &["BTC", "ETH", "SOL"],                 // any IntoCoins: one symbol, a slice, a Vec, …
        MonthSpan::new("2026-07", "2026-08")?,  // any IntoMonths: one month, a span, a list, …
        Some(&["time", "open", "high", "low", "close", "volume"]),
    )?;
    println!("{}", df.head(Some(5)));
    Ok(())
}
```

Months outside the trailing 12-month window are no longer served (`TesseraError::NotFound`)
— call `partitions()` to discover what's available, as in the quickstart.

## Async

Inside a tokio runtime, use `AsyncTesseraClient` (the sync client owns a private runtime
and must not be constructed from async code):

```rust,no_run
use tessera::AsyncTesseraClient;

#[tokio::main]
async fn main() -> Result<(), tessera::TesseraError> {
    let client = AsyncTesseraClient::new(None)?;

    let df = client
        .read("gold_ohlcv_1m", &["BTC", "ETH"], "2026-08", None)
        .await?;
    println!("{} rows", df.height());
    Ok(())
}
```

## SQL with DuckDB

Opt in to the `duckdb` feature (drop the default `polars` feature if you don't need both):

```toml
tessera-api = { version = "0.1", default-features = false, features = ["duckdb"] }
```

`to_duckdb` returns an in-memory connection exposing a `tessera` view over the presigned
Parquet; filters and aggregations push down to object storage via `httpfs`:

```rust,no_run
use tessera::TesseraClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = TesseraClient::new(None)?;
    let conn = client.to_duckdb(
        "gold_ohlcv_1m",
        &["BTC", "ETH"],
        "2026-08",
        Some(&["time", "close", "volume"]),
    )?;

    let mut stmt =
        conn.prepare("SELECT coin, count(*), round(avg(close), 2) FROM tessera GROUP BY coin")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;
    for row in rows {
        let (coin, n, avg_close) = row?;
        println!("{coin}: {n} bars, avg close {avg_close}");
    }
    Ok(())
}
```

## Configuration

```rust,no_run
use std::time::Duration;

use tessera::{ClientConfig, TesseraClient};

fn main() -> Result<(), tessera::TesseraError> {
    let mut config = ClientConfig::new(None)?; // API key from $TESSERA_API_KEY
    config.timeout = Duration::from_secs(60);
    config.max_retries = 5;

    let client = TesseraClient::from_config(config)?;
    let _ = client.datasets()?;
    Ok(())
}
```

`ClientConfig` also exposes `base_url` and `user_agent`.

## Error handling

Every failure is a `TesseraError`; match on the variants you care about (or use the
`status_code()` / `code()` helpers):

```rust,no_run
use tessera::{TesseraClient, TesseraError};

fn main() -> Result<(), TesseraError> {
    let client = TesseraClient::new(None)?;

    // Months outside the trailing 12-month window 404 → NotFound:
    match client.read("gold_ohlcv_1m", "BTC", "2024-01", None) {
        Ok(df) => println!("{}", df.head(Some(5))),
        Err(TesseraError::NotFound { message, .. }) => {
            eprintln!("no such partition: {message}");
        }
        Err(TesseraError::PresignExpired) => {
            eprintln!("presigned URL expired — call read()/scan() again for fresh ones");
        }
        Err(err) => return Err(err),
    }
    Ok(())
}
```

## The raw API

The typed endpoints are also available directly — for browsing the catalog or fetching the
Parquet bytes yourself:

```rust,no_run
fn main() -> Result<(), tessera::TesseraError> {
    let client = tessera::TesseraClient::new(None)?;

    // Every dataset visible to your plan: name, coins, month coverage, partition count.
    for ds in &client.datasets()?.datasets {
        let earliest = ds.months.earliest.as_deref().unwrap_or("?");
        let latest = ds.months.latest.as_deref().unwrap_or("?");
        println!("{} · {} coins · {earliest}…{latest}", ds.name, ds.coins.len());
    }

    // Per-partition detail for one dataset: month, row count, on-disk size, …
    for p in &client.partitions("gold_ohlcv_1m", Some("BTC"), None)?.partitions {
        let rows = p.rows.map_or_else(|| "-".to_string(), |r| r.to_string());
        println!("{} · {rows} rows · {} MiB", p.month, p.size_bytes / (1024 * 1024));
    }

    // Presigned download URL for a single partition.
    let download = client.download_url("gold_ohlcv_1m", "BTC", "2026-08")?;
    println!("{} (expires at {})", download.url, download.expires_at);
    Ok(())
}
```

## Documentation

- 🦀 Rust guide: <https://tesseralytics.dev/rust-client>
- 🦀 API reference: <https://docs.rs/tessera-api>
- 🌐 Product & pricing: <https://tesseralytics.dev>

## License

GPL-3.0 © Tessera. See [LICENSE](LICENSE).
