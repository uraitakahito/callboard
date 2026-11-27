//! 入口の画面（`GET /`）と、どの画面にも出す頭の link・誤りの画面・CSS。
mod common;

use std::time::Duration;

use axum::Router;
use axum::http::{Request, StatusCode, header};
use axum::routing::get as route_get;
use common::{App, CASTLIST_LINK, PAGETRAY_LINK, get};

/// `id` の `<section>` の中身。
fn section<'a>(body: &'a str, id: &str) -> &'a str {
    let open = format!("<section id=\"{id}\">");
    let start = body
        .find(&open)
        .unwrap_or_else(|| panic!("{id} の節が無い: {body}"));
    let rest = &body[start..];
    &rest[..rest.find("</section>").expect("節の終わり")]
}

#[tokio::test]
async fn 入口は_castlist_と_pagetray_がつながっていることと版を出す() {
    let app = App::start().await;
    let body = app.html("/").await;
    let castlist = section(&body, "castlist");
    assert!(castlist.contains("つながっています"), "{castlist}");
    assert!(castlist.contains("<code>1.0.0</code>"), "{castlist}");
    let pagetray = section(&body, "pagetray");
    assert!(pagetray.contains("つながっています"), "{pagetray}");
    assert!(pagetray.contains("<code>1.1.1</code>"), "{pagetray}");
    assert!(!body.contains("届きません"), "{body}");
}

#[tokio::test]
async fn 呼ぶ先には_host_127_0_0_1_で聞き_browser_の_header_を渡さない() {
    let app = App::start().await;
    let request = Request::get("/")
        .header(header::HOST, "localhost:8803")
        .header(header::ORIGIN, "http://localhost:8803")
        .header(header::COOKIE, "a=b")
        .header(header::REFERER, "http://localhost:8803/x")
        .body(axum::body::Body::empty())
        .unwrap();
    assert_eq!(app.send(request).await.0, StatusCode::OK);
    for fake in [&app.castlist, &app.pagetray] {
        let seen = fake.seen();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert_eq!(seen[0].method, "GET");
        assert_eq!(seen[0].uri, "/healthz");
        assert_eq!(seen[0].header("host"), Some("127.0.0.1"));
        for name in ["origin", "cookie", "referer"] {
            assert_eq!(seen[0].header(name), None, "{name}");
        }
    }
}

#[tokio::test]
async fn 入口と頭の_link_は_まだ移していない画面を_link_の頭で_移した画面を_callboard_の道で指す() {
    let app = App::start().await;
    let body = app.html("/").await;
    let castlist = section(&body, "castlist");
    for (name, path) in [("アクターの一覧", "/"), ("タイムライン", "/graph/timeline")]
    {
        let link = format!("<a href=\"{CASTLIST_LINK}{path}\">{name}</a>");
        assert!(castlist.contains(&link), "{link}: {castlist}");
    }
    // 相関図は callboard に移したので、callboard の中の道を指す。
    let link = "<a href=\"/graph/network\">相関図</a>";
    assert!(castlist.contains(link), "{castlist}");
    let pagetray = section(&body, "pagetray");
    for (name, path) in [("ページの一覧", "/"), ("メッセージ", "/messages")] {
        let link = format!("<a href=\"{PAGETRAY_LINK}{path}\">{name}</a>");
        assert!(pagetray.contains(&link), "{link}: {pagetray}");
    }
    // 頭の link（どの画面にも出す）。無い道の画面にも出る。
    let (_, _, missing) = app.send(get("/no/such")).await;
    for page in [&body, &missing] {
        let nav = &page[page.find("<nav").unwrap()..page.find("</nav>").unwrap()];
        for link in [
            format!("<a href=\"{CASTLIST_LINK}/\">アクター</a>"),
            format!("<a href=\"{CASTLIST_LINK}/graph/timeline\">タイムライン</a>"),
            "<a href=\"/graph/network\">相関図</a>".to_owned(),
            format!("<a href=\"{PAGETRAY_LINK}/\">ページ</a>"),
            format!("<a href=\"{PAGETRAY_LINK}/messages\">メッセージ</a>"),
        ] {
            assert!(nav.contains(&link), "{link}: {nav}");
        }
        assert!(page.contains("<a class=\"home\" href=\"/\">callboard</a>"));
    }
    // 聞く先（container の中の名）は、画面に出さない。
    for fake in [&app.castlist, &app.pagetray] {
        assert!(!body.contains(&fake.url()), "{body}");
    }
}

#[tokio::test]
async fn 呼ぶ先が止まっていても入口は_200_で_届かないことと訳を書く() {
    let down = common::nowhere().await;
    let pagetray = common::Fake::start(common::pagetray_routes()).await;
    let router = callboard::app(&common::settings(&down, &pagetray.url()));
    let (status, _, body) = common::send(&router, get("/")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let castlist = section(&body, "castlist");
    assert!(castlist.contains("届きません"), "{castlist}");
    assert!(castlist.contains("つながりません"), "{castlist}");
    // 聞く先の URL は出さない。
    assert!(!body.contains(&down), "{body}");
    assert!(section(&body, "pagetray").contains("つながっています"));
}

#[tokio::test]
async fn 呼ぶ先の答えが_2xx_でない_読めない_遅い時は_それぞれの訳を書く() {
    let failing =
        || async { common::json(StatusCode::INTERNAL_SERVER_ERROR, r#"{"message":"x"}"#) };
    let app = App::start_with(
        Router::new().route("/healthz", route_get(failing)),
        Router::new().route(
            "/healthz",
            route_get(|| async { common::json(StatusCode::OK, "not json") }),
        ),
        callboard::Config::default(),
    )
    .await;
    let body = app.html("/").await;
    assert!(section(&body, "castlist").contains("500 が返りました"));
    assert!(section(&body, "pagetray").contains("答えを読めません"));

    // 形は JSON でも、status が ok でない・版が無い答えは、読めない物とする。
    let app = App::start_with(
        Router::new().route(
            "/healthz",
            route_get(|| async {
                common::json(StatusCode::OK, r#"{"status":"ng","version":"1"}"#)
            }),
        ),
        Router::new().route(
            "/healthz",
            route_get(|| async { common::json(StatusCode::OK, r#"{"status":"ok"}"#) }),
        ),
        callboard::Config::default(),
    )
    .await;
    let body = app.html("/").await;
    assert!(section(&body, "castlist").contains("答えを読めません"));
    assert!(section(&body, "pagetray").contains("答えを読めません"));

    // 1 秒で答えない呼ぶ先は、待たずに「時間内に答えがありません」と書く。
    let app = App::start_with(
        common::slow(Duration::from_secs(3), || common::castlist_health("1.0.0")),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let started = std::time::Instant::now();
    let body = app.html("/").await;
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(section(&body, "castlist").contains("時間内に答えがありません"));
    assert!(section(&body, "pagetray").contains("つながっています"));
}

#[tokio::test]
async fn 呼ぶ先の字は_html_で逃がす() {
    let app = App::start_with(
        Router::new().route(
            "/healthz",
            route_get(|| async { common::castlist_health("<script>alert(1)</script>&\"") }),
        ),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let body = app.html("/").await;
    assert!(!body.contains("<script"), "{body}");
    // askama は、数字の形で逃がす。
    assert!(
        body.contains("&#60;script&#62;alert(1)&#60;/script&#62;&#38;&#34;"),
        "{body}"
    );
}

#[tokio::test]
async fn 画面は_callboard_が配る_css_だけを読み_inline_の_script_と_style_が無い() {
    let app = App::start().await;
    let (_, _, missing) = app.send(get("/no/such")).await;
    for body in [app.html("/").await, missing] {
        for inline in ["<script", "<style", "style="] {
            assert!(!body.contains(inline), "{inline}: {body}");
        }
        // CSS の path には、中身の写しの頭が付く（中身が替われば path も替わる）。
        let at = body.find("<link rel=\"stylesheet\" href=\"").expect("CSS");
        let href = &body[at + 29..];
        let href = &href[..href.find('"').unwrap()];
        let (path, v) = href.split_once("?v=").expect("?v=");
        assert_eq!(path, "/static/site.css");
        assert!(
            v.len() == 12 && v.bytes().all(|b| b.is_ascii_hexdigit()),
            "{v}"
        );
        let (status, headers, css) = app.send(get(href)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            headers.get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );
        // 明るい色と暗い色の両方を持つ。
        assert!(css.contains("color-scheme: light dark"));
        assert!(css.contains("@media (prefers-color-scheme: dark)"));
    }
}

#[tokio::test]
async fn 画面は_tab_の印を名指し_callboard_が_svg_で配る() {
    let app = App::start().await;
    let (_, _, missing) = app.send(get("/no/such")).await;
    for body in [app.html("/").await, missing] {
        assert!(
            body.contains("<link rel=\"icon\" type=\"image/svg+xml\" href=\"/static/icon.svg\">"),
            "{body}"
        );
    }
    let (status, headers, body) = app.send(get("/static/icon.svg")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "image/svg+xml");
    assert_eq!(
        headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
        "nosniff"
    );
    // 絵だけ（script も、外の物を読む字も無い）。
    assert!(
        body.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""),
        "{body}"
    );
    for word in ["<script", "href", "url(", "style"] {
        assert!(!body.contains(word), "{word}");
    }
}

#[tokio::test]
async fn 無い道は_404_の画面で_入口への_link_を出す() {
    let app = App::start().await;
    for path in [
        "/no/such",
        "/actors/",
        "/api/graph",
        "/api/castlist/actors",
        "/graph/timeline",
        "/static/none.css",
    ] {
        let (status, headers, body) = app.send(get(path)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8",
            "{path}"
        );
        assert!(body.contains("<h1>404</h1>"), "{body}");
        assert!(body.contains("その画面はありません"), "{body}");
        assert!(body.contains("<a href=\"/\">入口へ</a>"), "{body}");
    }
    // 無い道では、呼ぶ先に聞かない。
    assert!(app.castlist.seen().is_empty());
}
