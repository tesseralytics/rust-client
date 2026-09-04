//! Client configuration and shared helpers (transport-agnostic).

use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};

use crate::error::TesseraError;

/// Production API base URL.
pub const DEFAULT_BASE_URL: &str = "https://tesseralytics.dev";

/// Environment variable consulted when no explicit API key is passed.
pub const API_KEY_ENV_VAR: &str = "TESSERA_API_KEY";

/// Default `User-Agent` for requests made by this client.
pub const USER_AGENT: &str = concat!("tessera-rust/", env!("CARGO_PKG_VERSION"));

/// Statuses worth retrying: rate limiting + transient server/gateway errors.
pub(crate) const RETRYABLE: &[u16] = &[429, 500, 502, 503, 504];

/// Resolve the API key from the argument, then `$TESSERA_API_KEY`.
///
/// # Errors
///
/// Returns [`TesseraError::Configuration`] when no key can be resolved.
pub fn resolve_api_key(api_key: Option<&str>) -> Result<String, TesseraError> {
    api_key
        .map(str::to_string)
        .or_else(|| std::env::var(API_KEY_ENV_VAR).ok())
        .filter(|key| !key.is_empty())
        .ok_or_else(|| {
            TesseraError::Configuration(
                "No API key provided. Pass api_key=... or set the TESSERA_API_KEY environment variable. Get a free key at https://tesseralytics.dev.".to_string(),
            )
        })
}

/// Compute the delay before a retry.
///
/// Honours a server `Retry-After` when present (and non-negative), otherwise
/// exponential backoff (0.5s, 1s, 2s, …) with full jitter to avoid thundering
/// herds (`fastrand` draws from the half-open interval `[0, 1)`).
#[must_use]
pub fn backoff_delay(attempt: u32, retry_after: Option<f64>) -> Duration {
    if let Some(seconds) = retry_after.filter(|s| *s >= 0.0) {
        return Duration::from_secs_f64(seconds);
    }
    let base = 0.5 * 2f64.powi(i32::try_from(attempt).unwrap_or(63));
    Duration::from_secs_f64(fastrand::f64() * base)
}

/// Resolved configuration shared by the sync and async clients.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Tessera API key (sent as `Authorization: Bearer ...`).
    pub api_key: String,
    /// API base URL, trailing `/` stripped.
    pub base_url: String,
    /// Per-request timeout.
    pub timeout: Duration,
    /// Retries for transient failures (429/5xx, network errors).
    pub max_retries: u32,
    /// `User-Agent` header value.
    pub user_agent: String,
}

impl ClientConfig {
    /// Build a configuration from an explicit key or the environment.
    ///
    /// # Errors
    ///
    /// Returns [`TesseraError::Configuration`] when no API key can be resolved.
    pub fn new(api_key: Option<&str>) -> Result<Self, TesseraError> {
        Ok(Self {
            api_key: resolve_api_key(api_key)?,
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            user_agent: USER_AGENT.to_string(),
        })
    }

    /// Headers every request carries: bearer auth, user agent, JSON accept.
    #[must_use]
    pub fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", self.api_key)) {
            headers.insert(AUTHORIZATION, value);
        }
        if let Ok(value) = HeaderValue::from_str(&self.user_agent) {
            headers.insert(reqwest::header::USER_AGENT, value);
        }
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers
    }
}
