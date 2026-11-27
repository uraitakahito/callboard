//! 守りの層の振る舞い（castlist・pagetray の守りと同じ決まり）。どの道（画面・配る file・/healthz・無い道）の前でも効く。
//! - 断る答えは restJson1 の形（header `X-Amzn-Errortype` と、本文の JSON の `message`）。
//!   種類は `Forbidden`（403）・`PayloadTooLarge`（413）・`Unavailable`（503）。
//! - Host と Origin・HEAD（GET と同じに答え、本文を捨てる）・本文の上限・時間切れ・どの答えにも付ける header。
mod common;

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Request, StatusCode, header};
use common::{App, error_type, get, with_host};
use serde_json::Value;

/// 断る答えの形: 種類の header・JSON の本文・`message` の字。`message` を返す。
fn refused(headers: &HeaderMap, body: &str, kind: &str) -> String {
    assert_eq!(error_type(headers), kind, "{body}");
    assert_eq!(
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/json"),
        "{body}"
    );
    let json: Value = serde_json::from_str(body).unwrap_or_else(|e| panic!("{e}: {body}"));
    let message = json["message"].as_str().unwrap_or_else(|| panic!("{body}"));
    assert!(!message.is_empty());
    message.to_owned()
}

#[tokio::test]
async fn 外の_host_は_どの道でも_forbidden_の形で断り_呼ぶ先に聞かない() {
    let app = App::start().await;
    for path in ["/healthz", "/", "/static/site.css", "/no/such"] {
        let (status, headers, body) = app.send(with_host("GET", path, "evil.example")).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
        let message = refused(&headers, &body, "Forbidden");
        assert!(message.contains("Host"), "{path}: {message}");
        // 断った答えにも、型の推し量りを止める header が付く（一番外）。
        assert_eq!(
            headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );
    }
    // Host の無い要求も断る。
    let request = Request::get("/healthz").body(Body::empty()).unwrap();
    let (status, headers, body) = app.send(request).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    refused(&headers, &body, "Forbidden");
    assert!(app.castlist.seen().is_empty());
    assert!(app.pagetray.seen().is_empty());
}

#[tokio::test]
async fn localhost_と_127_0_0_1_は_port_を問わず受ける() {
    let app = App::start().await;
    for host in ["localhost:9999", "127.0.0.1", "127.0.0.1:8803", "localhost"] {
        let (status, _, body) = app.send(with_host("GET", "/healthz", host)).await;
        assert_eq!(status, StatusCode::OK, "{host}: {body}");
    }
}

#[tokio::test]
async fn origin_の付いた書く要求は_forbidden_の形で断り_読む要求は通す() {
    let app = App::start().await;
    for method in ["POST", "PUT", "DELETE", "PATCH"] {
        let request = Request::builder()
            .method(method)
            .uri("/")
            .header(header::HOST, "127.0.0.1:8803")
            .header(header::ORIGIN, "http://127.0.0.1:8803")
            .body(Body::empty())
            .unwrap();
        let (status, headers, body) = app.send(request).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method}: {body}");
        let message = refused(&headers, &body, "Forbidden");
        assert!(message.contains("browser"), "{message}");
    }
    let request = Request::get("/healthz")
        .header(header::HOST, "127.0.0.1:8803")
        .header(header::ORIGIN, "http://127.0.0.1:8803")
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.send(request).await.0, StatusCode::OK);
}

#[tokio::test]
async fn head_は_get_と同じ状態と_header_で_本文なし() {
    let app = App::start().await;
    // 3 つ目は、本文に今の時刻（/healthz の now）が入るか。時刻の字は長さが替わりうるので、その道では Content-Length が
    // 有ることだけを見て、同じ長さは時刻の無い道で見る。
    for (path, content_type, timed) in [
        ("/healthz", "application/json", true),
        ("/", "text/html; charset=utf-8", false),
        ("/static/site.css", "text/css; charset=utf-8", false),
    ] {
        let (status, headers, body) = app.send(with_host("HEAD", path, "127.0.0.1:8803")).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        assert!(body.is_empty(), "{path}: {body}");
        let (_, get_headers, get_body) = app.send(get(path)).await;
        assert!(!get_body.is_empty(), "{path}");
        if timed {
            assert!(headers.get(header::CONTENT_LENGTH).is_some(), "{path}");
        } else {
            assert_eq!(
                headers.get(header::CONTENT_LENGTH).unwrap(),
                &get_body.len().to_string(),
                "{path}"
            );
            assert_eq!(
                headers.get(header::CONTENT_LENGTH),
                get_headers.get(header::CONTENT_LENGTH),
                "{path}"
            );
        }
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            content_type,
            "{path}"
        );
    }
    // 無い道の HEAD も、GET と同じ 404。
    for method in ["HEAD", "GET"] {
        let (status, _, _) = app
            .send(with_host(method, "/no/such", "127.0.0.1:8803"))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method}");
    }
}

#[test]
fn 時間切れは_5_秒_本文の上限は_2_mib_で_呼ぶ先を待つ長さは_時間切れより短い() {
    assert_eq!(callboard::REQUEST_TIMEOUT, Duration::from_secs(5));
    assert_eq!(callboard::BODY_LIMIT, 2 * 1024 * 1024);
    assert_eq!(
        callboard::Config::default().timeout,
        callboard::REQUEST_TIMEOUT
    );
    // 等しいと、呼ぶ先が答えない時に守りの層の時間切れが先に答え、どこが答えないかを画面に書けない。
    assert!(callboard::upstream::TIMEOUT < callboard::REQUEST_TIMEOUT);
    assert!(callboard::upstream::HEALTH_TIMEOUT < callboard::upstream::TIMEOUT);
}

fn post_bytes(path: &str, body: Body) -> Request<Body> {
    Request::post(path)
        .header(header::HOST, "127.0.0.1:8803")
        .body(body)
        .unwrap()
}

#[tokio::test]
async fn 上限までの本文は読み_超えた本文は_payload_too_large_の形で断る() {
    let app = App::start().await;
    // callboard に書く道はまだ無いので、上限の内の本文は、道が GET だけだという 405 まで届く。
    let body = vec![b' '; callboard::BODY_LIMIT];
    let (status, _, answer) = app.send(post_bytes("/healthz", Body::from(body))).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{answer}");

    let body = vec![b' '; callboard::BODY_LIMIT + 1];
    let (status, headers, answer) = app.send(post_bytes("/healthz", Body::from(body))).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{answer}");
    let message = refused(&headers, &answer, "PayloadTooLarge");
    assert!(message.contains("2 MiB"), "{message}");
}

/// 長さを前に言わない本文（chunked で送られた物と同じ）。
struct Chunks(VecDeque<Bytes>);

impl http_body::Body for Chunks {
    type Data = Bytes;
    type Error = std::convert::Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Bytes>, Self::Error>>> {
        Poll::Ready(self.0.pop_front().map(|b| Ok(http_body::Frame::data(b))))
    }
}

#[tokio::test]
async fn 長さを言わない本文も_上限を超えれば断る() {
    let app = App::start().await;
    let body = Bytes::from(vec![b' '; callboard::BODY_LIMIT + 1]);
    let chunks = Chunks(
        (0..body.len())
            .step_by(64 * 1024)
            .map(|at| body.slice(at..(at + 64 * 1024).min(body.len())))
            .collect(),
    );
    let (status, headers, answer) = app.send(post_bytes("/healthz", Body::new(chunks))).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{answer}");
    refused(&headers, &answer, "PayloadTooLarge");
}

#[tokio::test]
async fn 時間切れを過ぎた問い合わせは_unavailable_の形で返す() {
    // 試験では時間切れを短くし、偽の castlist の答えを遅らせる（入口の画面は、呼ぶ先を 1 秒まで待つ。その前に時間切れになる）。
    let app = App::start_with(
        common::slow(Duration::from_secs(3), || common::castlist_health("1.0.0")),
        common::pagetray_routes(),
        callboard::Config {
            timeout: Duration::from_millis(200),
        },
    )
    .await;
    let started = Instant::now();
    let (status, headers, body) = app.send(get("/")).await;
    let took = started.elapsed();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let message = refused(&headers, &body, "Unavailable");
    assert!(message.contains("時間がかかりすぎた"), "{message}");
    assert!(
        took < Duration::from_millis(900),
        "呼ぶ先を待つ長さ（1 秒）より前に答える: {took:?}"
    );
}

#[tokio::test]
async fn どの答えにも_nosniff_と_no_referrer_を付け_画面には_csp_を付ける() {
    let app = App::start().await;
    for (path, html) in [
        ("/", true),
        ("/no/such", true),
        ("/static/site.css", false),
        ("/healthz", false),
    ] {
        let (_, headers, _) = app.send(get(path)).await;
        assert_eq!(
            headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff",
            "{path}"
        );
        assert_eq!(
            headers.get(header::REFERRER_POLICY).unwrap(),
            "no-referrer",
            "{path}"
        );
        let csp = headers
            .get(header::CONTENT_SECURITY_POLICY)
            .map(|v| v.to_str().unwrap());
        if html {
            assert_eq!(
                csp,
                Some(
                    "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
                ),
                "{path}"
            );
        } else {
            assert_eq!(csp, None, "{path}");
        }
    }
}
