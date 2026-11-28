//! callboard の server: castlist（アクターの台帳）と pagetray（ページの置き場）の画面をまとめる。
//!
//! - castlist と pagetray は API を通して使う（[`upstream`]。Smithy の定義から作った client）。DB は持たない。
//! - 入口には、castlist・pagetray と同じ守りの層（[`guard`]）を掛ける。
//! - 画面（[`ui`]）は、サーバーが HTML を組んで返す。
pub mod config;
mod errors;
mod guard;
mod lang;
mod ui;
pub mod upstream;

/// 画面に出す字（鍵と値）。呼ぶ先に聞けなかった訳（[`upstream::Failure`]）も、この形で持つ。
pub use ui::i18n::Text;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::header;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

/// 止める合図を受けた印。付いたら、流し続ける答え（新着の知らせの中継）を閉じる。
pub use tokio_util::sync::CancellationToken as Closing;

use crate::config::Settings;
use crate::upstream::castlist::Castlist;
use crate::upstream::pagetray::Pagetray;

/// 画面の応答に付ける Content-Security-Policy。script と style は、callboard 自身が配る file だけを許す。
/// ページの一覧だけは、これに pagetray の link の頭を足した物を付ける（本文の枠と、file の図。`ui::pages`）。
/// form は callboard 自身へだけ送れる（castlist の画面の CSP と同じ字）。
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";

/// 1 つの問い合わせにかけてよい長さ。過ぎれば 503 `Unavailable` で返す（castlist・pagetray と同じ）。
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// 本文の上限（axum の既定と同じ 2 MiB）。超えれば 413 `PayloadTooLarge` で返す。
pub const BODY_LIMIT: usize = 2 * 1024 * 1024;

/// file を上げる道だけの、本文の上限（pagetray が file 1 つに受ける大きさと同じ 16 MiB）。
pub const UPLOAD_LIMIT: usize = 16 * 1024 * 1024;

/// 守りの層の設定。
#[derive(Clone)]
pub struct Config {
    /// 1 つの問い合わせにかけてよい長さ（既定は [`REQUEST_TIMEOUT`]。試験は短くする）。
    pub timeout: Duration,
    /// 新着の知らせの中継で、pagetray の答えの頭を待つ長さ（既定は [`upstream::TIMEOUT`]。試験は短くする）。
    pub events_wait: Duration,
    /// 止める合図を受けた印（binary が付ける）。付いたら、新着の知らせの流れを閉じる。
    pub closing: Closing,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            timeout: REQUEST_TIMEOUT,
            events_wait: upstream::TIMEOUT,
            closing: Closing::new(),
        }
    }
}

/// どの道も使う物: 呼ぶ先と、画面の間の link。
struct App {
    castlist: Castlist,
    pagetray: Pagetray,
    nav: ui::Nav,
    /// ページの一覧の答えに付ける Content-Security-Policy（[`CSP`] に、pagetray の link の頭を足した物）。
    pages_csp: axum::http::HeaderValue,
    /// 新着の知らせの中継で、pagetray の答えの頭を待つ長さ。
    events_wait: Duration,
    /// メッセージを送る側が、返事の先を付け始めた時刻（UNIX のミリ秒。設定 `CALLBOARD_EXCHANGES_SINCE`）。
    exchanges_since: Option<i64>,
    /// 止める合図を受けた印。
    closing: Closing,
}

/// 動かす Router。
pub fn app(settings: &Settings) -> Router {
    app_with(settings, Config::default())
}

/// 動かす Router（守りの層の設定を渡す）。
pub fn app_with(settings: &Settings, config: Config) -> Router {
    let castlist = Castlist::new(&settings.castlist);
    let pagetray = Pagetray::new(&settings.pagetray);
    let nav = ui::Nav::new();
    let pages_csp = ui::pages_csp(pagetray.link());
    let app = Arc::new(App {
        castlist,
        pagetray,
        nav,
        pages_csp,
        events_wait: config.events_wait,
        exchanges_since: settings.exchanges_since,
        closing: config.closing,
    });
    ui::routes(app)
        .route("/healthz", get(healthz))
        // 画面の言語を決めて、呼ぶ先への頼みに付けられるようにする（道のすぐ外。守りの層が通した要求だけ）。
        .layer(middleware::from_fn(lang::scope))
        .layer(middleware::from_fn(guard::limit))
        .layer(middleware::from_fn_with_state(
            config.timeout,
            guard::timeout,
        ))
        .layer(middleware::from_fn(guard::guard))
        // 一番外に置く。guard が断った答えにも付く。
        .layer(middleware::from_fn(guard::stamp))
}

/// `GET /healthz`: callboard 自身が生きているか（castlist・pagetray の `/healthz` と同じ形）。呼ぶ先には聞かない
/// （呼ぶ先が止まっていても、callboard は生きていると答える。呼ぶ先の様子は入口の画面 `/` に出す）。
async fn healthz() -> Response {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let body = serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "now": now,
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}
