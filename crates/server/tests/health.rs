//! `GET /healthz`: callboard 自身が生きているか。
mod common;

use axum::http::{StatusCode, header};
use common::{App, get};
use serde_json::Value;

#[tokio::test]
async fn healthz_は_ok_と版と今の時刻を_json_で返す() {
    let app = App::start().await;
    let (status, headers, body) = app.send(get("/healthz")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    let now = json["now"].as_str().unwrap();
    assert!(
        time::OffsetDateTime::parse(now, &time::format_description::well_known::Rfc3339).is_ok(),
        "{now}"
    );
    assert_eq!(json.as_object().unwrap().len(), 3, "{body}");
}

#[tokio::test]
async fn healthz_は_呼ぶ先に聞かず_呼ぶ先が止まっていても_ok() {
    let app = App::start().await;
    app.send(get("/healthz")).await;
    assert!(app.castlist.seen().is_empty());
    assert!(app.pagetray.seen().is_empty());

    let down = common::nowhere().await;
    let router = callboard::app(&common::settings(&down, &down));
    let (status, _, body) = common::send(&router, get("/healthz")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}
