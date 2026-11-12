//! Model helpers: month validation, spans, refs, coercion traits.
use tessera::{
    DayDownload, DayDownloadResponse, DownloadBody, DownloadResponse, IntoCoins, IntoMonths,
    MonthSpan, PartitionRef, TesseraError, download_urls,
};

#[test]
fn month_span_is_inclusive_and_crosses_years() {
    let span = MonthSpan::new("2024-11", "2025-02").unwrap();
    assert_eq!(span.months(), ["2024-11", "2024-12", "2025-01", "2025-02"]);
}

#[test]
fn month_span_single_month() {
    let span = MonthSpan::new("2025-09", "2025-09").unwrap();
    assert_eq!(span.months(), ["2025-09"]);
}

#[test]
fn month_span_long_range() {
    let span = MonthSpan::new("2025-01", "2025-09").unwrap();
    assert_eq!(span.months().len(), 9);
    assert_eq!(span.months()[0], "2025-01");
    assert_eq!(span.months()[8], "2025-09");
}

#[test]
fn month_span_reversed_is_error() {
    let err = MonthSpan::new("2025-09", "2025-01").unwrap_err();
    assert!(matches!(err, TesseraError::InvalidArgument(_)));
    assert_eq!(
        err.to_string(),
        "MonthSpan start \"2025-09\" is after end \"2025-01\""
    );
}

#[test]
fn month_validation_rejects_bad_shapes() {
    for bad in [
        "2025-13", "2025-00", "25-09", "2025/09", "2025-9", "2025-099", "abcd-xy",
    ] {
        let err = MonthSpan::new("2025-01", bad).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("month must be in YYYY-MM format, got {bad:?}"),
            "case {bad}"
        );
    }
}

#[test]
fn partition_ref_object_key_layout() {
    let partition = PartitionRef::new("gold_ohlcv_1m", "BTC", "2025-09").unwrap();
    assert_eq!(
        partition.object_key(),
        "gold_ohlcv_1m/coin=BTC/month=2025-09.parquet"
    );
}

#[test]
fn partition_ref_rejects_bad_month() {
    assert!(PartitionRef::new("gold_ohlcv_1m", "BTC", "2025-9").is_err());
}

#[test]
fn into_coins_accepts_str_and_lists() {
    assert_eq!("BTC".into_coins().unwrap(), ["BTC"]);
    assert_eq!(String::from("BTC").into_coins().unwrap(), ["BTC"]);
    assert_eq!(["BTC", "ETH"].into_coins().unwrap(), ["BTC", "ETH"]);
    assert_eq!(vec!["BTC", "ETH"].into_coins().unwrap(), ["BTC", "ETH"]);
    let owned = vec![String::from("BTC"), String::from("ETH")];
    assert_eq!(owned.into_coins().unwrap(), ["BTC", "ETH"]);
}

#[test]
fn into_coins_rejects_empty() {
    let empty: [&str; 0] = [];
    let err = empty.into_coins().unwrap_err();
    assert_eq!(err.to_string(), "at least one coin is required");
}

#[test]
fn into_months_accepts_str_span_and_lists() {
    assert_eq!("2025-09".into_months().unwrap(), ["2025-09"]);
    let span = MonthSpan::new("2025-01", "2025-03").unwrap();
    assert_eq!(
        span.into_months().unwrap(),
        ["2025-01", "2025-02", "2025-03"]
    );
    assert_eq!(
        ["2025-01", "2025-02"].into_months().unwrap(),
        ["2025-01", "2025-02"]
    );
}

#[test]
fn into_months_validates_each_entry() {
    let err = ["2025-01", "bogus"].into_months().unwrap_err();
    assert_eq!(
        err.to_string(),
        "month must be in YYYY-MM format, got \"bogus\""
    );
}

#[test]
fn into_months_rejects_empty() {
    let empty: [&str; 0] = [];
    let err = empty.into_months().unwrap_err();
    assert_eq!(err.to_string(), "at least one month is required");
}

#[test]
fn download_urls_finalised_month_is_one_object() {
    let body = DownloadBody::DownloadResponse(DownloadResponse {
        url: "https://storage.example.com/2025-09.parquet".to_string(),
        expires_at: "2025-01-01T00:15:00Z".to_string(),
    });
    assert_eq!(
        download_urls(body).unwrap(),
        ["https://storage.example.com/2025-09.parquet"]
    );
}

#[test]
fn download_urls_orders_day_objects_by_date() {
    let day = |date: &str, url: &str| DayDownload {
        date: date.to_string(),
        url: url.to_string(),
        expires_at: "2025-01-01T00:15:00Z".to_string(),
    };
    let body = DownloadBody::DayDownloadResponse(DayDownloadResponse {
        asset: "gold_ohlcv_1m".to_string(),
        coin: Some("BTC".to_string()),
        month: "2026-10".to_string(),
        is_open: true,
        days: vec![
            day("2026-10-02", "https://storage.example.com/02.parquet"),
            day("2026-10-01", "https://storage.example.com/01.parquet"),
        ],
    });
    assert_eq!(
        download_urls(body).unwrap(),
        [
            "https://storage.example.com/01.parquet",
            "https://storage.example.com/02.parquet"
        ]
    );
}

#[test]
fn download_urls_rejects_day_backed_partition_without_days() {
    let body = DownloadBody::DayDownloadResponse(DayDownloadResponse {
        asset: "gold_ohlcv_1m".to_string(),
        coin: Some("BTC".to_string()),
        month: "2026-10".to_string(),
        is_open: true,
        days: Vec::new(),
    });
    let err = download_urls(body).unwrap_err();
    assert!(
        matches!(err, TesseraError::UnexpectedResponse(_)),
        "got {err:?}"
    );
    assert!(err.to_string().contains("gold_ohlcv_1m/BTC/2026-10"));
}
