//! 画面: 入口（`/`）と、画面が読む CSS（`/static/site.css`）。
//!
//! - サーバーが HTML を組んで返す（askama。`templates/`）。字はみな HTML として逃がす。
//! - CSP（[`crate::CSP`]）が inline の script と style を断るので、style は callboard が配る CSS の file だけ。
//! - まだ callboard に移していない画面への link は、castlist・pagetray の link の頭（[`crate::config`]）で組む。
use std::sync::{Arc, LazyLock};

use askama::Template;
use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use sha2::{Digest, Sha256};

use crate::App;
use crate::upstream::Health;

// --- CSS ---

// 配る file は repo の static/ の物を、binary に埋め込む。
const SITE_CSS: &str = include_str!("../../../static/site.css");

/// 中身の写しの頭を `?v=` に付けた path。中身が替われば browser は読み直す（長く持たせてよい）。
fn versioned(path: &str, body: &str) -> String {
    let digest = Sha256::digest(body.as_bytes());
    let v: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!("{path}?v={v}")
}

static CSS_HREF: LazyLock<String> = LazyLock::new(|| versioned("/static/site.css", SITE_CSS));

/// binary に埋め込んだ file を、長く持たせてよい答えで返す（path が中身ごとに替わるので）。
fn asset(content_type: &'static str, body: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        body,
    )
        .into_response()
}

/// `GET /static/site.css`: 画面の CSS。
async fn site_css() -> Response {
    asset("text/css; charset=utf-8", SITE_CSS)
}

// --- 画面に出す形 ---

/// どの画面の頭にも出す、画面の間の link。まだ移していない画面は、castlist・pagetray の画面を指す。
pub struct Nav {
    actors: String,
    timeline: String,
    network: String,
    pages: String,
    messages: String,
}

impl Nav {
    /// link の頭（`http://<host>:<port>`）から組む。
    pub fn new(castlist: &str, pagetray: &str) -> Self {
        Self {
            actors: format!("{castlist}/"),
            timeline: format!("{castlist}/graph/timeline"),
            network: format!("{castlist}/graph/network"),
            pages: format!("{pagetray}/"),
            messages: format!("{pagetray}/messages"),
        }
    }
}

/// 入口に出す、呼ぶ先 1 つ。
struct Service<'a> {
    /// 呼ぶ先の名（`castlist`・`pagetray`）。
    id: &'static str,
    title: &'static str,
    /// つながれば、呼ぶ先の版。
    version: Option<String>,
    /// つながらない訳。
    reason: String,
    /// その呼ぶ先の画面（名と URL）。
    links: Vec<(&'static str, &'a str)>,
}

impl<'a> Service<'a> {
    fn new(
        id: &'static str,
        title: &'static str,
        health: Result<Health, String>,
        links: Vec<(&'static str, &'a str)>,
    ) -> Self {
        let (version, reason) = match health {
            Ok(health) => (Some(health.version), String::new()),
            Err(reason) => (None, reason),
        };
        Self {
            id,
            title,
            version,
            reason,
            links,
        }
    }
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomePage<'a> {
    css: &'static str,
    nav: &'a Nav,
    services: Vec<Service<'a>>,
}

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorPage<'a> {
    css: &'static str,
    nav: &'a Nav,
    status: u16,
    message: &'a str,
}

fn render(status: StatusCode, page: &impl Template) -> Response {
    match page.render() {
        Ok(html) => (status, Html(html)).into_response(),
        Err(e) => {
            tracing::error!("画面を表示できません: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "画面を表示できません").into_response()
        }
    }
}

/// 誤りの画面。
fn error_page(nav: &Nav, status: StatusCode, message: &str) -> Response {
    render(
        status,
        &ErrorPage {
            css: &CSS_HREF,
            nav,
            status: status.as_u16(),
            message,
        },
    )
}

// --- 道 ---

/// 画面の道。どれも `GET`（`HEAD` も受ける）。無い道は 404 の画面。
pub fn routes(app: Arc<App>) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/static/site.css", get(site_css))
        .fallback(not_found)
        .with_state(app)
}

/// `GET /`: 入口。castlist と pagetray が生きているか（版）と、それぞれの画面への link。
/// 呼ぶ先が答えなくても 200 で返し、届かないことと訳を書く。
async fn home(State(app): State<Arc<App>>) -> Response {
    let (castlist, pagetray) = tokio::join!(app.castlist.health(), app.pagetray.health());
    let nav = &app.nav;
    let page = HomePage {
        css: &CSS_HREF,
        nav,
        services: vec![
            Service::new(
                "castlist",
                "アクター",
                castlist,
                vec![
                    ("アクターの一覧", &nav.actors),
                    ("タイムライン", &nav.timeline),
                    ("相関図", &nav.network),
                ],
            ),
            Service::new(
                "pagetray",
                "ページ",
                pagetray,
                vec![("ページの一覧", &nav.pages), ("メッセージ", &nav.messages)],
            ),
        ],
    };
    render(StatusCode::OK, &page)
}

async fn not_found(State(app): State<Arc<App>>) -> Response {
    error_page(&app.nav, StatusCode::NOT_FOUND, "その画面はありません")
}
