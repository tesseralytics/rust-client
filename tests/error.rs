//! `error_from_response` mapping — the observable error contract.
use tessera::TesseraError;

#[test]
fn body_code_maps_to_variant() {
    let cases = [
        (
            "bad_request",
            400,
            TesseraError::BadRequest {
                code: None,
                message: String::new(),
            },
        ),
        (
            "unauthorized",
            401,
            TesseraError::Authentication {
                code: None,
                message: String::new(),
            },
        ),
        (
            "forbidden",
            403,
            TesseraError::Forbidden {
                code: None,
                message: String::new(),
            },
        ),
        (
            "not_found",
            404,
            TesseraError::NotFound {
                code: None,
                message: String::new(),
            },
        ),
        (
            "unavailable",
            503,
            TesseraError::ServiceUnavailable {
                code: None,
                message: String::new(),
            },
        ),
        (
            "internal",
            500,
            TesseraError::InternalServer {
                code: None,
                message: String::new(),
            },
        ),
    ];
    for (code, status, _) in cases {
        let body = format!(r#"{{"error":"{code}"}}"#);
        let err = tessera::error_from_response(status, body.as_bytes());
        assert_eq!(err.code(), Some(code), "code for {code}");
        assert_eq!(err.status_code(), Some(status), "status for {code}");
        assert_eq!(
            err.to_string(),
            format!("Tessera API request failed with HTTP {status} ({code})")
        );
    }
}

#[test]
fn unknown_code_falls_back_to_api_variant() {
    let err = tessera::error_from_response(409, br#"{"error":"conflict"}"#);
    match &err {
        TesseraError::Api {
            status_code,
            code,
            message,
        } => {
            assert_eq!(*status_code, 409);
            assert_eq!(code.as_deref(), Some("conflict"));
            assert_eq!(
                message,
                "Tessera API request failed with HTTP 409 (conflict)"
            );
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

#[test]
fn status_fallback_without_body() {
    assert!(matches!(
        tessera::error_from_response(400, b""),
        TesseraError::BadRequest { code: None, .. }
    ));
    assert!(matches!(
        tessera::error_from_response(401, b""),
        TesseraError::Authentication { code: None, .. }
    ));
    assert!(matches!(
        tessera::error_from_response(404, b""),
        TesseraError::NotFound { code: None, .. }
    ));
    assert!(matches!(
        tessera::error_from_response(502, b""),
        TesseraError::ServiceUnavailable { code: None, .. }
    ));
}

#[test]
fn unparseable_body_uses_status() {
    assert!(matches!(
        tessera::error_from_response(403, b"<html>gateway</html>"),
        TesseraError::Forbidden { code: None, .. }
    ));
    // JSON but wrong shape / non-string error.
    assert!(matches!(
        tessera::error_from_response(403, b"[1,2,3]"),
        TesseraError::Forbidden { code: None, .. }
    ));
    assert!(matches!(
        tessera::error_from_response(403, br#"{"error":42}"#),
        TesseraError::Forbidden { code: None, .. }
    ));
}

#[test]
fn unknown_status_maps_to_api() {
    let err = tessera::error_from_response(418, b"short and stout");
    match err {
        TesseraError::Api {
            status_code: 418,
            code: None,
            message,
        } => {
            assert_eq!(message, "Tessera API request failed with HTTP 418");
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

#[test]
fn client_side_errors_carry_no_status_or_code() {
    let err = TesseraError::Configuration("no key".to_string());
    assert_eq!(err.status_code(), None);
    assert_eq!(err.code(), None);
    assert_eq!(tessera::TesseraError::PresignExpired.status_code(), None);
}
