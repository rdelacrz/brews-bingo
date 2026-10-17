//! Common private JSON response headers; serialization and error policy stay with callers.
use crate::api::ApiError;
use axum::{body::Body, response::Response};
use http::HeaderValue;

pub(super) fn private_json_headers(response: &mut Response, content_type: &'static str) {
    for (name, value) in [
        ("content-type", content_type),
        ("cache-control", "no-store"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
}

pub(super) fn json_bytes(status: u16, bytes: Vec<u8>) -> Result<Response, ApiError> {
    let mut response = http::Response::builder()
        .status(status)
        .body(Body::from(bytes))
        .map_err(|_| ApiError::Unavailable)?;
    private_json_headers(&mut response, "application/json; charset=utf-8");
    Ok(response)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Response fixtures fail fast.")]
mod tests {
    use super::*;

    #[tokio::test]
    async fn json_bytes_preserves_status_payload_and_private_headers() {
        let response = json_bytes(409, b"{}".to_vec()).unwrap();
        assert_eq!(response.status(), 409);
        assert_eq!(response.headers().len(), 4);
        assert_eq!(
            response.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["referrer-policy"], "no-referrer");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 2)
                .await
                .unwrap()
                .as_ref(),
            b"{}"
        );
        assert_eq!(
            json_bytes(99, Vec::new()).err(),
            Some(ApiError::Unavailable)
        );
    }

    #[test]
    fn headers_keep_endpoint_media_type_status_and_unrelated_headers() {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = http::StatusCode::UNAUTHORIZED;
        response
            .headers_mut()
            .insert("retry-after", HeaderValue::from_static("1"));
        response
            .headers_mut()
            .insert("cache-control", HeaderValue::from_static("public"));
        private_json_headers(&mut response, "application/json");
        assert_eq!(response.status(), 401);
        assert_eq!(response.headers()["content-type"], "application/json");
        assert_eq!(response.headers()["retry-after"], "1");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(!response.headers().contains_key("set-cookie"));
    }
}
