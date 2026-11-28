//! 画面: 入口（`/`）・アクターの一覧（`/actors/`）・プロフィール（`/actors/{id}`）・タイムライン（`/graph/timeline`）・
//! 相関図（`/graph/network`）・使用量（`/usage/`）・メッセージの一覧（`/messages`）・ページの一覧（`/pages/`）と、それが読む CSS
//! （`/static/site.css`）と script（`/static/network.js` と取り込んだ cytoscape、`/static/messages.js`、`/static/pages.js`、取り込んだ i18next）。
//!
//! - サーバーが HTML を組んで返す（askama。`templates/`）。字はみな HTML として逃がす。
//!   script を使うのは相関図・メッセージの一覧・ページの一覧だけで、callboard が配る file だけを読む（inline の script は無い）。
//! - CSP（[`crate::CSP`]）が inline の script と style を断るので、style は callboard が配る CSS の file だけ。
//! - ページの本文と file の中身は pagetray が持つ。ページの一覧の、本文の枠と file の link は、pagetray の link の頭
//!   （[`crate::config`]）で組む（[`pages`]）。
mod exchanges;
pub(crate) mod i18n;
mod list;
mod messages;
mod network;
mod pages;
mod params;
mod profile;
mod thread;
mod time;
mod timeline;
mod timeline_layout;
mod usage;
mod usage_chart;
mod usage_labels;
mod usage_summary;

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
use i18n::{Text, filters};

pub use pages::csp as pages_csp;

// --- CSS と script ---

// 配る file は repo の static/ の物を、binary に埋め込む。
const SITE_CSS: &str = include_str!("../../../static/site.css");

/// 中身の写しの頭を `?v=` に付けた path。中身が替われば browser は読み直す（長く持たせてよい）。
pub(crate) fn versioned(path: &str, body: &str) -> String {
    let digest = Sha256::digest(body.as_bytes());
    let v: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!("{path}?v={v}")
}

/// browser の tab に出す印（掲示の板）。画面が名指すので、browser は `/favicon.ico` を取りに来ない。
const ICON_SVG: &str = include_str!("../../../static/icon.svg");

/// 相関図（`/graph/network`）が使う cytoscape（npm の物をそのまま取り込んだ。static/vendor/SOURCES.md）。
const CYTOSCAPE_JS: &str = include_str!("../../../static/vendor/cytoscape-3.35.0.min.js");

/// cytoscape の path。版が名に入るので、中身が替われば path も替わる。
const CYTOSCAPE_PATH: &str = "/static/vendor/cytoscape-3.35.0.min.js";

/// 相関図を描く、callboard の script。
const NETWORK_JS: &str = include_str!("../../../static/network.js");

/// メッセージの一覧（`/messages`）の、新着の知らせの script。
const MESSAGES_JS: &str = include_str!("../../../static/messages.js");

static MESSAGES_JS_HREF: LazyLock<String> =
    LazyLock::new(|| versioned("/static/messages.js", MESSAGES_JS));
/// ページの一覧（`/pages/`）の script（新着の知らせ・未読の印・file を上げる枠）。
const PAGES_JS: &str = include_str!("../../../static/pages.js");

static PAGES_JS_HREF: LazyLock<String> = LazyLock::new(|| versioned("/static/pages.js", PAGES_JS));
static CSS_HREF: LazyLock<String> = LazyLock::new(|| versioned("/static/site.css", SITE_CSS));
static NETWORK_JS_HREF: LazyLock<String> =
    LazyLock::new(|| versioned("/static/network.js", NETWORK_JS));

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

/// `GET /static/icon.svg`: browser の tab に出す印。path に中身の写しを付けないので、長くは持たせない。
async fn icon_svg() -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        ICON_SVG,
    )
        .into_response()
}

/// `GET /static/vendor/cytoscape-3.35.0.min.js`: 相関図が使う cytoscape。
async fn cytoscape_js() -> Response {
    asset("application/javascript; charset=utf-8", CYTOSCAPE_JS)
}

/// `GET /static/vendor/i18next-27.2.9.min.js`: 画面の字を差し替える i18next。
async fn i18next_js() -> Response {
    asset("application/javascript; charset=utf-8", i18n::I18NEXT_JS)
}

/// `GET /static/i18n.js`: 画面の字を差し替える script。
async fn i18n_js() -> Response {
    asset("application/javascript; charset=utf-8", i18n::I18N_JS)
}

/// `GET /static/locales/ja.js`: 日本語の字（i18next の JSON を、script に包んだ物）。
async fn locales_ja_js() -> Response {
    asset(
        "application/javascript; charset=utf-8",
        i18n::JA_JS.as_str(),
    )
}

/// `GET /static/locales/en.js`: 英語の字。
async fn locales_en_js() -> Response {
    asset(
        "application/javascript; charset=utf-8",
        i18n::EN_JS.as_str(),
    )
}

/// `GET /static/messages.js`: メッセージの一覧の、新着の知らせの script。
async fn messages_js() -> Response {
    asset("application/javascript; charset=utf-8", MESSAGES_JS)
}

/// `GET /static/pages.js`: ページの一覧の script。
async fn pages_js() -> Response {
    asset("application/javascript; charset=utf-8", PAGES_JS)
}

/// `GET /static/network.js`: 相関図を描く script。
async fn network_js() -> Response {
    asset("application/javascript; charset=utf-8", NETWORK_JS)
}

// --- 画面に出す形 ---

/// 種類の印。人（Product Manager）にだけ「人」を付け、AI には付けない（アクターはほとんど AI なので。Product Manager の頼み）。
fn kind_name(kind: &str) -> Option<Text> {
    (kind == "human").then(|| Text::new("common.kind.human"))
}

/// どの画面の頭にも出す、画面の間の link。castlist と pagetray の画面は、みな callboard に移した。
pub struct Nav {
    actors: String,
    timeline: String,
    network: String,
    usage: String,
    pages: String,
    messages: String,
}

impl Nav {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            // castlist の 4 つの画面は、みな callboard に移した。
            actors: list::PATH.to_owned(),
            // タイムラインも callboard に移した。
            timeline: "/graph/timeline".to_owned(),
            // 相関図は callboard に移した。
            network: "/graph/network".to_owned(),
            // 使用量（トークンの数と額。castlist 2.1.0 の ListUsage から）。
            usage: usage::PATH.to_owned(),
            // ページの一覧も callboard に移した。
            pages: pages::PATH.to_owned(),
            // メッセージの一覧は callboard に移した。
            messages: messages::PATH.to_owned(),
        }
    }
}

/// 入口に出す、呼ぶ先 1 つ。
struct Service<'a> {
    /// 呼ぶ先の名（`castlist`・`pagetray`）。
    id: &'static str,
    title: Text,
    /// つながれば、呼ぶ先の版。
    version: Option<String>,
    /// つながらない訳。
    reason: Option<Text>,
    /// その呼ぶ先の画面（名と URL）。
    links: Vec<(Text, &'a str)>,
}

impl<'a> Service<'a> {
    fn new(
        id: &'static str,
        title: &'static str,
        health: Result<Health, Text>,
        links: Vec<(&'static str, &'a str)>,
    ) -> Self {
        let (version, reason) = match health {
            Ok(health) => (Some(health.version), None),
            Err(reason) => (None, Some(reason)),
        };
        let title = Text::new(title);
        let links = links
            .into_iter()
            .map(|(name, href)| (Text::new(name), href))
            .collect();
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
    message: &'a Text,
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
fn error_page(nav: &Nav, status: StatusCode, message: &Text) -> Response {
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
        .route(list::PATH, get(list::list))
        .route("/actors/{id}", get(profile::profile))
        .route("/graph/timeline", get(timeline::timeline))
        .route("/graph/network", get(network::network))
        .route(network::DATA_PATH, get(network::graph))
        .route(usage::PATH, get(usage::usage))
        .route(messages::PATH, get(messages::messages))
        .route(messages::EVENTS_PATH, get(messages::events))
        .route(thread::ROUTE, get(thread::thread))
        .route(pages::PATH, get(pages::pages))
        .route("/pages", get(pages::redirect))
        .route(pages::UPLOAD_ROUTE, axum::routing::post(pages::upload))
        .route("/static/site.css", get(site_css))
        .route("/static/icon.svg", get(icon_svg))
        .route("/static/network.js", get(network_js))
        .route("/static/messages.js", get(messages_js))
        .route("/static/pages.js", get(pages_js))
        .route(CYTOSCAPE_PATH, get(cytoscape_js))
        .route(i18n::I18NEXT_PATH, get(i18next_js))
        .route("/static/i18n.js", get(i18n_js))
        .route("/static/locales/ja.js", get(locales_ja_js))
        .route("/static/locales/en.js", get(locales_en_js))
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
                "nav.actors",
                castlist,
                vec![
                    ("home.castlist.actors", &nav.actors),
                    ("nav.timeline", &nav.timeline),
                    ("nav.network", &nav.network),
                    ("nav.usage", &nav.usage),
                ],
            ),
            Service::new(
                "pagetray",
                "nav.pages",
                pagetray,
                vec![
                    ("home.pagetray.pages", &nav.pages),
                    ("nav.messages", &nav.messages),
                ],
            ),
        ],
    };
    render(StatusCode::OK, &page)
}

async fn not_found(State(app): State<Arc<App>>) -> Response {
    error_page(
        &app.nav,
        StatusCode::NOT_FOUND,
        &Text::new("error.noSuchScreen"),
    )
}
