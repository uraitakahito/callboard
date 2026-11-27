//! 試験の道具: castlist・pagetray の代わりに、決めた答えを返す偽の server を process の中に立て、callboard の Router を直に呼ぶ。
// 試験ごとに使う物が違う。
#![allow(dead_code)]

pub mod castlist;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get as route;
use callboard::config::{Base, Settings};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// 偽の server が受けた要求 1 つ。
#[derive(Clone, Debug)]
pub struct Seen {
    pub method: String,
    /// path と引数（`/api/graph?label=…`）。
    pub uri: String,
    pub headers: HeaderMap,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }
}

/// 偽の castlist か pagetray。127.0.0.1 の空いた port で待ち、受けた要求を残す。落とすと止まる。
pub struct Fake {
    pub addr: SocketAddr,
    seen: Arc<Mutex<Vec<Seen>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Fake {
    /// この道で起こす。
    pub async fn start(routes: Router) -> Self {
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let record = seen.clone();
        let routes = routes.layer(middleware::from_fn(move |request: Request, next: Next| {
            record.lock().unwrap().push(Seen {
                method: request.method().to_string(),
                uri: request.uri().to_string(),
                headers: request.headers().clone(),
            });
            next.run(request)
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, routes).await.unwrap();
        });
        Self { addr, seen, task }
    }

    /// 聞く先（`http://127.0.0.1:<port>`）。
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// 受けた要求（受けた順）。
    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

/// だれも待っていない所（つないでも断られる）。
pub async fn nowhere() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

pub fn json(status: StatusCode, body: &str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_owned(),
    )
        .into_response()
}

/// castlist の `/healthz` の答え。
pub fn castlist_health(version: &str) -> Response {
    json(
        StatusCode::OK,
        &serde_json::json!({"status": "ok", "version": version, "now": "2026-11-27T00:00:00Z"})
            .to_string(),
    )
}

/// pagetray の `/healthz` の答え。
pub fn pagetray_health(version: &str) -> Response {
    json(
        StatusCode::OK,
        &serde_json::json!({"status": "ok", "version": version}).to_string(),
    )
}

/// 生きている偽の castlist（版 1.0.0）と pagetray（版 1.1.1）の道。
pub fn castlist_routes() -> Router {
    Router::new().route("/healthz", route(|| async { castlist_health("1.0.0") }))
}

pub fn pagetray_routes() -> Router {
    Router::new().route("/healthz", route(|| async { pagetray_health("1.1.1") }))
}

/// link の頭（試験の値。聞く先と違う所にして、取り違えを見分ける）。
pub const PAGETRAY_LINK: &str = "http://127.0.0.1:18801";

pub fn settings(castlist: &str, pagetray: &str) -> Settings {
    Settings {
        listen: "127.0.0.1:0".to_owned(),
        castlist: castlist.to_owned(),
        pagetray: Base {
            api: pagetray.to_owned(),
            link: PAGETRAY_LINK.to_owned(),
        },
    }
}

/// 偽の castlist と pagetray と、それを呼ぶ callboard の Router。
pub struct App {
    pub castlist: Fake,
    pub pagetray: Fake,
    pub router: Router,
}

impl App {
    /// 生きている偽の 2 つで起こす。
    pub async fn start() -> Self {
        Self::start_with(
            castlist_routes(),
            pagetray_routes(),
            callboard::Config::default(),
        )
        .await
    }

    pub async fn start_with(castlist: Router, pagetray: Router, config: callboard::Config) -> Self {
        let castlist = Fake::start(castlist).await;
        let pagetray = Fake::start(pagetray).await;
        let router = callboard::app_with(&settings(&castlist.url(), &pagetray.url()), config);
        Self {
            castlist,
            pagetray,
            router,
        }
    }

    pub async fn send(&self, request: Request) -> (StatusCode, HeaderMap, String) {
        send(&self.router, request).await
    }

    /// `GET` の答えの本文（200 のはず）。
    pub async fn html(&self, path: &str) -> String {
        let (status, _, body) = self.send(get(path)).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        body
    }
}

pub async fn send(router: &Router, request: Request) -> (StatusCode, HeaderMap, String) {
    let response = router.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    (
        parts.status,
        parts.headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

/// host の browser からの `GET`。
pub fn get(path: &str) -> Request {
    with_host("GET", path, "127.0.0.1:8803")
}

pub fn with_host(method: &str, path: &str, host: &str) -> Request {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, host)
        .body(Body::empty())
        .unwrap()
}

/// 誤りの答えの型の名（header `X-Amzn-Errortype`）。
pub fn error_type(headers: &HeaderMap) -> &str {
    headers
        .get("x-amzn-errortype")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

/// 少し待ってから答える道（時間切れの試験）。
pub fn slow(wait: Duration, answer: fn() -> Response) -> Router {
    Router::new().route(
        "/healthz",
        route(move || async move {
            tokio::time::sleep(wait).await;
            answer()
        }),
    )
}

/// どの画面も `<head>` で読む、字の差し替えの script 4 つ（AI-50。i18next・訳 2 つ・i18n.js）を外した HTML。
/// 4 つが `<head>` に、この順で 1 つずつ、属性は src だけで中身の無い tag（`<script src="…"></script>`）であることも見る
/// （無ければ落とす）。画面の中の script を数える試験が使う。
#[allow(dead_code)]
pub fn without_i18n_scripts(body: &str) -> String {
    let head = &body[..body.find("</head>").expect("</head> が無い")];
    let mut out = body.to_owned();
    let mut last = 0;
    // (src の頭, ?v= の後に 12 桁の 16 進が続くか)
    for (src, versioned) in [
        ("/static/vendor/i18next-27.2.9.min.js", false),
        ("/static/locales/ja.js?v=", true),
        ("/static/locales/en.js?v=", true),
        ("/static/i18n.js?v=", true),
    ] {
        let prefix = format!("<script src=\"{src}");
        let at = head
            .find(&prefix)
            .unwrap_or_else(|| panic!("{prefix} が head に無い: {head}"));
        assert!(at >= last, "{prefix} の順が違う: {head}");
        last = at;
        assert_eq!(body.matches(&prefix).count(), 1, "{prefix} が 1 つでない");
        let rest = &head[at + prefix.len()..];
        let digits = if versioned { 12 } else { 0 };
        assert!(
            rest[..digits].bytes().all(|b| b.is_ascii_hexdigit()),
            "{}",
            &rest[..digits]
        );
        assert!(
            rest[digits..].starts_with("\"></script>"),
            "{prefix} の tag が src だけの中身の無い形でない: {}",
            &rest[..rest.len().min(80)]
        );
        let tag = format!("{prefix}{}\"></script>", &rest[..digits]);
        let start = out.find(&tag).unwrap();
        out.replace_range(start..start + tag.len(), "");
    }
    out
}
