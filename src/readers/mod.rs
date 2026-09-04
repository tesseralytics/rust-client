//! Data-engine plumbing shared by the Polars and DuckDB readers.

#[cfg(feature = "duckdb")]
pub mod duckdb;
#[cfg(feature = "polars")]
pub mod polars;

use crate::error::TesseraError;
use crate::models::{IntoCoins, IntoMonths, PartitionRef};
#[cfg(any(feature = "polars", feature = "duckdb"))]
/// A resolved partition: its reference plus a freshly-minted presigned URL.
pub type ResolvedPartition = (PartitionRef, String);

/// Expand `(asset, coins, months)` into the cartesian list of partitions.
///
/// Coin-major ordering: every month of the first coin, then the second, …
///
/// # Errors
///
/// Returns [`TesseraError::InvalidArgument`] when either argument is empty or
/// contains an invalid month.
pub fn expand_refs(
    asset: &str,
    coin: impl IntoCoins,
    month: impl IntoMonths,
) -> Result<Vec<PartitionRef>, TesseraError> {
    let coins = coin.into_coins()?;
    let months = month.into_months()?;
    let mut refs = Vec::with_capacity(coins.len() * months.len());
    for coin in &coins {
        for month in &months {
            refs.push(PartitionRef::new(asset, coin, month)?);
        }
    }
    Ok(refs)
}
