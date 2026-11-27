//! 画面の字の多言語化（AI-50）: 取り込んだ i18next を配る道。
mod common;

use axum::http::{StatusCode, header};
use common::{App, get};

const I18NEXT_PATH: &str = "/static/vendor/i18next-27.2.9.min.js";

#[tokio::test]
async fn i18next_は_callboard_が_application_javascript_で_長く持たせて配る() {
    let app = App::start().await;
    let (status, headers, script) = app.send(get(I18NEXT_PATH)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/javascript; charset=utf-8"
    );
    assert_eq!(
        headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
        "nosniff"
    );
    assert_eq!(
        headers.get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=31536000, immutable"
    );
    assert!(headers.get(header::CONTENT_SECURITY_POLICY).is_none());
    // 配る i18next は、repo に取り込んだ file と同じ字（npm の物と同じかは ci/check-vendor.sh が見る）。
    let vendored = include_str!("../../../static/vendor/i18next-27.2.9.min.js");
    assert!(script == vendored);
}
