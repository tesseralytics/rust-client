//! Load Tessera partitions into Polars frames.
//!
//! Reads happen directly over the presigned HTTPS URL via range requests —
//! only the Parquet footer and the row-groups/columns a query touches cross
//! the wire.

use polars::prelude::*;

use super::ResolvedPartition;
use crate::error::TesseraError;

/// Substrings that signal a presigned URL was rejected (typically expired).
const EXPIRY_MARKERS: [&str; 5] = [
    "403",
    "expired",
    "accessdenied",
    "access denied",
    "forbidden",
];

/// Build a (possibly concatenated) `LazyFrame` over the resolved partitions.
///
/// For multi-partition reads, a `coin` and `month` column identifying the
/// source partition are appended so rows stay attributable after concatenation.
///
/// # Errors
///
/// Returns [`TesseraError::Network`] when a Parquet path fails to register.
pub fn build_lazyframe(
    parts: &[ResolvedPartition],
    columns: Option<&[&str]>,
) -> Result<LazyFrame, TesseraError> {
    let multi = parts.len() > 1;
    let mut frames = Vec::with_capacity(parts.len());
    for (partition, url) in parts {
        let mut lazy = LazyFrame::scan_parquet(url.as_str().into(), ScanArgsParquet::default())
            .map_err(|err| TesseraError::Network(err.to_string()))?;
        if let Some(columns) = columns {
            lazy = lazy.select(
                columns
                    .iter()
                    .map(|column| col(*column))
                    .collect::<Vec<_>>(),
            );
        }
        if multi {
            lazy = lazy.with_columns(vec![
                lit(partition.coin.as_str()).alias("coin"),
                lit(partition.month.as_str()).alias("month"),
            ]);
        }
        frames.push(lazy);
    }
    if let [single] = &frames[..] {
        return Ok(single.clone());
    }
    concat(
        frames,
        UnionArgs {
            rechunk: true,
            parallel: true,
            to_supertypes: true,
            ..Default::default()
        },
    )
    .map_err(|err| TesseraError::Network(err.to_string()))
}

/// Collect a lazy frame, translating presign-expiry failures into a clear error.
///
/// Non-expiry Polars failures surface as [`TesseraError::Network`].
///
/// # Errors
///
/// Returns [`TesseraError::PresignExpired`] when the failure looks like a
/// rejected presigned URL; other failures map to [`TesseraError::Network`].
pub fn collect(lazy: LazyFrame) -> Result<DataFrame, TesseraError> {
    lazy.collect().map_err(|err| {
        let message = err.to_string().to_lowercase();
        if EXPIRY_MARKERS.iter().any(|marker| message.contains(marker)) {
            TesseraError::PresignExpired
        } else {
            TesseraError::Network(err.to_string())
        }
    })
}
