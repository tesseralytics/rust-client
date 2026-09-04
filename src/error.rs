//! Error taxonomy mirroring the Python SDK's exception hierarchy.
//!
//! Python's distinct exception classes map onto a single `#[non_exhaustive]`
//! enum; users `match` on [`TesseraError::NotFound`] instead of `except`.
//! Python's `MissingDependencyError` is deliberately omitted: Rust encodes
//! "optional dependency" as cargo features, so a disabled feature means the
//! method is absent at compile time rather than a runtime error.

/// Every error raised by this crate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TesseraError {
    /// The client was misconfigured — e.g. no API key could be resolved.
    #[error("{0}")]
    Configuration(String),
    /// A request argument was invalid (month format, empty coin/month, reversed span).
    #[error("{0}")]
    InvalidArgument(String),
    /// A network-level failure (connection/timeout) after exhausting retries.
    #[error("network error contacting Tessera: {0}")]
    Network(String),
    /// A presigned download URL expired before the read completed.
    #[error(
        "A presigned download URL was rejected (likely expired). Presigned URLs are short-lived — call read()/scan() again to mint fresh ones."
    )]
    PresignExpired,
    /// 400 — the request was malformed (bad coin, month format, etc.).
    #[error("{message}")]
    BadRequest {
        /// Machine-readable error code from the `{"error": ...}` body, if any.
        code: Option<String>,
        /// Human-readable message.
        message: String,
    },
    /// 401 — the API key is missing, invalid, or revoked.
    #[error("{message}")]
    Authentication {
        /// Machine-readable error code from the `{"error": ...}` body, if any.
        code: Option<String>,
        /// Human-readable message.
        message: String,
    },
    /// 403 — your plan does not grant access to this dataset or coin.
    #[error("{message}")]
    Forbidden {
        /// Machine-readable error code from the `{"error": ...}` body, if any.
        code: Option<String>,
        /// Human-readable message.
        message: String,
    },
    /// 404 — the dataset, coin, or partition does not exist.
    #[error("{message}")]
    NotFound {
        /// Machine-readable error code from the `{"error": ...}` body, if any.
        code: Option<String>,
        /// Human-readable message.
        message: String,
    },
    /// 503 — the catalog is temporarily unavailable. Safe to retry.
    #[error("{message}")]
    ServiceUnavailable {
        /// Machine-readable error code from the `{"error": ...}` body, if any.
        code: Option<String>,
        /// Human-readable message.
        message: String,
    },
    /// 500 — an unexpected server error.
    #[error("{message}")]
    InternalServer {
        /// Machine-readable error code from the `{"error": ...}` body, if any.
        code: Option<String>,
        /// Human-readable message.
        message: String,
    },
    /// Any other error response, carrying its raw status code.
    #[error("{message}")]
    Api {
        /// HTTP status code of the response.
        status_code: u16,
        /// Machine-readable error code from the `{"error": ...}` body, if any.
        code: Option<String>,
        /// Human-readable message.
        message: String,
    },
}

impl TesseraError {
    /// The HTTP status code for API-derived errors; `None` for client-side errors.
    #[must_use]
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::BadRequest { .. } => Some(400),
            Self::Authentication { .. } => Some(401),
            Self::Forbidden { .. } => Some(403),
            Self::NotFound { .. } => Some(404),
            Self::ServiceUnavailable { .. } => Some(503),
            Self::InternalServer { .. } => Some(500),
            Self::Api { status_code, .. } => Some(*status_code),
            Self::Configuration(_)
            | Self::InvalidArgument(_)
            | Self::Network(_)
            | Self::PresignExpired => None,
        }
    }

    /// The machine-readable error code from the response body, if any.
    #[must_use]
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::BadRequest { code, .. }
            | Self::Authentication { code, .. }
            | Self::Forbidden { code, .. }
            | Self::NotFound { code, .. }
            | Self::ServiceUnavailable { code, .. }
            | Self::InternalServer { code, .. }
            | Self::Api { code, .. } => code.as_deref(),
            Self::Configuration(_)
            | Self::InvalidArgument(_)
            | Self::Network(_)
            | Self::PresignExpired => None,
        }
    }
}

/// Build the appropriate [`TesseraError`] from an error response.
///
/// Prefers the `{"error": "<code>"}` body; falls back to the HTTP status.
pub fn error_from_response(status_code: u16, body: &[u8]) -> TesseraError {
    let code: Option<String> = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|parsed| {
            parsed
                .get("error")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        });

    let detail = code
        .as_ref()
        .map_or_else(String::new, |c| format!(" ({c})"));
    let message = format!("Tessera API request failed with HTTP {status_code}{detail}");

    if let Some(c) = &code {
        return match c.as_str() {
            "bad_request" => TesseraError::BadRequest {
                code: Some(c.clone()),
                message,
            },
            "unauthorized" => TesseraError::Authentication {
                code: Some(c.clone()),
                message,
            },
            "forbidden" => TesseraError::Forbidden {
                code: Some(c.clone()),
                message,
            },
            "not_found" => TesseraError::NotFound {
                code: Some(c.clone()),
                message,
            },
            "unavailable" => TesseraError::ServiceUnavailable {
                code: Some(c.clone()),
                message,
            },
            "internal" => TesseraError::InternalServer {
                code: Some(c.clone()),
                message,
            },
            _ => TesseraError::Api {
                status_code,
                code: Some(c.clone()),
                message,
            },
        };
    }
    match status_code {
        400 => TesseraError::BadRequest {
            code: None,
            message,
        },
        401 => TesseraError::Authentication {
            code: None,
            message,
        },
        403 => TesseraError::Forbidden {
            code: None,
            message,
        },
        404 => TesseraError::NotFound {
            code: None,
            message,
        },
        500 => TesseraError::InternalServer {
            code: None,
            message,
        },
        502..=504 => TesseraError::ServiceUnavailable {
            code: None,
            message,
        },
        _ => TesseraError::Api {
            status_code,
            code: None,
            message,
        },
    }
}
