//! DuckDB reader: local Parquet exposed through the `tessera` view.
#![cfg(feature = "duckdb")]
#![allow(clippy::doc_markdown)]

use tessera::PartitionRef;
use tessera::readers::duckdb::build_relation;

fn write_sample_parquet(path: &std::path::Path) {
    use polars::prelude::*;
    let mut frame = df![
        "time" => [1i64, 2, 3, 4, 5],
        "close" => [10.0f64, 11.0, 12.0, 13.0, 14.0],
    ]
    .unwrap();
    let file = std::fs::File::create(path).unwrap();
    ParquetWriter::new(file).finish(&mut frame).unwrap();
}

#[test]
fn single_partition_is_queryable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("btc.parquet");
    write_sample_parquet(&path);

    let parts = vec![(
        PartitionRef::new("gold_ohlcv_1m", "BTC", "2025-09").unwrap(),
        path.to_string_lossy().to_string(),
    )];
    let connection = build_relation(&parts, None).unwrap();
    let count: i64 = connection
        .query_row("SELECT count(*) FROM tessera", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 5);
}

#[test]
fn multi_partition_unions_with_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let btc = dir.path().join("btc.parquet");
    let eth = dir.path().join("eth.parquet");
    write_sample_parquet(&btc);
    write_sample_parquet(&eth);

    let parts = vec![
        (
            PartitionRef::new("gold_ohlcv_1m", "BTC", "2025-09").unwrap(),
            btc.to_string_lossy().to_string(),
        ),
        (
            PartitionRef::new("gold_ohlcv_1m", "ETH", "2025-10").unwrap(),
            eth.to_string_lossy().to_string(),
        ),
    ];
    let connection = build_relation(&parts, None).unwrap();
    let count: i64 = connection
        .query_row("SELECT count(*) FROM tessera", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 10);

    let coins: Vec<String> = connection
        .prepare("SELECT DISTINCT coin FROM tessera ORDER BY coin")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(coins, ["BTC", "ETH"]);
}

#[test]
fn projection_narrows_leaf_selects() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("btc.parquet");
    write_sample_parquet(&path);

    let parts = vec![(
        PartitionRef::new("gold_ohlcv_1m", "BTC", "2025-09").unwrap(),
        path.to_string_lossy().to_string(),
    )];
    let connection = build_relation(&parts, Some(&["close"])).unwrap();
    let close_sum: f64 = connection
        .query_row("SELECT sum(close) FROM tessera", [], |row| row.get(0))
        .unwrap();
    assert!((close_sum - 60.0).abs() < 1e-9);
}
