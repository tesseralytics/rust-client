//! Retry behavior: transient statuses retried with backoff, others fail fast.
use serde_json::json;
use tessera::{ClientConfig, TesseraClient, TesseraError};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mock_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("mock runtime")
}

fn client_with_retries(base_url: String, max_retries: u32) -> TesseraClient {
    let mut config = ClientConfig::new(Some("test-key")).unwrap();
    config.base_url = base_url;
    config.max_retries = max_retries;
    TesseraClient::from_config(config).unwrap()
}

#[test]
fn transient_503_is_retried_then_succeeds() {
    let rt = mock_runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "generated_at": "2025-01-01T00:00:00Z", "datasets": []
            })))
            .expect(1)
            .mount(&server)
            .await;
    });

    let client = client_with_retries(server.uri(), 3);
    let datasets = client.datasets().unwrap();
    assert_eq!(datasets.datasets.len(), 0);
    rt.block_on(server.verify());
}

#[test]
fn gives_up_after_max_retries() {
    let rt = mock_runtime();
    let server = rt.block_on(MockServer::start());
    // max_retries=1 -> initial attempt + 1 retry = 2 calls, then fail.
    rt.block_on(async {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .expect(2)
            .mount(&server)
            .await;
    });

    let client = client_with_retries(server.uri(), 1);
    let err = client.datasets().unwrap_err();
    assert!(
        matches!(err, TesseraError::ServiceUnavailable { .. }),
        "got {err:?}"
    );
    rt.block_on(server.verify());
}

#[test]
fn auth_error_is_never_retried() {
    let rt = mock_runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(json!({"error": "unauthorized"})),
            )
            .expect(1)
            .mount(&server)
            .await;
    });

    let client = client_with_retries(server.uri(), 3);
    let err = client.datasets().unwrap_err();
    assert!(
        matches!(err, TesseraError::Authentication { .. }),
        "got {err:?}"
    );
    rt.block_on(server.verify());
}

#[test]
fn network_failure_surfaces_as_network_error() {
    // Bind and release a port: connecting there is refused immediately.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let client = client_with_retries(format!("http://127.0.0.1:{port}"), 0);
    let err = client.datasets().unwrap_err();
    assert!(matches!(err, TesseraError::Network(_)), "got {err:?}");
    assert!(err.to_string().contains("network error contacting Tessera"));
}

#[test]
fn retry_after_header_is_honoured_via_success() {
    // Retry-After: 0 on the first response keeps the test fast while
    // exercising the header parsing path end to end.
    let rt = mock_runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "generated_at": "2025-01-01T00:00:00Z", "datasets": []
            })))
            .expect(1)
            .mount(&server)
            .await;
    });

    let client = client_with_retries(server.uri(), 1);
    client.datasets().unwrap();
    rt.block_on(server.verify());
}

#[test]
fn json_error_body_is_used_over_status() {
    let rt = mock_runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        // 400 with a `not_found` code: the body code wins.
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error": "not_found"})))
            .expect(1)
            .mount(&server)
            .await;
    });

    let client = client_with_retries(server.uri(), 0);
    let err = client.datasets().unwrap_err();
    assert!(matches!(err, TesseraError::NotFound { .. }), "got {err:?}");
}
