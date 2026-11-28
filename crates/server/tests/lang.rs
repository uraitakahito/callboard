//! 呼ぶ先（castlist・pagetray）への頼みに付ける言語（`Accept-Language`）。画面の言語（cookie `callboard-lang` → browser の
//! 言語の並びの初めの 1 つ → 日本語。`static/i18n.js` と同じ順）を、`ja` か `en` に直して伝える。呼ぶ先は、断った訳の字を
//! その言語で返す（castlist 2.2.0・pagetray 2.1.0 から）。言語の決め方の細かい所は `src/lang.rs` の単体の試験が見る。
mod common;

use axum::body::Body;
use axum::extract::Request;
use axum::http::header;
use common::{App, Fake, get};

/// 偽の呼ぶ先が受けた要求のうち、path が `prefix` で始まる物の `Accept-Language`（受けた順）。
fn asked(fake: &Fake, prefix: &str) -> Vec<Option<String>> {
    fake.seen()
        .iter()
        .filter(|seen| seen.uri.starts_with(prefix))
        .map(|seen| seen.header("accept-language").map(str::to_owned))
        .collect()
}

fn with(mut request: Request, name: header::HeaderName, value: &'static str) -> Request {
    request.headers_mut().insert(name, value.parse().unwrap());
    request
}

fn ja() -> Vec<Option<String>> {
    vec![Some("ja".to_owned())]
}

fn en() -> Vec<Option<String>> {
    vec![Some("en".to_owned())]
}

#[tokio::test]
async fn 言語の印が無い_browser_には_日本語を頼む() {
    let app = App::start().await;
    app.send(get("/actors/")).await;
    app.send(get("/pages/")).await;
    assert_eq!(asked(&app.castlist, "/api/actors"), ja());
    assert!(!asked(&app.pagetray, "/api/").is_empty());
    for language in asked(&app.pagetray, "/api/") {
        assert_eq!(language.as_deref(), Some("ja"));
    }
}

#[tokio::test]
async fn 英語の_browser_には_英語を頼む() {
    let app = App::start().await;
    app.send(with(
        get("/actors/"),
        header::ACCEPT_LANGUAGE,
        "en-US,en;q=0.9,ja;q=0.8",
    ))
    .await;
    assert_eq!(asked(&app.castlist, "/api/actors"), en());
}

#[tokio::test]
async fn cookie_で選んだ言語が_browser_の言語より先() {
    let app = App::start().await;
    let request = with(
        with(get("/actors/"), header::ACCEPT_LANGUAGE, "ja"),
        header::COOKIE,
        "callboard-lang=en",
    );
    app.send(request).await;
    assert_eq!(asked(&app.castlist, "/api/actors"), en());
    // 日本語を選んだ英語の browser。
    let app = App::start().await;
    let request = with(
        with(get("/pages/"), header::ACCEPT_LANGUAGE, "en"),
        header::COOKIE,
        "callboard-lang=ja",
    );
    app.send(request).await;
    assert!(!asked(&app.pagetray, "/api/").is_empty());
    for language in asked(&app.pagetray, "/api/") {
        assert_eq!(language.as_deref(), Some("ja"));
    }
}

#[tokio::test]
async fn 呼ぶ先には_ja_か_en_だけを渡し_browser_の字はそのまま渡さない() {
    let app = App::start().await;
    app.send(with(
        get("/actors/"),
        header::ACCEPT_LANGUAGE,
        "fr-CH, fr;q=0.9, en;q=0.8",
    ))
    .await;
    // 初めの 1 つが ja・en でなければ日本語（画面の字と同じ決め方）。
    assert_eq!(asked(&app.castlist, "/api/actors"), ja());
}

#[tokio::test]
async fn 相関図の中継も_言語を伝える() {
    let app = App::start().await;
    app.send(with(
        get("/api/castlist/graph?label=a"),
        header::COOKIE,
        "callboard-lang=en",
    ))
    .await;
    assert_eq!(asked(&app.castlist, "/api/graph"), en());
    app.send(get("/api/castlist/graph?label=a")).await;
    assert_eq!(
        asked(&app.castlist, "/api/graph"),
        [en(), ja()].concat(),
        "言語は要求ごとに決める"
    );
}

#[tokio::test]
async fn file_を上げる中継も_言語を伝える() {
    let app = App::start().await;
    let upload = |cookie: &'static str| {
        Request::builder()
            .method("POST")
            .uri("/api/pagetray/projects/p/keys/A-1/files?name=a.png")
            .header(header::HOST, "127.0.0.1:8803")
            .header(header::ORIGIN, "http://127.0.0.1:8803")
            .header("sec-fetch-site", "same-origin")
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::COOKIE, cookie)
            .body(Body::from("x"))
            .unwrap()
    };
    app.send(upload("callboard-lang=en")).await;
    app.send(upload("a=b")).await;
    assert_eq!(
        asked(&app.pagetray, "/api/projects/p/keys/A-1/files"),
        [en(), ja()].concat()
    );
}

#[tokio::test]
async fn 入口の画面が呼ぶ先の様子を聞く時も_言語を付ける() {
    let app = App::start().await;
    app.send(with(get("/"), header::COOKIE, "callboard-lang=en"))
        .await;
    assert_eq!(asked(&app.castlist, "/healthz"), en());
    assert_eq!(asked(&app.pagetray, "/healthz"), en());
}
