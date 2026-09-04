//! Async client parity: metadata endpoints and multi-partition reads.
#![cfg(feature = "polars")]

use polars::prelude::*;
use serde_json::json;
use tessera::{AsyncTesseraClient, ClientConfig, PartitionRef, TesseraError};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(base_url: String) -> AsyncTesseraClient {
    let mut config = ClientConfig::new(Some("test-key")).unwrap();
    config.base_url = base_url;
    AsyncTesseraClient::from_config(config).unwrap()
}

fn write_sample_parquet(path: &std::path::Path) {
    let mut frame = polars::prelude::df![
        "time" => [1i64, 2, 3, 4, 5],
        "close" => [10.0f64, 11.0, 12.0, 13.0, 14.0],
    ]
    .unwrap();
    let file = std::fs::File::create(path).unwrap();
    ParquetWriter::new(file).finish(&mut frame).unwrap();
}

#[tokio::test]
async fn datasets_parity_with_sync_surface() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/datasets"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "generated_at": "2025-01-01T00:00:00Z",
            "datasets": [{"name": "gold_ohlcv_1m", "partition_count": 1, "coins": ["BTC"], "months": {}}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(server.uri());
    let datasets = client.datasets().await.unwrap();
    assert_eq!(datasets.datasets[0].name, "gold_ohlcv_1m");
    assert_eq!(datasets.datasets[0].partition_count, 1);
    assert!(datasets.datasets[0].months.earliest.is_none());

    server.verify().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_partition_read_carries_provenance() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let btc = dir.path().join("btc.parquet");
    let eth = dir.path().join("eth.parquet");
    write_sample_parquet(&btc);
    write_sample_parquet(&eth);

    for (coin, month, file) in [("BTC", "2025-09", btc), ("ETH", "2025-09", eth)] {
        Mock::given(method("GET"))
            .and(path(format!(
                "/v1/datasets/gold_ohlcv_1m/{coin}/{month}/download"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "url": file.to_string_lossy(),
                "expires_at": "2025-01-01T00:15:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;
    }

    let client = client(server.uri());
    let frame = client
        .read("gold_ohlcv_1m", ["BTC", "ETH"], "2025-09", None)
        .await
        .unwrap();
    assert_eq!(frame.height(), 10);
    let coin = frame.column("coin").unwrap().as_materialized_series();
    assert_eq!(coin.unique().unwrap().len(), 2, "both coins present");

    server.verify().await;
}

#[tokio::test]
async fn partition_refs_sync_helper() {
    let client = AsyncTesseraClient::new(Some("k")).unwrap();
    let refs = client
        .partition_refs("gold_ohlcv_1m", "BTC", "2025-09")
        .unwrap();
    assert_eq!(
        refs,
        vec![PartitionRef::new("gold_ohlcv_1m", "BTC", "2025-09").unwrap()]
    );
}

#[tokio::test]
async fn missing_key_is_configuration_error() {
    if std::env::var_os("TESSERA_API_KEY").is_some() {
        return;
    }
    let err = AsyncTesseraClient::new(None).unwrap_err();
    assert!(matches!(err, TesseraError::Configuration(_)), "got {err:?}");
}
