//! 相関図（`GET /graph/network`）と、その script が点と線を引く道（`GET /api/castlist/graph`）。castlist の画面の試験
//! （castlist の tests/ui.rs の「相関図」と「期間の欄」）を移した物。描いた絵は、browser の試験（e2e/）で見る。
mod common;

use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::routing::get as route_get;
use common::{App, error_type, get, with_host};
use serde_json::Value;

/// 図の答え（偽の castlist が返す。中身は中継の試験で見分けられればよい）。
const GRAPH: &str = r#"{"asOf":"2026-11-22T06:00:00Z","nodes":[],"edges":[]}"#;

fn castlist() -> Router {
    common::castlist_routes().route(
        "/api/graph",
        route_get(|| async { common::json(StatusCode::OK, GRAPH) }),
    )
}

async fn start() -> App {
    App::start_with(
        castlist(),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await
}

/// `#network` の data-src（HTML の逃がしを戻した字）。
fn data_src(body: &str) -> String {
    let at = body.find("data-src=\"").expect("data-src") + 10;
    let rest = &body[at..];
    unescape(&rest[..rest.find('"').unwrap()])
}

fn unescape(s: &str) -> String {
    s.replace("&#38;", "&")
        .replace("&#34;", "\"")
        .replace("&#60;", "<")
        .replace("&#62;", ">")
}

/// 名の付いた link の href（HTML の逃がしを戻した字）。
fn link_to(body: &str, name: &str) -> String {
    let end = body
        .find(&format!("\">{name}</a>"))
        .unwrap_or_else(|| panic!("{name} の link が無い: {body}"));
    let start = body[..end].rfind("href=\"").unwrap() + 6;
    // 字の鍵（data-i18n）が href の後に付くので、href の値は次の `"` まで。
    let end = start + body[start..end].find('"').unwrap_or(end - start);
    unescape(&body[start..end])
}

/// `name` の欄（`<input … name="…" …>`）。
fn input<'a>(body: &'a str, name: &str) -> &'a str {
    let at = body
        .find(&format!("name=\"{name}\""))
        .unwrap_or_else(|| panic!("{name} の欄が無い"));
    let start = body[..at].rfind("<input").unwrap();
    &body[start..start + body[start..].find('>').unwrap()]
}

#[tokio::test]
async fn 相関図は_callboard_が配る_script_だけを読み_inline_の_script_と_style_が無く_castlist_に聞かない()
 {
    let app = start().await;
    let (status, headers, body) = app.send(get("/graph/network")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(headers.get(header::CONTENT_SECURITY_POLICY).is_some());
    assert!(
        body.contains("<title data-i18n=\"network.title\">相関図 ・ callboard</title>"),
        "{body}"
    );
    for inline in ["<style", "style="] {
        assert!(!body.contains(inline), "{inline}");
    }
    // script は src で読む 2 つだけ（cytoscape と network.js。network.js の path には中身の写しの頭が付く）。
    let page = common::without_i18n_scripts(&body);
    let scripts: Vec<&str> = page
        .match_indices("<script")
        .map(|(at, _)| &page[at..at + page[at..].find('>').unwrap() + 1])
        .collect();
    assert_eq!(scripts.len(), 2, "{scripts:?}");
    assert_eq!(
        scripts[0],
        "<script src=\"/static/vendor/cytoscape-3.35.0.min.js\">"
    );
    assert!(
        scripts[1].starts_with("<script src=\"/static/network.js?v=") && scripts[1].len() == 48,
        "{}",
        scripts[1]
    );
    assert_eq!(page.matches("</script>").count(), 2);
    // 点と線は、callboard の中継の道から引く。
    assert_eq!(data_src(&body), "/api/castlist/graph");
    // 画面を組む時には、castlist に聞かない（点と線は script が後で引く）。
    assert!(app.castlist.seen().is_empty());
}

#[tokio::test]
async fn 相関図から_一覧とタイムラインへ行け_script_を切った_browser_にタイムラインを勧める() {
    let app = start().await;
    let body = app
        .html("/graph/network?label=tpm.key:t-23zs&from=2026-11-22T13:00")
        .await;
    assert_eq!(link_to(&body, "一覧へ"), "/actors/");
    assert_eq!(link_to(&body, "タイムラインへ"), "/graph/timeline");
    let at = body.find("<noscript>").expect("noscript");
    let noscript = &body[at..at + body[at..].find("</noscript>").unwrap()];
    assert!(
        noscript.contains("この画面は JavaScript で描きます"),
        "{noscript}"
    );
    // 同じ絞りのタイムライン（期間は、日本時間の RFC 3339 のまま）。
    assert_eq!(
        link_to(noscript, "タイムライン"),
        "/graph/timeline?label=tpm.key:t-23zs&from=2026-11-22T13:00:00%2B09:00"
    );
    // 絞り込みの form は、callboard の相関図へ送る。
    assert!(
        body.contains("<form class=\"filters\" method=\"get\" action=\"/graph/network\">"),
        "{body}"
    );
    assert_eq!(link_to(&body, "条件を消す"), "/graph/network");
}

#[tokio::test]
async fn 相関図は_絞りを中継の道へ渡し_期間は_utc_のミリ秒までの字にする() {
    let app = start().await;
    for (uri, want) in [
        (
            "/graph/network?label=tpm.key:t-23zs",
            "?label=tpm.key:t-23zs",
        ),
        // 空の値は、付けなかったのと同じ（form は空の欄も送る）。
        ("/graph/network?label=&from=&to=", ""),
        // 日時の欄の値（日本時間）も、RFC 3339 の値も、同じ時刻の UTC の字になる。
        (
            "/graph/network?from=2026-11-22T13:00&to=2026-11-22T14:00:30.5",
            "?from=2026-11-22T04:00:00.000Z&to=2026-11-22T05:00:30.500Z",
        ),
        (
            "/graph/network?to=2026-11-22T14:00:00%2B09:00&label=a:b%20c",
            "?to=2026-11-22T05:00:00.000Z&label=a:b%20c",
        ),
    ] {
        let body = app.html(uri).await;
        assert_eq!(
            data_src(&body),
            format!("/api/castlist/graph{want}"),
            "{uri}"
        );
    }
}

#[tokio::test]
async fn 期間の欄は_browser_の日時の欄で_今の期間を日本時間で出し_期間をすぐ選ぶ_link_を出す() {
    let app = start().await;
    let body = app
        .html("/graph/network?label=tpm.key:t-23zs&from=2026-11-22T04:00:00Z&to=2026-11-22T05:00:30.500Z")
        .await;
    let from = input(&body, "from");
    assert!(
        from.contains("type=\"datetime-local\"") && from.contains("value=\"2026-11-22T13:00\""),
        "{from}"
    );
    assert!(input(&body, "to").contains("value=\"2026-11-22T14:00:30.500\""));
    assert!(input(&body, "label").contains("value=\"tpm.key:t-23zs\""));
    // 期間をすぐ選ぶ link は、今の条件のまま、期間だけを日本時間の RFC 3339 で替える。
    for name in ["直近 1 時間", "今日", "昨日", "直近 7 日"] {
        let href = link_to(&body, name);
        let (path, query) = href.split_once('?').expect("引数");
        assert_eq!(path, "/graph/network", "{name}");
        let pairs: Vec<(&str, &str)> = query
            .split('&')
            .map(|p| p.split_once('=').unwrap())
            .collect();
        assert_eq!(pairs.len(), 3, "{href}");
        assert_eq!(pairs[0], ("label", "tpm.key:t-23zs"));
        assert_eq!(pairs[1].0, "from");
        assert_eq!(pairs[2].0, "to");
        for (_, value) in &pairs[1..] {
            assert!(value.ends_with("%2B09:00") && value.len() == 27, "{value}");
        }
    }
    // 今日は、日本時間の 0 時から次の 0 時まで。
    assert!(link_to(&body, "今日").contains("T00:00:00%2B09:00&to="));
}

#[tokio::test]
async fn 相関図の引数の誤りは_引数の名を書いた_400_の画面で_castlist_に聞かない() {
    let app = start().await;
    for (uri, word) in [
        (
            "/graph/network?kind=ai",
            "知らない引数: kind（使えるのは label・from・to）",
        ),
        (
            "/graph/network?label=a:b&label=c:d",
            "label は 1 つだけ付ける",
        ),
        (
            "/graph/network?label=nocolon",
            "label は「鍵:値」の形（nocolon）",
        ),
        ("/graph/network?label=UPPER:x", "label は「鍵:値」の形"),
        // 日時の欄の読めない値は、欄の見出しと直し方を、です・ます で。
        (
            "/graph/network?from=yesterday",
            "「登録した期間（から）」の日時を読めません。日時の欄で選ぶか、2026-11-22T13:00 の形で入れてください。",
        ),
        (
            "/graph/network?to=2026-13-01T00:00",
            "「登録した期間（まで）」の日時を読めません。",
        ),
    ] {
        let (status, headers, body) = app.send(get(uri)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        assert!(body.contains("<h1>400</h1>"), "{body}");
        assert!(body.contains(word), "{uri}: {word}: {body}");
    }
    assert!(app.castlist.seen().is_empty());
}

#[tokio::test]
async fn 相関図も利用者の字を_html_で逃がす() {
    let app = start().await;
    let body = app
        .html("/graph/network?label=a:%3Cscript%3Ealert(1)%3C/script%3E%22")
        .await;
    assert!(!body.contains("<script>alert"), "{body}");
    assert!(
        input(&body, "label")
            .contains("value=\"a:&#60;script&#62;alert(1)&#60;/script&#62;&#34;\""),
        "{body}"
    );
    // URL の引数としても包む。
    assert_eq!(
        data_src(&body),
        "/api/castlist/graph?label=a:%3Cscript%3Ealert%281%29%3C%2Fscript%3E%22"
    );
    // 誤りの画面でも。
    let (_, _, body) = app.send(get("/graph/network?label=%3Cb%3E")).await;
    assert!(
        !body.contains("<b>") && body.contains("&#60;b&#62;"),
        "{body}"
    );
}

#[tokio::test]
async fn 相関図は_見出しと行き来と説明と凡例を決めた字で書く() {
    let app = start().await;
    let body = app.html("/graph/network").await;
    for word in [
        "<legend data-i18n=\"filters.legend\">絞り込み</legend>",
        "<span data-i18n=\"filters.label\">ラベル</span> <input",
        "登録した期間（から）",
        "期間（今の条件のまま、期間だけを替えます）",
        "<button type=\"submit\" data-i18n=\"filters.submit\">絞り込む</button>",
        "<legend data-i18n=\"network.controls.legend\">表示する点を選ぶ</legend>",
        "薄い点は停止したアクターです",
        "角のある形: 人",
        "実線の矢印: 起動した",
        "点線の矢印: 引き継いだ",
        "時刻は日本時間（UTC+9）で、castlist が受け取った時刻です。",
    ] {
        assert!(body.contains(word), "{word}: {body}");
    }
}

#[tokio::test]
async fn 相関図の_script_と_cytoscape_は_callboard_が_application_javascript_で配る() {
    let app = start().await;
    let body = app.html("/graph/network").await;
    let at = body.find("/static/network.js?v=").unwrap();
    let href = &body[at..at + body[at..].find('"').unwrap()];
    for path in [href, "/static/vendor/cytoscape-3.35.0.min.js"] {
        let (status, headers, script) = app.send(get(path)).await;
        assert_eq!(status, StatusCode::OK, "{path}");
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
        assert!(script.len() > 1000, "{path}");
    }
    // 配る cytoscape は、repo に取り込んだ file と同じ字（npm の物と同じかは ci/check-vendor.sh が見る）。
    let (_, _, served) = app
        .send(get("/static/vendor/cytoscape-3.35.0.min.js"))
        .await;
    let vendored = include_str!("../../../static/vendor/cytoscape-3.35.0.min.js");
    assert!(served == vendored);
    // network.js は、点と線を data-src から引き、押した点のプロフィールへ行く。
    let (_, _, script) = app.send(get(href)).await;
    assert!(script.contains("container.dataset.src"), "data-src を読む");
    assert!(
        script.contains("window.location.assign(`/actors/${encodeURIComponent("),
        "プロフィールへ"
    );
    assert!(!script.contains(".innerHTML"));
}

// --- 中継の道 ---

#[tokio::test]
async fn 中継の道は_引数をそのまま_castlist_の図の_api_に渡し_答えをそのまま返す() {
    let app = start().await;
    let request =
        Request::get("/api/castlist/graph?label=tpm.key:t-23zs&from=2026-11-22T04%3A00%3A00.000Z")
            .header(header::HOST, "localhost:8803")
            .header(header::COOKIE, "a=b")
            .header(header::REFERER, "http://localhost:8803/graph/network")
            .header("x-forwarded-for", "10.0.0.1")
            .body(axum::body::Body::empty())
            .unwrap();
    let (status, headers, body) = app.send(request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    assert_eq!(body, GRAPH);
    // 画面でない答えには CSP を付けない。型の推し量りは止める。
    assert!(headers.get(header::CONTENT_SECURITY_POLICY).is_none());
    assert_eq!(
        headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
        "nosniff"
    );

    let seen = app.castlist.seen();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0].method, "GET");
    assert_eq!(
        seen[0].uri,
        "/api/graph?label=tpm.key:t-23zs&from=2026-11-22T04%3A00%3A00.000Z"
    );
    assert_eq!(seen[0].header("host"), Some("127.0.0.1"));
    for name in ["origin", "cookie", "referer", "x-forwarded-for"] {
        assert_eq!(seen[0].header(name), None, "{name}");
    }

    // 引数が無ければ、引数なしで聞く。
    app.send(get("/api/castlist/graph")).await;
    assert_eq!(app.castlist.seen()[1].uri, "/api/graph");
}

#[tokio::test]
async fn 中継の道は_castlist_の誤りの状態と型と訳をそのまま返す() {
    let app = App::start_with(
        Router::new().route(
            "/api/graph",
            route_get(|| async {
                common::castlist::error(
                    StatusCode::BAD_REQUEST,
                    "ValidationException",
                    "label は「鍵:値」の形",
                )
            }),
        ),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let (status, headers, body) = app.send(get("/api/castlist/graph?label=x")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(error_type(&headers), "ValidationException");
    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["message"], "label は「鍵:値」の形");
    // 運んだ誤りには、型の名の鍵と、castlist の訳（detail）を足す。
    assert_eq!(json["code"], "errors.kind.validation");
    assert_eq!(json["values"]["detail"], "label は「鍵:値」の形");
}

#[tokio::test]
async fn 中継の道は_castlist_に聞けなければ_unavailable_の形で返し_聞く先を出さない() {
    let down = common::nowhere().await;
    let pagetray = common::Fake::start(common::pagetray_routes()).await;
    let router = callboard::app(&common::settings(&down, &pagetray.url()));
    let (status, headers, body) = common::send(&router, get("/api/castlist/graph")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(error_type(&headers), "Unavailable");
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["message"], "castlist に聞けません（つながりません）");
    // callboard が組んだ誤りには、字の鍵と値（英語の字の値も）を付ける。
    assert_eq!(json["code"], "errors.unreachable");
    assert_eq!(
        json["values"],
        serde_json::json!({"service": "castlist", "why": {"$t": "upstream.unreachable"}})
    );
    assert_eq!(
        common::ja_text("errors.unreachable", &json["values"]),
        "castlist に聞けません（つながりません）"
    );
    assert!(!body.contains(&down), "{body}");
}

#[tokio::test]
async fn 中継の道は_答えの頭が遅ければ_callboard_の時間切れより前に_unavailable_で返す() {
    let app = App::start_with(
        Router::new().route(
            "/api/graph",
            route_get(|| async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                common::json(StatusCode::OK, GRAPH)
            }),
        ),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let started = std::time::Instant::now();
    let (status, headers, body) = app.send(get("/api/castlist/graph")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(error_type(&headers), "Unavailable");
    assert!(
        body.contains("castlist に聞けません（時間内に答えがありません）"),
        "{body}"
    );
    // 呼ぶ先を待つ 4 秒で切る（守りの層の 5 秒の「時間がかかりすぎた」より前）。
    assert!(
        started.elapsed() < callboard::REQUEST_TIMEOUT,
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn 中継の道は_本文が遅くても_読み終えるまでを区切り_unavailable_で返す() {
    // 頭はすぐ返し、本文の途中で止まる castlist。
    let app = App::start_with(
        Router::new().route(
            "/api/graph",
            route_get(|| async {
                let stream = stalled();
                axum::response::Response::builder()
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::new(stream))
                    .unwrap()
            }),
        ),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    // 直す前は、答えが終わらない。試験は 10 秒で見切る。
    let answer = tokio::time::timeout(
        Duration::from_secs(10),
        app.send(get("/api/castlist/graph")),
    )
    .await;
    let (status, headers, body) = answer.expect("答えが終わらない");
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(error_type(&headers), "Unavailable");
    assert!(
        body.contains("castlist に聞けません（時間内に答えがありません）"),
        "{body}"
    );
}

/// 初めの 1 片を出した後、終わらない本文。
fn stalled() -> Stalled {
    Stalled(false)
}

struct Stalled(bool);

impl http_body::Body for Stalled {
    type Data = axum::body::Bytes;
    type Error = std::convert::Infallible;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if self.0 {
            return std::task::Poll::Pending;
        }
        self.0 = true;
        std::task::Poll::Ready(Some(Ok(http_body::Frame::data(
            axum::body::Bytes::from_static(b"{\"asOf\":"),
        ))))
    }
}

#[tokio::test]
async fn 中継の道は読むだけで_head_は本文なし_書く_method_は_405() {
    let app = start().await;
    let (status, headers, body) = app
        .send(with_host("HEAD", "/api/castlist/graph", "127.0.0.1:8803"))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_empty());
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    // castlist には GET で聞く。
    assert_eq!(app.castlist.seen()[0].method, "GET");
    for method in ["POST", "PUT", "DELETE"] {
        let (status, _, _) = app
            .send(with_host(method, "/api/castlist/graph", "127.0.0.1:8803"))
            .await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
    }
    assert_eq!(app.castlist.seen().len(), 1);
    // castlist のほかの道は、中継しない。
    for path in [
        "/api/castlist/actors",
        "/api/castlist/graph/x",
        "/api/graph",
        "/api/castlist/backup",
    ] {
        assert_eq!(app.send(get(path)).await.0, StatusCode::NOT_FOUND, "{path}");
    }
}
