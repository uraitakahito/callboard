//! 相関図（`GET /graph/network`）と、その script が点と線を引く道（`GET /api/castlist/graph`）。castlist の画面から移した物。
//!
//! script を使う唯一の画面。HTML は枠だけで、点と線は `static/network.js` が、castlist の図の API（GetGraph）を
//! callboard が中継する道から引いて描く（browser は castlist に直には聞かない。CSP の `connect-src 'self'`）。
use std::sync::Arc;

use askama::Template;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, RawQuery, State};
use axum::http::StatusCode;
use axum::response::Response;
use time::OffsetDateTime;

use super::params::{self, GRAPH_PERIOD, GraphForm, PageError, Pairs};
use super::{CSS_HREF, CYTOSCAPE_PATH, NETWORK_JS_HREF, Nav, error_page, render};
use crate::App;

/// script が点と線を引く道。castlist の `GET /api/graph` を、引数ごと中継する。
pub const DATA_PATH: &str = "/api/castlist/graph";

#[derive(Template)]
#[template(path = "network.html")]
struct NetworkPage<'a> {
    css: &'static str,
    nav: &'a Nav,
    cytoscape: &'static str,
    script: &'static str,
    form: GraphForm,
    quick: Vec<(&'static str, String)>,
    /// network.js が点と線を引く URL（同じ絞りの [`DATA_PATH`]）。
    data_src: String,
    /// script を切った browser に勧める、同じ絞りのタイムライン。
    timeline: String,
}

/// `GET /graph/network?label=…&from=…&to=…`: 相関図（空の値は付けなかったのと同じ）。
/// ここでは引数を確かめ（誤りは 400 の画面）、同じ絞りの中継の道の URL を `data-src` に置く。castlist には聞かない。
pub async fn network(
    State(app): State<Arc<App>>,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Response {
    match network_page(&app.nav, q) {
        Ok(page) => render(StatusCode::OK, &page),
        Err(PageError(status, message)) => error_page(&app.nav, status, &message),
    }
}

fn network_page(
    nav: &Nav,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Result<NetworkPage<'_>, PageError> {
    let mut pairs = params::pairs(q)?;
    pairs.retain(|(_, v)| !v.is_empty());
    params::period(&mut pairs, GRAPH_PERIOD)?;
    let form = GraphForm::new(&pairs);
    let quick = params::quick_links(
        "/graph/network",
        &pairs,
        GRAPH_PERIOD,
        OffsetDateTime::now_utc(),
    );
    // 確かめた引数を、そのままの順でタイムラインと中継の道の URL に付ける（中継の道には、期間を UTC の字にして）。
    let with = |pairs: &[(String, String)]| match params::query(pairs) {
        q if q.is_empty() => q,
        q => format!("?{q}"),
    };
    let api = params::graph_query(&pairs)?;
    Ok(NetworkPage {
        css: &CSS_HREF,
        nav,
        cytoscape: CYTOSCAPE_PATH,
        script: &NETWORK_JS_HREF,
        data_src: format!("{DATA_PATH}{}", with(&api)),
        timeline: format!("{}{}", nav.timeline, with(&pairs)),
        form,
        quick,
    })
}

/// `GET /api/castlist/graph?…`: castlist の図の点と線（GetGraph）の答えを、そのまま返す。引数は castlist が確かめる。
pub async fn graph(State(app): State<Arc<App>>, RawQuery(query): RawQuery) -> Response {
    app.castlist.graph(query.as_deref()).await
}
