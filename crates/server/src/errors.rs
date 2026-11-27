//! 画面でない答えの誤りの形。
use axum::http::{HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// 守りの層などが返す誤りの答え。castlist・pagetray の API の誤りと同じ restJson1 の形にする:
/// header `X-Amzn-Errortype` に型の名（`kind`。ai.common の `Forbidden`・`PayloadTooLarge`・`Unavailable`）、本文は JSON の `{"message":…}`。
pub fn reply(status: StatusCode, kind: &'static str, message: &str) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (HeaderName::from_static("x-amzn-errortype"), kind),
        ],
        serde_json::json!({ "message": message }).to_string(),
    )
        .into_response()
}
