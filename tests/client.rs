//! Sync `TesseraClient` behavior over a mocked HTTP server.
//!
//! The sync client builds its own blocking reqwest client, which panics when
//! constructed inside a tokio runtime; tests therefore run on plain threads
//! and host the mock on a dedicated current-thread runtime.
use serde_json::json;
use tessera::{ClientConfig, TesseraClient, TesseraError};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Runtime hosting the mock server, independent from the client's internals.
fn mock_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("mock runtime")
}

fn client(base_url: String) -> TesseraClient {
    let mut config = ClientConfig::new(Some("test-key")).unwrap();
    config.base_url = base_url;
    TesseraClient::from_config(config).unwrap()
}

#[test]
fn datasets_parses_response_and_sends_auth() {
    let rt = mock_runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/v1/datasets"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "generated_at": "2025-01-01T00:00:00Z",
                "datasets": [{
                    "name": "gold_ohlcv_1m",
                    "partition_count": 3,
                    "coins": ["BTC"],
                    "months": {"earliest": "2025-01", "latest": "2025-03"}
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
    });

    let client = client(server.uri());
    let datasets = client.datasets().unwrap();
    assert_eq!(datasets.generated_at, "2025-01-01T00:00:00Z");
    assert_eq!(datasets.datasets.len(), 1);
    assert_eq!(datasets.datasets[0].name, "gold_ohlcv_1m");
    assert_eq!(datasets.datasets[0].partition_count, 3);
    assert_eq!(
        datasets.datasets[0].months.earliest.as_deref(),
        Some("2025-01")
    );

    rt.block_on(server.verify());
}

#[test]
fn partitions_passes_query_params() {
    let rt = mock_runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/v1/datasets/gold_ohlcv_1m"))
            .and(query_param("coin", "BTC"))
            .and(query_param("month", "2025-09"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "asset": "gold_ohlcv_1m",
                "generated_at": "2025-01-01T00:00:00Z",
                "partitions": [{
                    "coin": "BTC", "month": "2025-09", "size_bytes": 1234
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
    });

    let client = client(server.uri());
    let partitions = client
        .partitions("gold_ohlcv_1m", Some("BTC"), Some("2025-09"))
        .unwrap();
    assert_eq!(partitions.partitions.len(), 1);
    assert_eq!(partitions.partitions[0].size_bytes, 1234);

    rt.block_on(server.verify());
}

#[test]
fn partitions_without_filters_omits_query() {
    let rt = mock_runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/v1/datasets/anything"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "asset": "anything",
                "generated_at": "2025-01-01T00:00:00Z",
                "partitions": []
            })))
            .expect(1)
            .mount(&server)
            .await;
    });

    let client = client(server.uri());
    let partitions = client.partitions("anything", None, None).unwrap();
    assert!(partitions.partitions.is_empty());
    rt.block_on(server.verify());
}

#[test]
fn download_url_does_not_follow_redirects() {
    let rt = mock_runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        // Production fronts the API via CloudFront; the client must surface
        // the raw 302 (redirects disabled) rather than chasing the location.
        Mock::given(method("GET"))
            .and(path("/v1/datasets/gold_ohlcv_1m/BTC/2025-09/download"))
            .and(header("accept", "application/json"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", "https://storage.example.com/parquet"),
            )
            .expect(1)
            .mount(&server)
            .await;
    });

    let client = client(server.uri());
    let err = client
        .download_url("gold_ohlcv_1m", "BTC", "2025-09")
        .unwrap_err();
    assert!(
        matches!(
            err,
            TesseraError::Api {
                status_code: 302,
                ..
            }
        ),
        "got {err:?}"
    );

    rt.block_on(server.verify());
}

#[test]
fn download_url_parses_presign_json() {
    let rt = mock_runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/v1/datasets/gold_ohlcv_1m/BTC/2025-09/download"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "url": "https://storage.example.com/parquet",
                "expires_at": "2025-01-01T00:15:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;
    });

    let client = client(server.uri());
    let download = client
        .download_url("gold_ohlcv_1m", "BTC", "2025-09")
        .unwrap();
    assert_eq!(download.url, "https://storage.example.com/parquet");
    assert_eq!(download.expires_at, "2025-01-01T00:15:00Z");

    rt.block_on(server.verify());
}

#[test]
fn error_response_maps_to_forbidden() {
    let rt = mock_runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({"error": "forbidden"})))
            .mount(&server)
            .await;
    });

    let client = client(server.uri());
    let err = client.datasets().unwrap_err();
    assert!(matches!(err, TesseraError::Forbidden { .. }), "got {err:?}");
    assert_eq!(err.code(), Some("forbidden"));
    assert_eq!(err.status_code(), Some(403));
}

#[test]
fn missing_api_key_is_configuration_error() {
    if std::env::var_os("TESSERA_API_KEY").is_some() {
        // The environment resolves a key; nothing to assert here.
        return;
    }
    let err = TesseraClient::new(None).unwrap_err();
    assert!(matches!(err, TesseraError::Configuration(_)), "got {err:?}");
    assert!(err.to_string().contains("No API key provided"));
}
