//! Public data models.
//!
//! Response models ([`DatasetsResponse`], [`PartitionsResponse`], …) are
//! generated from the vendored OpenAPI spec at build time (`build.rs` — the
//! generated code lives in `$OUT_DIR`, so it can never drift from the spec).
//! This module re-exports them alongside hand-written ergonomic helpers
//! ([`PartitionRef`], [`MonthSpan`]).

/// Generated OpenAPI response models, exempted from lint scrutiny.
#[allow(clippy::all, clippy::pedantic)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/tessera_generated.rs"));
}

pub use generated::{
    DatasetSummary, DatasetsResponse, DownloadResponse, ErrorBody, MonthRange, Partition,
    PartitionsResponse,
};

use crate::error::TesseraError;

/// Validate a `YYYY-MM` month string.
///
/// Matches the Python SDK's `^\d{4}-(0[1-9]|1[0-2])$` regex: a four-digit
/// year, a hyphen, and a month in `01..=12`.
pub(crate) fn validate_month(month: &str) -> Result<(), TesseraError> {
    let digits = |bytes: &[u8]| bytes.iter().all(u8::is_ascii_digit);
    let valid = month.len() == 7
        && month.as_bytes()[4] == b'-'
        && digits(&month.as_bytes()[0..4])
        && digits(&month.as_bytes()[5..7]);
    if !valid {
        return Err(TesseraError::InvalidArgument(format!(
            "month must be in YYYY-MM format, got {month:?}"
        )));
    }
    let m: u8 = month[5..7].parse().expect("digits checked above");
    if !(1..=12).contains(&m) {
        return Err(TesseraError::InvalidArgument(format!(
            "month must be in YYYY-MM format, got {month:?}"
        )));
    }
    Ok(())
}

/// A fully-qualified reference to a single partition.
///
/// A partition is one `(asset, coin, month)` Parquet object, e.g.
/// `gold_ohlcv_1m` / `BTC` / `2025-09`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PartitionRef {
    /// Dataset name, e.g. `gold_ohlcv_1m`.
    pub asset: String,
    /// Coin symbol, e.g. `BTC`.
    pub coin: String,
    /// Partition month, `YYYY-MM`.
    pub month: String,
}

impl PartitionRef {
    /// Create a reference, validating the month format.
    ///
    /// # Errors
    ///
    /// Returns [`TesseraError::InvalidArgument`] when `month` is not `YYYY-MM`.
    pub fn new(
        asset: impl Into<String>,
        coin: impl Into<String>,
        month: impl Into<String>,
    ) -> Result<Self, TesseraError> {
        let month = month.into();
        validate_month(&month)?;
        Ok(Self {
            asset: asset.into(),
            coin: coin.into(),
            month,
        })
    }

    /// The object-storage key layout: `{asset}/coin={COIN}/month={YYYY-MM}.parquet`.
    #[must_use]
    pub fn object_key(&self) -> String {
        format!(
            "{}/coin={}/month={}.parquet",
            self.asset, self.coin, self.month
        )
    }
}

/// An inclusive range of months, e.g. `MonthSpan::new("2025-01", "2025-09")?`.
///
/// Pass it anywhere a `month` argument is accepted to expand to every month
/// in the range (inclusive of both endpoints).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthSpan {
    /// First month in the span, inclusive.
    pub start: String,
    /// Last month in the span, inclusive.
    pub end: String,
}

impl MonthSpan {
    /// Create a span, validating both months and their ordering.
    ///
    /// # Errors
    ///
    /// Returns [`TesseraError::InvalidArgument`] when either endpoint is not
    /// `YYYY-MM`, or when `start` is after `end`.
    pub fn new(start: impl Into<String>, end: impl Into<String>) -> Result<Self, TesseraError> {
        let start = start.into();
        let end = end.into();
        validate_month(&start)?;
        validate_month(&end)?;
        if start > end {
            return Err(TesseraError::InvalidArgument(format!(
                "MonthSpan start {start:?} is after end {end:?}"
            )));
        }
        Ok(Self { start, end })
    }

    /// Return the months in the span as a list of `YYYY-MM` strings.
    #[must_use]
    pub fn months(&self) -> Vec<String> {
        let (mut year, mut month) = parse_ym(&self.start);
        let (end_year, end_month) = parse_ym(&self.end);
        let mut months = Vec::new();
        while (year, month) <= (end_year, end_month) {
            months.push(format!("{year:04}-{month:02}"));
            month += 1;
            if month > 12 {
                month = 1;
                year += 1;
            }
        }
        months
    }
}

impl IntoIterator for MonthSpan {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;

    /// Iterate the span's months (inclusive, crossing year boundaries).
    fn into_iter(self) -> Self::IntoIter {
        self.months().into_iter()
    }
}

/// Parse `YYYY-MM` into `(year, month)` integers.
///
/// Callers must have run [`validate_month`] first; numeric parsing is
/// infallible on validated input.
fn parse_ym(month: &str) -> (i32, i32) {
    (
        month[0..4]
            .parse()
            .expect("validated month has a numeric year"),
        month[5..7]
            .parse()
            .expect("validated month has a numeric month"),
    )
}

/// Accepted shapes for a `coin` argument: one symbol or any iterable of them.
pub trait IntoCoins {
    /// Coerce into a non-empty list of coin symbols.
    ///
    /// # Errors
    ///
    /// Returns [`TesseraError::InvalidArgument`] when the result is empty.
    fn into_coins(self) -> Result<Vec<String>, TesseraError>;
}

/// Accepted shapes for a `month` argument: one month, a [`MonthSpan`], or any
/// iterable of months.
pub trait IntoMonths {
    /// Coerce into a non-empty, validated list of `YYYY-MM` strings.
    ///
    /// # Errors
    ///
    /// Returns [`TesseraError::InvalidArgument`] when the result is empty or
    /// any entry is not in `YYYY-MM` format.
    fn into_months(self) -> Result<Vec<String>, TesseraError>;
}

impl IntoCoins for &str {
    fn into_coins(self) -> Result<Vec<String>, TesseraError> {
        Ok(vec![self.to_string()])
    }
}

impl IntoCoins for String {
    fn into_coins(self) -> Result<Vec<String>, TesseraError> {
        Ok(vec![self])
    }
}

impl IntoCoins for &String {
    fn into_coins(self) -> Result<Vec<String>, TesseraError> {
        self.as_str().into_coins()
    }
}

impl<C: AsRef<str>> IntoCoins for Vec<C> {
    fn into_coins(self) -> Result<Vec<String>, TesseraError> {
        self.as_slice().into_coins()
    }
}

impl<C: AsRef<str>> IntoCoins for &Vec<C> {
    fn into_coins(self) -> Result<Vec<String>, TesseraError> {
        self.as_slice().into_coins()
    }
}

impl<C: AsRef<str>, const N: usize> IntoCoins for [C; N] {
    fn into_coins(self) -> Result<Vec<String>, TesseraError> {
        self.as_slice().into_coins()
    }
}

impl<C: AsRef<str>, const N: usize> IntoCoins for &[C; N] {
    fn into_coins(self) -> Result<Vec<String>, TesseraError> {
        self.as_slice().into_coins()
    }
}

impl IntoMonths for &str {
    fn into_months(self) -> Result<Vec<String>, TesseraError> {
        validate_month(self)?;
        Ok(vec![self.to_string()])
    }
}

impl IntoMonths for String {
    fn into_months(self) -> Result<Vec<String>, TesseraError> {
        validate_month(&self)?;
        Ok(vec![self])
    }
}

impl IntoMonths for &String {
    fn into_months(self) -> Result<Vec<String>, TesseraError> {
        self.as_str().into_months()
    }
}

impl IntoMonths for MonthSpan {
    fn into_months(self) -> Result<Vec<String>, TesseraError> {
        Ok(self.months())
    }
}

impl<C: AsRef<str>> IntoMonths for &[C] {
    fn into_months(self) -> Result<Vec<String>, TesseraError> {
        let months: Vec<String> = self.iter().map(|m| m.as_ref().to_string()).collect();
        if months.is_empty() {
            return Err(TesseraError::InvalidArgument(
                "at least one month is required".to_string(),
            ));
        }
        for m in &months {
            validate_month(m)?;
        }
        Ok(months)
    }
}

impl<C: AsRef<str>> IntoMonths for Vec<C> {
    fn into_months(self) -> Result<Vec<String>, TesseraError> {
        self.as_slice().into_months()
    }
}

impl<C: AsRef<str>> IntoMonths for &Vec<C> {
    fn into_months(self) -> Result<Vec<String>, TesseraError> {
        self.as_slice().into_months()
    }
}

impl<C: AsRef<str>, const N: usize> IntoMonths for [C; N] {
    fn into_months(self) -> Result<Vec<String>, TesseraError> {
        self.as_slice().into_months()
    }
}

impl<C: AsRef<str>, const N: usize> IntoMonths for &[C; N] {
    fn into_months(self) -> Result<Vec<String>, TesseraError> {
        self.as_slice().into_months()
    }
}

impl<C: AsRef<str>> IntoCoins for &[C] {
    fn into_coins(self) -> Result<Vec<String>, TesseraError> {
        let coins: Vec<String> = self.iter().map(|c| c.as_ref().to_string()).collect();
        if coins.is_empty() {
            return Err(TesseraError::InvalidArgument(
                "at least one coin is required".to_string(),
            ));
        }
        Ok(coins)
    }
}
