//! Reader plumbing: lazyframe construction, projection, expiry translation,
//! and `(asset, coins, months)` expansion. Uses local Parquet files.
#![cfg(feature = "polars")]

use polars::prelude::*;
use tessera::readers::expand_refs;
use tessera::readers::polars::{build_lazyframe, collect};
use tessera::{MonthSpan, PartitionRef};

/// Write a deterministic 5-row sample frame: `time` Int64, `close` Float64.
fn write_sample_parquet(path: &std::path::Path) {
    let mut frame = df![
        "time" => [1i64, 2, 3, 4, 5],
        "close" => [10.0f64, 11.0, 12.0, 13.0, 14.0],
    ]
    .unwrap();
    let file = std::fs::File::create(path).unwrap();
    ParquetWriter::new(file).finish(&mut frame).unwrap();
}

fn resolved(path: &std::path::Path, coin: &str, month: &str) -> (tessera::PartitionRef, String) {
    (
        PartitionRef::new("gold_ohlcv_1m", coin, month).unwrap(),
        path.to_string_lossy().to_string(),
    )
}

#[test]
fn single_partition_frame_is_pristine() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("btc.parquet");
    write_sample_parquet(&path);

    let parts = vec![resolved(&path, "BTC", "2025-09")];
    let frame = build_lazyframe(&parts, None).unwrap().collect().unwrap();
    assert_eq!(frame.height(), 5);
    assert_eq!(frame.width(), 2);
    assert!(
        frame.column("coin").is_err(),
        "single read must not inject provenance columns"
    );
}

#[test]
fn multi_partition_frames_inject_provenance_columns() {
    let dir = tempfile::tempdir().unwrap();
    let btc = dir.path().join("btc.parquet");
    let eth = dir.path().join("eth.parquet");
    write_sample_parquet(&btc);
    write_sample_parquet(&eth);

    let parts = vec![
        resolved(&btc, "BTC", "2025-09"),
        resolved(&eth, "ETH", "2025-10"),
    ];
    let frame = build_lazyframe(&parts, None).unwrap().collect().unwrap();
    assert_eq!(frame.height(), 10);
    assert_eq!(frame.width(), 4);

    let coin = frame.column("coin").unwrap().as_materialized_series();
    assert_eq!(
        coin.unique().unwrap().len(),
        2,
        "coin column must carry both partitions"
    );
    let month = frame.column("month").unwrap().as_materialized_series();
    assert_eq!(month.unique().unwrap().len(), 2);
}

#[test]
fn projection_narrows_columns() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("btc.parquet");
    write_sample_parquet(&path);

    let parts = vec![resolved(&path, "BTC", "2025-09")];
    let frame = build_lazyframe(&parts, Some(&["time", "close"]))
        .unwrap()
        .collect()
        .unwrap();
    assert_eq!(frame.width(), 2);
    assert_eq!(frame.get_column_names(), &["time", "close"]);
}

#[test]
fn collect_translates_presign_expiry_marker() {
    // Bind a port, drop the listener: the http store GET fails fast; the
    // object_store error text contains "403"/"forbidden" only over real
    // rejections, so assert the generic failure path instead and cover the
    // 403 marker via the mock server test below.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.parquet");

    let parts = vec![resolved(&path, "BTC", "2025-09")];
    let err = collect(build_lazyframe(&parts, None).unwrap()).unwrap_err();
    assert!(
        matches!(err, tessera::TesseraError::Network(_)),
        "got {err:?}"
    );
}

#[test]
fn collect_maps_403_to_presign_expired() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = rt.block_on(wiremock::MockServer::start());
    rt.block_on(async {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(403)
                    .set_body_string("AccessDenied: presigned url expired"),
            )
            .mount(&server)
            .await;
    });

    let url = format!("{}/expired.parquet", server.uri());
    let parts = vec![(
        PartitionRef::new("gold_ohlcv_1m", "BTC", "2025-09").unwrap(),
        url,
    )];
    let lazy = build_lazyframe(&parts, None).unwrap();
    let err = rt.block_on(async { collect(lazy) }).unwrap_err();
    assert!(
        matches!(err, tessera::TesseraError::PresignExpired),
        "got {err:?}"
    );
}

#[test]
fn expand_refs_is_cartesian_coin_major() {
    let refs = expand_refs(
        "gold_ohlcv_1m",
        ["BTC", "ETH"],
        MonthSpan::new("2025-01", "2025-02").unwrap(),
    )
    .unwrap();
    assert_eq!(refs.len(), 4);
    let pairs: Vec<(&str, &str)> = refs
        .iter()
        .map(|p| (p.coin.as_str(), p.month.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("BTC", "2025-01"),
            ("BTC", "2025-02"),
            ("ETH", "2025-01"),
            ("ETH", "2025-02")
        ]
    );
}

#[test]
fn expand_refs_validates_months() {
    let err = expand_refs("gold_ohlcv_1m", ["BTC"], ["2025-13"]).unwrap_err();
    assert!(
        matches!(err, tessera::TesseraError::InvalidArgument(_)),
        "got {err:?}"
    );
}
