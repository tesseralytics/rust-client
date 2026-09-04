//! Synchronous Tessera API client.

#[cfg(feature = "polars")]
use polars::prelude::{DataFrame, LazyFrame};

use crate::base::{
    PreparedRequest, datasets_request, download_request, parse_retry_after, partitions_request,
    should_retry,
};
use crate::config::{ClientConfig, backoff_delay};
use crate::error::{TesseraError, error_from_response};
use crate::models::{
    DatasetsResponse, DownloadResponse, IntoCoins, IntoMonths, PartitionRef, PartitionsResponse,
};
#[cfg(any(feature = "polars", feature = "duckdb"))]
use crate::readers::ResolvedPartition;
use crate::readers::expand_refs;
#[cfg(any(feature = "polars", feature = "duckdb"))]
use crate::resolver::resolve_sync;

/// A synchronous client for the Tessera API.
///
/// Call [`TesseraClient::new`] with an explicit API key or rely on
/// `$TESSERA_API_KEY`.
///
/// # Panics
///
/// The constructor builds a private tokio runtime (needed for Polars cloud
/// reads), so a `TesseraClient` must not be constructed or used from inside
/// another async runtime — use [`crate::AsyncTesseraClient`] there instead.
pub struct TesseraClient {
    config: ClientConfig,
    http: reqwest::blocking::Client,
    #[cfg(feature = "polars")]
    runtime: tokio::runtime::Runtime,
}

impl TesseraClient {
    /// Create a client with defaults, resolving the key from `api_key` or
    /// `$TESSERA_API_KEY`.
    ///
    /// # Errors
    ///
    /// Returns [`TesseraError::Configuration`] when no API key can be resolved.
    pub fn new(api_key: Option<&str>) -> Result<Self, TesseraError> {
        Self::from_config(ClientConfig::new(api_key)?)
    }

    /// Create a client from a fully-specified [`ClientConfig`].
    ///
    /// # Errors
    ///
    /// Returns [`TesseraError::Network`] when the HTTP client or tokio runtime
    /// cannot be built.
    pub fn from_config(config: ClientConfig) -> Result<Self, TesseraError> {
        let http = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .default_headers(config.auth_headers())
            .timeout(config.timeout)
            .build()
            .map_err(|err| TesseraError::Network(err.to_string()))?;
        #[cfg(feature = "polars")]
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|err| TesseraError::Network(err.to_string()))?;
        Ok(Self {
            config,
            http,
            #[cfg(feature = "polars")]
            runtime,
        })
    }

    /// The resolved configuration this client was built with.
    #[must_use]
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Close the underlying HTTP connection pool.
    pub fn close(&mut self) {
        // reqwest clients tear their pools down on drop; nothing else to do.
    }

    /// Send a prepared request, retrying transient failures.
    fn request(
        &self,
        prepared: &PreparedRequest,
    ) -> Result<reqwest::blocking::Response, TesseraError> {
        for attempt in 0..=self.config.max_retries {
            let last = attempt == self.config.max_retries;
            let url = format!("{}{}", self.config.base_url, prepared.path);
            let sent = self.http.get(url).query(&prepared.params).send();
            match sent {
                Err(err) if err.is_connect() || err.is_timeout() || err.is_request() => {
                    if last {
                        return Err(TesseraError::Network(format!(
                            "network error contacting Tessera: {err}"
                        )));
                    }
                    std::thread::sleep(backoff_delay(attempt, None));
                }
                Ok(response) if !last && should_retry(response.status().as_u16()) => {
                    let retry_after = parse_retry_after(response.headers());
                    std::thread::sleep(backoff_delay(attempt, retry_after));
                }
                Ok(response) => {
                    if response.status().is_success() {
                        return Ok(response);
                    }
                    let status = response.status().as_u16();
                    let body = response
                        .bytes()
                        .map_err(|err| TesseraError::Network(err.to_string()))?;
                    return Err(error_from_response(status, &body));
                }
                Err(err) => {
                    return Err(TesseraError::Network(format!(
                        "network error contacting Tessera: {err}"
                    )));
                }
            }
        }
        unreachable!("retry loop always returns")
    }

    /// Parse a success response body as JSON.
    fn json<T: serde::de::DeserializeOwned>(
        response: reqwest::blocking::Response,
    ) -> Result<T, TesseraError> {
        let body = response
            .bytes()
            .map_err(|err| TesseraError::Network(err.to_string()))?;
        serde_json::from_slice(&body).map_err(|err| TesseraError::Network(err.to_string()))
    }

    /// List every dataset visible to your plan.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises.
    pub fn datasets(&self) -> Result<DatasetsResponse, TesseraError> {
        let response = self.request(&datasets_request())?;
        Self::json(response)
    }

    /// List the partitions of `asset`, optionally filtered by coin/month.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises.
    pub fn partitions(
        &self,
        asset: &str,
        coin: Option<&str>,
        month: Option<&str>,
    ) -> Result<PartitionsResponse, TesseraError> {
        let response = self.request(&partitions_request(asset, coin, month))?;
        Self::json(response)
    }

    /// Mint a short-lived presigned download URL for one partition.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises.
    pub fn download_url(
        &self,
        asset: &str,
        coin: &str,
        month: &str,
    ) -> Result<DownloadResponse, TesseraError> {
        let response = self.request(&download_request(asset, coin, month))?;
        Self::json(response)
    }

    /// Expand `(asset, coins, months)` into concrete partition references.
    ///
    /// # Errors
    ///
    /// Returns [`TesseraError::InvalidArgument`] on empty/invalid arguments.
    pub fn partition_refs(
        &self,
        asset: &str,
        coin: impl IntoCoins,
        month: impl IntoMonths,
    ) -> Result<Vec<PartitionRef>, TesseraError> {
        let _ = self;
        expand_refs(asset, coin, month)
    }

    /// Resolve presigned URLs for every ref, concurrently and order-preserving.
    #[cfg(any(feature = "polars", feature = "duckdb"))]
    fn resolve(&self, refs: &[PartitionRef]) -> Result<Vec<ResolvedPartition>, TesseraError> {
        resolve_sync(
            |partition| {
                self.download_url(&partition.asset, &partition.coin, &partition.month)
                    .map(|d| d.url)
            },
            refs,
        )
    }

    /// Lazily scan one or more partitions into a Polars `LazyFrame`.
    ///
    /// URLs are minted now but the data is read on `.collect()`. Because
    /// presigned URLs expire (~15 min), collect promptly; for long-lived
    /// graphs re-run `scan` to refresh.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises.
    #[cfg(feature = "polars")]
    pub fn scan(
        &self,
        asset: &str,
        coin: impl IntoCoins,
        month: impl IntoMonths,
        columns: Option<&[&str]>,
    ) -> Result<LazyFrame, TesseraError> {
        let parts = self.resolve(&expand_refs(asset, coin, month)?)?;
        crate::readers::polars::build_lazyframe(&parts, columns)
    }

    /// Eagerly read one or more partitions into a Polars `DataFrame`.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises, including
    /// [`TesseraError::PresignExpired`] for rejected presigned URLs.
    #[cfg(feature = "polars")]
    pub fn read(
        &self,
        asset: &str,
        coin: impl IntoCoins,
        month: impl IntoMonths,
        columns: Option<&[&str]>,
    ) -> Result<DataFrame, TesseraError> {
        let parts = self.resolve(&expand_refs(asset, coin, month)?)?;
        let lazy = crate::readers::polars::build_lazyframe(&parts, columns)?;
        self.runtime
            .block_on(async { crate::readers::polars::collect(lazy) })
    }

    /// Open one or more partitions as an in-memory DuckDB connection exposing
    /// a `tessera` view for SQL querying.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises.
    #[cfg(feature = "duckdb")]
    pub fn to_duckdb(
        &self,
        asset: &str,
        coin: impl IntoCoins,
        month: impl IntoMonths,
        columns: Option<&[&str]>,
    ) -> Result<duckdb::Connection, TesseraError> {
        let parts = self.resolve(&expand_refs(asset, coin, month)?)?;
        crate::readers::duckdb::build_relation(&parts, columns)
    }
}

impl Drop for TesseraClient {
    fn drop(&mut self) {
        self.close();
    }
}

impl std::fmt::Debug for TesseraClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the API key.
        f.debug_struct("TesseraClient")
            .field("base_url", &self.config.base_url)
            .field("timeout", &self.config.timeout)
            .field("max_retries", &self.config.max_retries)
            .finish_non_exhaustive()
    }
}
