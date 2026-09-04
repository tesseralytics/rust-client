//! Transport-agnostic plumbing shared by the sync and async clients.
//!
//! The two clients differ only in how they *await* I/O; everything else — URL
//! construction, header building, retry decisions — lives here so there is a
//! single source of truth.

use percent_encoding::{AsciiSet, utf8_percent_encode};
use reqwest::header::HeaderMap;

use crate::config::RETRYABLE;

/// URL-encode a single path segment (no slashes survive).
///
/// Matches Python's `quote(value, safe="")`: letters, digits and `_.-~` are
/// left unescaped; everything else (including `/`) is percent-encoded.
fn seg(value: &str) -> String {
    utf8_percent_encode(value, PATH_SEGMENT).to_string()
}

/// Unreserved characters kept verbatim, exactly Python's `quote(safe="")`.
const PATH_SEGMENT: &AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'_')
    .remove(b'.')
    .remove(b'-')
    .remove(b'~');

/// A resolved HTTP request: path + query params, ready to send.
pub(crate) struct PreparedRequest {
    /// Percent-encoded path, relative to the base URL.
    pub(crate) path: String,
    /// Query parameters, in insertion order; omitted when empty.
    pub(crate) params: Vec<(&'static str, String)>,
}

/// `GET /v1/datasets` — list datasets.
pub(crate) fn datasets_request() -> PreparedRequest {
    PreparedRequest {
        path: "/v1/datasets".to_string(),
        params: Vec::new(),
    }
}

/// `GET /v1/datasets/{asset}` — list partitions, optionally filtered.
pub(crate) fn partitions_request(
    asset: &str,
    coin: Option<&str>,
    month: Option<&str>,
) -> PreparedRequest {
    let mut params = Vec::new();
    if let Some(coin) = coin {
        params.push(("coin", coin.to_string()));
    }
    if let Some(month) = month {
        params.push(("month", month.to_string()));
    }
    PreparedRequest {
        path: format!("/v1/datasets/{}", seg(asset)),
        params,
    }
}

/// `GET /v1/datasets/{asset}/{coin}/{month}/download` — mint a presigned URL.
pub(crate) fn download_request(asset: &str, coin: &str, month: &str) -> PreparedRequest {
    PreparedRequest {
        path: format!(
            "/v1/datasets/{}/{}/{}/download",
            seg(asset),
            seg(coin),
            seg(month)
        ),
        params: Vec::new(),
    }
}

/// Whether a status code is worth retrying (429/5xx).
pub(crate) fn should_retry(status_code: u16) -> bool {
    RETRYABLE.contains(&status_code)
}

/// Parse the `Retry-After` header (seconds form only) if present.
pub(crate) fn parse_retry_after(headers: &HeaderMap) -> Option<f64> {
    headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.parse::<f64>().ok())
}

#[cfg(test)]
mod tests {
    use super::{download_request, parse_retry_after, partitions_request, seg, should_retry};

    #[test]
    fn seg_escapes_like_python_quote_safe_empty() {
        assert_eq!(seg("gold_ohlcv_1m"), "gold_ohlcv_1m");
        assert_eq!(seg("BTC/USDT"), "BTC%2FUSDT");
        assert_eq!(seg("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(seg("ü"), "%C3%BC");
    }

    #[test]
    fn builders_shape_paths_and_params() {
        let prepared = partitions_request("gold_ohlcv_1m", Some("BTC"), None);
        assert_eq!(prepared.path, "/v1/datasets/gold_ohlcv_1m");
        assert_eq!(prepared.params, vec![("coin", "BTC".to_string())]);

        let prepared = download_request("gold_ohlcv_1m", "BTC", "2025-09");
        assert_eq!(
            prepared.path,
            "/v1/datasets/gold_ohlcv_1m/BTC/2025-09/download"
        );
    }

    #[test]
    fn retry_decision_and_retry_after_parsing() {
        assert!(should_retry(429));
        assert!(should_retry(503));
        assert!(!should_retry(401));

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after", "12".try_into().unwrap());
        assert_eq!(parse_retry_after(&headers), Some(12.0));
        headers.insert("retry-after", "soon".try_into().unwrap());
        assert_eq!(parse_retry_after(&headers), None);
        assert_eq!(parse_retry_after(&reqwest::header::HeaderMap::new()), None);
    }
}
