//! Asyncio-native Tessera API client (`async fn` surface).

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
use crate::resolver::resolve_async;

/// An async client for the Tessera API.
///
/// Mirror of [`crate::TesseraClient`] with `async fn`; use it inside tokio
/// runtimes (including when the sync client would panic).
pub struct AsyncTesseraClient {
    config: ClientConfig,
    http: reqwest::Client,
}

impl AsyncTesseraClient {
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
    /// Returns [`TesseraError::Network`] when the HTTP client cannot be built.
    pub fn from_config(config: ClientConfig) -> Result<Self, TesseraError> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .default_headers(config.auth_headers())
            .timeout(config.timeout)
            .build()
            .map_err(|err| TesseraError::Network(err.to_string()))?;
        Ok(Self { config, http })
    }

    /// The resolved configuration this client was built with.
    #[must_use]
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Close the client.
    ///
    /// `reqwest::Client` tears its pool down on drop; taking the client out
    /// eagerly releases the handle for parity with the Python `aclose()`.
    pub fn close(&mut self) {
        self.http = reqwest::Client::new();
    }

    /// Send a prepared request, retrying transient failures.
    async fn request(&self, prepared: &PreparedRequest) -> Result<reqwest::Response, TesseraError> {
        send_with_retries(
            &self.http,
            &self.config.base_url,
            self.config.max_retries,
            prepared,
        )
        .await
    }

    /// Parse a success response body as JSON.
    async fn json<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, TesseraError> {
        parse_json(response).await
    }

    /// List every dataset visible to your plan.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises.
    pub async fn datasets(&self) -> Result<DatasetsResponse, TesseraError> {
        let response = self.request(&datasets_request()).await?;
        self.json(response).await
    }

    /// List the partitions of `asset`, optionally filtered by coin/month.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises.
    pub async fn partitions(
        &self,
        asset: &str,
        coin: Option<&str>,
        month: Option<&str>,
    ) -> Result<PartitionsResponse, TesseraError> {
        let response = self
            .request(&partitions_request(asset, coin, month))
            .await?;
        self.json(response).await
    }

    /// Mint a short-lived presigned download URL for one partition.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises.
    pub async fn download_url(
        &self,
        asset: &str,
        coin: &str,
        month: &str,
    ) -> Result<DownloadResponse, TesseraError> {
        let response = self.request(&download_request(asset, coin, month)).await?;
        self.json(response).await
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
    async fn resolve(&self, refs: &[PartitionRef]) -> Result<Vec<ResolvedPartition>, TesseraError> {
        let http = self.http.clone();
        let base_url = self.config.base_url.clone();
        let max_retries = self.config.max_retries;
        resolve_async(
            move |partition| {
                let http = http.clone();
                let base_url = base_url.clone();
                async move {
                    let prepared =
                        download_request(&partition.asset, &partition.coin, &partition.month);
                    let response =
                        send_with_retries(&http, &base_url, max_retries, &prepared).await?;
                    parse_json(response).await.map(|d: DownloadResponse| d.url)
                }
            },
            refs,
        )
        .await
    }

    /// Lazily scan one or more partitions into a Polars `LazyFrame`.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises.
    #[cfg(feature = "polars")]
    pub async fn scan(
        &self,
        asset: &str,
        coin: impl IntoCoins,
        month: impl IntoMonths,
        columns: Option<&[&str]>,
    ) -> Result<LazyFrame, TesseraError> {
        let parts = self.resolve(&expand_refs(asset, coin, month)?).await?;
        crate::readers::polars::build_lazyframe(&parts, columns)
    }

    /// Eagerly read one or more partitions into a Polars `DataFrame`.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises, including
    /// [`TesseraError::PresignExpired`] for rejected presigned URLs.
    #[cfg(feature = "polars")]
    pub async fn read(
        &self,
        asset: &str,
        coin: impl IntoCoins,
        month: impl IntoMonths,
        columns: Option<&[&str]>,
    ) -> Result<DataFrame, TesseraError> {
        let parts = self.resolve(&expand_refs(asset, coin, month)?).await?;
        let lazy = crate::readers::polars::build_lazyframe(&parts, columns)?;
        crate::readers::polars::collect(lazy)
    }

    /// Open one or more partitions as an in-memory DuckDB connection exposing
    /// a `tessera` view for SQL querying.
    ///
    /// # Errors
    ///
    /// Returns any [`TesseraError`] the API or transport raises.
    #[cfg(feature = "duckdb")]
    pub async fn to_duckdb(
        &self,
        asset: &str,
        coin: impl IntoCoins,
        month: impl IntoMonths,
        columns: Option<&[&str]>,
    ) -> Result<duckdb::Connection, TesseraError> {
        let parts = self.resolve(&expand_refs(asset, coin, month)?).await?;
        let owned_columns: Option<Vec<String>> =
            columns.map(|cols| cols.iter().map(|c| (*c).to_string()).collect());
        tokio::task::spawn_blocking(move || {
            let borrowed: Option<Vec<&str>> = owned_columns
                .as_deref()
                .map(|cols| cols.iter().map(String::as_str).collect());
            crate::readers::duckdb::build_relation(&parts, borrowed.as_deref())
        })
        .await
        .map_err(|err| TesseraError::Network(format!("duckdb task failed: {err}")))?
    }
}

/// Send a request with the shared retry policy (transient network errors and
/// retryable statuses back off exponentially, honouring `Retry-After`).
async fn send_with_retries(
    http: &reqwest::Client,
    base_url: &str,
    max_retries: u32,
    prepared: &PreparedRequest,
) -> Result<reqwest::Response, TesseraError> {
    for attempt in 0..=max_retries {
        let last = attempt == max_retries;
        let sent = http
            .get(format!("{base_url}{}", prepared.path))
            .query(&prepared.params)
            .send()
            .await;
        match sent {
            Err(err) if err.is_connect() || err.is_timeout() || err.is_request() => {
                if last {
                    return Err(TesseraError::Network(format!(
                        "network error contacting Tessera: {err}"
                    )));
                }
                tokio::time::sleep(backoff_delay(attempt, None)).await;
            }
            Ok(response) if !last && should_retry(response.status().as_u16()) => {
                let retry_after = parse_retry_after(response.headers());
                tokio::time::sleep(backoff_delay(attempt, retry_after)).await;
            }
            Ok(response) => {
                if response.status().is_success() {
                    return Ok(response);
                }
                let status = response.status().as_u16();
                let body = response
                    .bytes()
                    .await
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
async fn parse_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, TesseraError> {
    let body = response
        .bytes()
        .await
        .map_err(|err| TesseraError::Network(err.to_string()))?;
    serde_json::from_slice(&body).map_err(|err| TesseraError::Network(err.to_string()))
}

impl std::fmt::Debug for AsyncTesseraClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncTesseraClient")
            .field("base_url", &self.config.base_url)
            .field("timeout", &self.config.timeout)
            .finish_non_exhaustive()
    }
}
