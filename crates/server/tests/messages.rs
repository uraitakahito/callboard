//! メッセージの一覧（`GET /messages`）と、新着の知らせの中継（`GET /api/pagetray/events`）。pagetray の画面の試験
//! （pagetray の tests/messages.rs の「メッセージの一覧の画面」）を、pagetray の API（ListMessages・GetMessageFacets。
//! 偽の pagetray）に聞く形に移した物。絞りの中身（どの記録が当たるか・候補の数え方）は pagetray の試験が見る。
//! ここは、引数を API にどう渡すか・答えをどう出すか・誤りの画面・知らせをそのまま流すことを見る。
mod common;

use std::convert::Infallible;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{StatusCode, header};
use axum::routing::get as route_get;
use common::{App, error_type, get};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tower::ServiceExt;

/// メッセージの記録 1 件（要る欄だけ。まだ届いていない）。
fn message(project: &str, id: &str) -> Value {
    json!({
        "project": project, "id": id, "heading": "request", "from": "t-1.swe.1", "to": "tpm",
        "text": "本文", "sentAt": "2026-11-24T00:00:00Z", "state": "waiting",
        "createdAt": "2026-11-24T00:00:01Z", "updatedAt": "2026-11-24T00:00:01Z",
    })
}

fn with(mut base: Value, change: Value) -> Value {
    for (name, value) in change.as_object().unwrap() {
        base[name] = value.clone();
    }
    base
}

fn count(value: &str, count: i64) -> Value {
    json!({ "value": value, "count": count })
}

/// 候補の答え（GetMessageFacets）。
fn facets() -> Value {
    json!({
        "states": [count("waiting", 2), count("delivered", 1), count("carried", 1)],
        "to": [count("tpm", 3), count("pm.1", 1)],
        "from": [count("pm.1", 2), count("a b?c&d", 1)],
        "keys": [count("A-1", 4)],
        "projects": [count("p", 4), count("q", 2)],
    })
}

/// 一覧の答え（ListMessages）と候補の答え（GetMessageFacets）を返す偽の pagetray。
fn pagetray(list: Value, facets: Value) -> Router {
    common::pagetray_routes()
        .route(
            "/api/messages",
            route_get(move || {
                let list = list.clone();
                async move { common::json(StatusCode::OK, &list.to_string()) }
            }),
        )
        .route(
            "/api/messages/facets",
            route_get(move || {
                let facets = facets.clone();
                async move { common::json(StatusCode::OK, &facets.to_string()) }
            }),
        )
}

async fn start(list: Value, facets: Value) -> App {
    App::start_with(
        common::castlist_routes(),
        pagetray(list, facets),
        callboard::Config::default(),
    )
    .await
}

fn unescape(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = s.get(i + 1..i + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap()
}

/// pagetray に送った問い（`path` の道の、引数の名と値の並び。名の順。% の包みは戻す）。
fn asked(app: &App, path: &str) -> Vec<(String, String)> {
    let seen: Vec<_> = app
        .pagetray
        .seen()
        .into_iter()
        .filter(|s| s.uri.split('?').next() == Some(path))
        .collect();
    assert_eq!(seen.len(), 1, "{path}: {seen:?}");
    assert_eq!(seen[0].method, "GET");
    assert_eq!(seen[0].header("host"), Some("127.0.0.1"));
    assert_eq!(seen[0].header("origin"), None);
    assert_eq!(seen[0].header("cookie"), None);
    let query = seen[0].uri.split_once('?').map_or("", |(_, q)| q);
    let mut pairs: Vec<(String, String)> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (name, value) = p.split_once('=').unwrap();
            (name.to_owned(), unescape(value))
        })
        .collect();
    pairs.sort();
    pairs
}

fn pairs<const N: usize>(pairs: [(&str, &str); N]) -> Vec<(String, String)> {
    pairs
        .into_iter()
        .map(|(n, v)| (n.to_owned(), v.to_owned()))
        .collect()
}

/// 画面に出たメッセージ（`<project>/<id>`）を、出た順に。
fn shown(page: &str) -> Vec<&str> {
    page.split("<li data-id=\"")
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap())
        .collect()
}

/// 画面の、1 件の行（`<li …>` から `</li>` まで）。
fn row<'a>(page: &'a str, id: &str) -> &'a str {
    let start = page
        .find(&format!("<li data-id=\"{id}\""))
        .unwrap_or_else(|| panic!("{id} が無い: {page}"));
    let rest = &page[start..];
    &rest[..rest.find("</li>").unwrap()]
}

// --- 行 ---

/// 行は、送った時刻（日本の時刻）・Key・見出し・「送り手 → 受け手」・状態の字・届いた時刻・渡した時・渡し直しの数・本文。
/// 並びは pagetray の答えの順。
#[tokio::test]
async fn 行は_記録を_状態の字にして出す() {
    let list = json!({ "messages": [
        with(message("p", "m-2"), json!({
            "key": "A-1", "heading": "answer", "from": "t-1.swe.1", "to": "pm.1", "text": "りんごにする",
            "sentAt": "2026-11-02T00:00:00Z", "state": "delivered", "deliveredAt": "2026-11-02T15:00:07Z",
            "via": "session-start", "deliveries": 3, "updatedAt": "2026-11-02T15:00:08.5Z",
        })),
        message("p", "m-1"),
    ]});
    let app = start(list, facets()).await;
    let page = app.html("/messages").await;
    common::check_i18n("messages.rs:163", &page);
    assert_eq!(shown(&page), ["p/m-2", "p/m-1"]);
    let line = row(&page, "p/m-2");
    assert!(
        line.starts_with(
            r#"<li data-id="p/m-2" data-state="delivered" data-at="2026-11-02T15:00:08.500Z">"#
        ),
        "{line}"
    );
    for part in [
        // UTC の 00:00 は、日本の時刻で 09:00。
        r#"<time datetime="2026-11-02T00:00:00.000Z" title="2026-11-02T00:00:00.000Z">11-02 09:00</time>"#,
        "<span>A-1</span>",
        r#"<span class="name">answer</span>"#,
        r#"<span class="state delivered" data-i18n="messages.delivered">届いた</span>"#,
        r#"<span class="route">t-1.swe.1 → pm.1</span>"#,
        // 日付をまたぐ（UTC の 15:00 は、次の日の 00:00）。
        r#"<span><span data-i18n="messages.delivered">届いた</span> <time datetime="2026-11-02T15:00:07.000Z" title="2026-11-02T15:00:07.000Z">11-03 00:00</time></span>"#,
        // 決めていない名は、書かれた字のまま。
        r#"<span data-i18n="messages.via" data-i18n-options="{&#34;via&#34;:&#34;session-start&#34;,&#34;viaEn&#34;:&#34;session-start&#34;}">渡した時: session-start</span>"#,
        r#"<span data-i18n="messages.again" data-i18n-options="{&#34;n&#34;:2}">渡し直し 2 度</span>"#,
        "<pre>りんごにする</pre>",
    ] {
        assert!(line.contains(part), "{part}: {line}");
    }
    // 要る欄だけの記録: Key・届いた時刻・渡した時・渡し直しの行は出さない。
    let line = row(&page, "p/m-1");
    assert!(
        line.contains(r#"<span class="state waiting" data-i18n="messages.state.waiting">まだ届いていない</span>"#),
        "{line}"
    );
    assert!(!line.contains(r#"class="who""#), "{line}");
    assert!(!line.contains("<span>A-1</span>"), "{line}");
    assert!(!page.contains("合うメッセージは無い"), "{page}");
    assert!(!page.contains("続きを出す"), "{page}");
}

/// 状態の字と、渡した時の字（決めてある名は画面の字に。pagetray の画面と同じ字）。
#[tokio::test]
async fn 状態と_渡した時は_画面の字で出す() {
    let states = [
        ("waiting", "まだ届いていない", "messages.state.waiting"),
        ("delivered", "届いた", "messages.delivered"),
        ("undeliverable", "渡せない", "messages.state.undeliverable"),
        ("expired", "期限切れ", "messages.state.expired"),
        ("carried", "ほかの道具で運んだ", "messages.state.carried"),
    ];
    let vias = [
        ("gate", "検査が止めた後"),
        ("wake", "起こした"),
        ("prompt", "人の入力の時"),
        ("max-deliveries", "渡し直しの上限を超えた"),
        ("ttl", "期限で消えた"),
        ("closed", "席を閉じた"),
    ];
    let mut messages = Vec::new();
    for (i, (state, _, _)) in states.iter().enumerate() {
        messages.push(with(
            message("p", &format!("s-{i}")),
            json!({ "state": state }),
        ));
    }
    for (i, (via, _)) in vias.iter().enumerate() {
        messages.push(with(
            message("p", &format!("v-{i}")),
            json!({ "via": via, "deliveries": 1 }),
        ));
    }
    let app = start(json!({ "messages": messages }), facets()).await;
    let page = app.html("/messages").await;
    common::check_i18n("messages.rs:232", &page);
    for (i, (state, label, key)) in states.iter().enumerate() {
        let line = row(&page, &format!("p/s-{i}"));
        assert!(
            line.contains(&format!(
                r#"<span class="state {state}" data-i18n="{key}">{label}</span>"#
            )),
            "{line}"
        );
    }
    for (i, (_, label)) in vias.iter().enumerate() {
        let line = row(&page, &format!("p/v-{i}"));
        // 英語の字は、値 viaEn（鍵 messages.viaName.* の英語の字）で組む。
        assert!(
            line.contains(&format!(
                r#"<span data-i18n="messages.via" data-i18n-options="{{&#34;via&#34;:&#34;{label}&#34;,&#34;viaEn&#34;:&#34;"#
            )) && line.contains(&format!(r#"&#34;}}">渡した時: {label}</span>"#)),
            "{line}"
        );
        // 渡した回数が 1 なら、渡し直しは出さない。
        assert!(!line.contains(" 度</span>"), "{line}");
    }
}

/// 本文も名も、HTML として出さない（字として逃がす）。長い本文（200 字か 5 行を超える）は、頭だけ見せて、開くと全部。
#[tokio::test]
async fn 本文は逃がして出し_長ければ頭だけを見せる() {
    let evil = r#"<img src=x onerror="alert(1)"><b>太字</b>"#;
    let long = format!("頭の行\n{}終わり", "あ".repeat(400));
    let list = json!({ "messages": [
        with(message("p", "m-1"), json!({ "text": evil, "from": "<i>from</i>" })),
        with(message("p", "m-2"), json!({ "text": long })),
        with(message("p", "m-3"), json!({ "text": "1\n2\n3\n4\n5\n6" })),
        with(message("p", "m-4"), json!({ "text": "あ".repeat(200) })),
    ]});
    let app = start(list, facets()).await;
    let page = app.html("/messages").await;
    assert!(!page.contains("<img"), "{page}");
    assert!(!page.contains("<b>"), "{page}");
    assert!(!page.contains("<i>"), "{page}");
    let line = row(&page, "p/m-1");
    assert!(
        line.contains(
            "<pre>&#60;img src=x onerror=&#34;alert(1)&#34;&#62;&#60;b&#62;太字&#60;/b&#62;</pre>"
        ),
        "{line}"
    );
    assert!(
        line.contains(r#"<span class="route">&#60;i&#62;from&#60;/i&#62; → tpm</span>"#),
        "{line}"
    );
    // 長い本文: 頭は、改行を空白にした初めの 120 字と「…」。
    let line = row(&page, "p/m-2");
    let head = format!("頭の行 {}…", "あ".repeat(116));
    assert!(
        line.contains(&format!(
            "<details><summary>{head}</summary><pre>{long}</pre></details>"
        )),
        "{line}"
    );
    // 行が多い本文も。切らなければ「…」は付けない。
    let line = row(&page, "p/m-3");
    assert!(
        line.contains(
            "<details><summary>1 2 3 4 5 6</summary><pre>1\n2\n3\n4\n5\n6</pre></details>"
        ),
        "{line}"
    );
    // 200 字ちょうどは、そのまま。
    assert!(!row(&page, "p/m-4").contains("<details>"));
}

// --- 引数と絞り込み ---

/// 画面の引数（project・key・to・from・state・at）を、ListMessages と GetMessageFacets に渡す。1 頁は 200 件。
/// 空の値と知らない状態は、条件が無いのと同じ。
#[tokio::test]
async fn 引数を_一覧と候補の問いに渡す() {
    let app = start(json!({ "messages": [] }), facets()).await;
    let page = app.html("/messages").await;
    assert_eq!(asked(&app, "/api/messages"), pairs([("maxResults", "200")]));
    assert_eq!(asked(&app, "/api/messages/facets"), pairs([]));
    assert!(
        page.contains(
            r#"<span class="empty" data-i18n="messages.empty">合うメッセージは無い</span>"#
        ),
        "{page}"
    );

    let app = start(json!({ "messages": [] }), facets()).await;
    app.html("/messages?project=p&key=A-1&to=a%20b&from=pm.1&state=waiting&at=6d65")
        .await;
    assert_eq!(
        asked(&app, "/api/messages"),
        pairs([
            ("from", "pm.1"),
            ("key", "A-1"),
            ("maxResults", "200"),
            ("nextToken", "6d65"),
            ("project", "p"),
            ("state", "waiting"),
            ("to", "a b"),
        ])
    );
    // 候補の問いには、今絞っている受け手・送り手・Key だけを渡す。
    assert_eq!(
        asked(&app, "/api/messages/facets"),
        pairs([("from", "pm.1"), ("key", "A-1"), ("to", "a b")])
    );

    let app = start(json!({ "messages": [] }), facets()).await;
    app.html("/messages?state=done&to=&key=%20&project=").await;
    assert_eq!(asked(&app, "/api/messages"), pairs([("maxResults", "200")]));
    assert_eq!(asked(&app, "/api/messages/facets"), pairs([]));
}

/// 絞り込みの列: 候補と件数は pagetray の答えのまま（置き場の全部）。押すと今の条件に重ね、選んである候補を押すと外す。
/// 「まだ届いていない n 件」は、操作の帯と tab の題に出す。
#[tokio::test]
async fn 絞り込みの列と_まだ届いていない数を出す() {
    let app = start(json!({ "messages": [message("p", "m-1")] }), facets()).await;
    let page = app.html("/messages").await;
    common::check_i18n("messages.rs:353", &page);
    assert!(
        page.contains("<title data-i18n=\"messages.titleWaiting\" data-i18n-options=\"{&#34;n&#34;:2}\">まだ届いていない 2 件 ・ メッセージの記録 ・ callboard</title>"),
        "{page}"
    );
    assert!(
        page.contains(r#"<a class="btn" id="undelivered" href="/messages?state=waiting" title="まだ届いていないメッセージだけに絞る" data-i18n="messages.undelivered" data-i18n-options="{&#34;n&#34;:2}" data-i18n-attr="title:messages.undeliveredTitle">まだ届いていない 2 件</a>"#),
        "{page}"
    );
    for choice in [
        r#"<h2 data-i18n="filters.state">状態</h2>"#,
        r#"<a href="/messages?state=waiting"><span data-i18n="messages.state.waiting">まだ届いていない</span><span class="n">2</span></a>"#,
        r#"<a href="/messages?state=carried"><span data-i18n="messages.state.carried">ほかの道具で運んだ</span><span class="n">1</span></a>"#,
        r#"<h2 data-i18n="filters.recipient">受け手</h2>"#,
        r#"<a href="/messages?to=tpm"><span>tpm</span><span class="n">3</span></a>"#,
        r#"<h2 data-i18n="filters.sender">送り手</h2>"#,
        r#"<a href="/messages?from=pm.1"><span>pm.1</span><span class="n">2</span></a>"#,
        // 値は % で包む。
        r#"<a href="/messages?from=a%20b%3Fc%26d"><span>a b?c&#38;d</span><span class="n">1</span></a>"#,
        r#"<h2 data-i18n="filters.key">Key</h2>"#,
        r#"<a href="/messages?key=A-1"><span>A-1</span><span class="n">4</span></a>"#,
        r#"<h2 data-i18n="filters.project">project</h2>"#,
        r#"<a href="/messages?project=q"><span>q</span><span class="n">2</span></a>"#,
    ] {
        assert!(page.contains(choice), "{choice}: {page}");
    }
    assert!(!page.contains("絞り込みを外す"), "{page}");
    // 件数が 0 の状態（答えに無い状態）の候補は出さない。
    assert!(!page.contains("期限切れ"), "{page}");

    let app = start(json!({ "messages": [message("p", "m-1")] }), facets()).await;
    let page = app.html("/messages?state=waiting&at=6d65").await;
    // 選んだ候補は印が付き、もう 1 度押すと外れる。ほかの候補は、今の条件に重ねる。続きの印（at）は、条件を替えると外す。
    assert!(
        page.contains(r#"<a href="/messages" class="on" aria-current="true"><span data-i18n="messages.state.waiting">まだ届いていない</span><span class="n">2</span></a>"#),
        "{page}"
    );
    assert!(
        page.contains(r#"<a href="/messages?key=A-1&#38;state=waiting"><span>A-1</span>"#),
        "{page}"
    );
    assert!(
        page.contains(r#"<a class="btn on" id="undelivered" href="/messages?state=waiting" title="まだ届いていないメッセージだけに絞る" aria-current="true" data-i18n="messages.undelivered" data-i18n-options="{&#34;n&#34;:2}" data-i18n-attr="title:messages.undeliveredTitle">まだ届いていない 2 件</a>"#),
        "{page}"
    );
    assert!(
        page.contains(
            r#"<a class="btn" href="/messages" data-i18n="common.tray.reset">絞り込みを外す</a>"#
        ),
        "{page}"
    );
    // 新着の印の先は、今の条件の頭（続きの印は付けない）。
    assert!(
        page.contains(
            r#"<a class="btn" id="fresh" href="/messages?state=waiting" hidden data-i18n="common.tray.fresh">● 新着</a>"#
        ),
        "{page}"
    );

    // まだ届いていない物が無ければ、数の釦は隠し、題にも出さない。空の並びの見出しは出さない。
    let quiet = json!({
        "states": [count("delivered", 1)], "to": [count("tpm", 1)], "from": [count("pm.1", 1)],
        "keys": [], "projects": [count("p", 1)],
    });
    let app = start(json!({ "messages": [] }), quiet).await;
    let page = app.html("/messages").await;
    assert!(
        page.contains("<title data-i18n=\"messages.title\">メッセージの記録 ・ callboard</title>"),
        "{page}"
    );
    assert!(
        page.contains(
            r#"title="まだ届いていないメッセージだけに絞る" hidden data-i18n="messages.undelivered" data-i18n-options="{&#34;n&#34;:0}" data-i18n-attr="title:messages.undeliveredTitle">まだ届いていない 0 件</a>"#
        ),
        "{page}"
    );
    assert!(!page.contains("<h2>Key</h2>"), "{page}");
}

/// 続きがあれば（答えに nextToken）、同じ条件に続きの印を付けた link を出す。
#[tokio::test]
async fn 続きは_link_で出す() {
    let list = json!({ "messages": [message("p", "m-1")], "nextToken": "6d6573" });
    let app = start(list, facets()).await;
    let page = app.html("/messages?to=tpm&at=00").await;
    assert!(
        page.contains(r#"<a class="more" href="/messages?to=tpm&#38;at=6d6573" data-i18n="common.tray.more">続きを出す</a>"#),
        "{page}"
    );
}

/// 頭の帯の「メッセージ」と入口の link は、callboard のメッセージの一覧を指す。操作の帯の「ページ」は、callboard の
/// ページの一覧を指す。画面は script の file を 1 つ読み、inline の script と style は無い。
#[tokio::test]
async fn 画面の間の_link_と_script() {
    let app = start(json!({ "messages": [] }), facets()).await;
    let page = app.html("/messages").await;
    assert!(
        page.contains(r#"<a href="/messages" data-i18n="nav.messages">メッセージ</a>"#),
        "{page}"
    );
    assert!(
        page.contains(r#"<a class="btn" href="/pages/" data-i18n="nav.pages">ページ</a>"#),
        "{page}"
    );
    assert!(
        page.contains(
            r#"<h1><a href="/messages" data-i18n="messages.heading">メッセージの記録</a></h1>"#
        ),
        "{page}"
    );
    let scripts = common::without_i18n_scripts(&page);
    let src = scripts
        .split(r#"<script src=""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_else(|| panic!("script が無い: {page}"));
    assert!(src.starts_with("/static/messages.js?v="), "{src}");
    assert_eq!(
        common::without_i18n_scripts(&page)
            .matches("<script")
            .count(),
        1,
        "{page}"
    );
    assert!(!page.contains("<style"), "{page}");
    assert!(
        page.contains(r#"data-events="/api/pagetray/events""#),
        "{page}"
    );
    let (status, headers, body) = app.send(get(src)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_TYPE],
        "application/javascript; charset=utf-8"
    );
    assert!(body.contains("EventSource"), "{body}");
    let home = app.html("/").await;
    assert!(
        home.contains(r#"<a href="/messages" data-i18n="nav.messages">メッセージ</a>"#),
        "{home}"
    );
}

// --- 誤り ---

/// 同じ名の引数を 2 度書いた物は 400 の画面。pagetray が引数を断れば（形の違う値）、その訳を書いた 400 の画面。
#[tokio::test]
async fn 読めない引数と_pagetray_が断った引数は_400_の画面() {
    let app = start(json!({ "messages": [] }), facets()).await;
    let (status, _, body) = app.send(get("/messages?to=a&to=b")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("引数を読めません"), "{body}");
    assert!(app.pagetray.seen().is_empty(), "pagetray に聞いた");

    let refuse = || async {
        (
            StatusCode::BAD_REQUEST,
            [
                (header::CONTENT_TYPE, "application/json"),
                (
                    header::HeaderName::from_static("x-amzn-errortype"),
                    "ValidationException",
                ),
            ],
            json!({ "message": "nextToken が読めない" }).to_string(),
        )
    };
    let app = App::start_with(
        common::castlist_routes(),
        common::pagetray_routes()
            .route("/api/messages", route_get(refuse))
            .route(
                "/api/messages/facets",
                route_get(|| async { common::json(StatusCode::OK, &facets().to_string()) }),
            ),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get("/messages?at=zz")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.contains("pagetray が引数を断りました: nextToken が読めない"),
        "{body}"
    );
}

/// pagetray に聞けなければ 503 の画面で、訳を書く。候補の操作の無い古い pagetray（1.2.0 より前）は、404 が返ったと書く。
#[tokio::test]
async fn pagetray_に聞けなければ_503_の画面() {
    let app = App::start_with(
        common::castlist_routes(),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get("/messages")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body.contains("pagetray に聞けません（404 が返りました）"),
        "{body}"
    );

    let nowhere = common::nowhere().await;
    let castlist = common::Fake::start(common::castlist_routes()).await;
    let router = callboard::app(&common::settings(&castlist.url(), &nowhere));
    let (status, _, body) = common::send(&router, get("/messages")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body.contains("pagetray に聞けません（つながりません）"),
        "{body}"
    );
    assert!(!body.contains(&nowhere), "聞く先を画面に出した: {body}");
}

// --- 新着の知らせの中継 ---

/// 知らせを流す偽の pagetray。`tx` に送った字が、そのまま流れる。`tx` を落とすと、流れが終わる。つなげるのは 1 度だけ。
fn events_pagetray() -> (Router, mpsc::UnboundedSender<&'static str>) {
    let (tx, rx) = mpsc::unbounded_channel::<&'static str>();
    let rx = std::sync::Arc::new(std::sync::Mutex::new(Some(rx)));
    let routes = common::pagetray_routes().route(
        "/api/events",
        route_get(move || {
            let rx = rx.lock().unwrap().take().expect("1 度だけつなぐ");
            async move {
                let stream = futures_util::stream::unfold(rx, |mut rx| async move {
                    rx.recv()
                        .await
                        .map(|chunk| (Ok::<_, Infallible>(Bytes::from(chunk)), rx))
                });
                (
                    [
                        (header::CONTENT_TYPE, "text/event-stream"),
                        (header::CACHE_CONTROL, "no-cache"),
                    ],
                    Body::from_stream(stream),
                )
            }
        }),
    );
    (routes, tx)
}

/// 流れの次の塊（1 秒まで待つ）。流れが終われば None。
async fn chunk(body: &mut Body) -> Option<String> {
    let frame = tokio::time::timeout(Duration::from_secs(1), body.frame())
        .await
        .expect("1 秒の内に、次の塊か終わりが来る")?;
    let data = frame.expect("流れの誤り").into_data().expect("data の塊");
    Some(String::from_utf8(data.to_vec()).unwrap())
}

/// 中継の道を開く（答えの頭と、流れ）。
async fn listen(app: &App) -> (StatusCode, axum::http::HeaderMap, Body) {
    let mut request = get("/api/pagetray/events");
    // browser が付ける header は、pagetray に渡さない。
    request
        .headers_mut()
        .insert(header::COOKIE, "a=b".parse().unwrap());
    request
        .headers_mut()
        .insert("last-event-id", "7".parse().unwrap());
    let response = app.router.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    (parts.status, parts.headers, body)
}

/// pagetray の知らせ（`GET /api/events`）を、名も中身もそのまま流す。callboard の時間切れ（答えの頭まで）は、流れを切らない。
#[tokio::test]
async fn 知らせを_そのまま流す() {
    let (routes, tx) = events_pagetray();
    let app = App::start_with(
        common::castlist_routes(),
        routes,
        callboard::Config {
            timeout: Duration::from_millis(100),
            ..callboard::Config::default()
        },
    )
    .await;
    let (status, headers, mut body) = listen(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "text/event-stream");
    assert_eq!(headers[header::CACHE_CONTROL], "no-cache");
    for sent in [
        "event: message-record\ndata: {\"project\":\"p\",\"id\":\"m-1\"}\n\n",
        // 名の無い event（ページを置いた知らせ）と、注釈の行（keep-alive）も。
        "data: {\"path\":\"/p/A-1/plan-1.html\"}\n\n",
        ": keep-alive\n\n",
    ] {
        tx.send(sent).unwrap();
        assert_eq!(chunk(&mut body).await.as_deref(), Some(sent));
    }
    // 時間切れの長さを過ぎても、流れは続く。
    tokio::time::sleep(Duration::from_millis(300)).await;
    tx.send("event: reply\ndata: {}\n\n").unwrap();
    assert_eq!(
        chunk(&mut body).await.as_deref(),
        Some("event: reply\ndata: {}\n\n")
    );

    let seen: Vec<_> = app
        .pagetray
        .seen()
        .into_iter()
        .filter(|s| s.uri == "/api/events")
        .collect();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0].method, "GET");
    assert_eq!(seen[0].header("host"), Some("127.0.0.1"));
    assert_eq!(seen[0].header("accept"), Some("text/event-stream"));
    assert_eq!(seen[0].header("cookie"), None);
    assert_eq!(seen[0].header("last-event-id"), None);
    assert_eq!(seen[0].header("origin"), None);

    // pagetray が流れを閉じれば（起こし直しなど）、callboard の流れも終わる（browser がつなぎ直す）。
    drop(tx);
    assert_eq!(chunk(&mut body).await, None);
}

/// 止める合図を受けたら、pagetray が流し続けていても、流れを閉じる（閉じないと、止める時に走っている物を終えるのを待つ所が、
/// browser が切るまで待ち続ける）。
#[tokio::test]
async fn 止める合図で_流れを閉じる() {
    let (routes, tx) = events_pagetray();
    let closing = callboard::Closing::new();
    let app = App::start_with(
        common::castlist_routes(),
        routes,
        callboard::Config {
            closing: closing.clone(),
            ..callboard::Config::default()
        },
    )
    .await;
    let (status, _, mut body) = listen(&app).await;
    assert_eq!(status, StatusCode::OK);
    tx.send(": hello\n\n").unwrap();
    assert_eq!(chunk(&mut body).await.as_deref(), Some(": hello\n\n"));
    closing.cancel();
    assert_eq!(chunk(&mut body).await, None);
    // 合図の後につないだ物は、すぐ終わる。
    drop(tx);
}

/// pagetray に届かない・知らせの流れでない答え（404・500・型の違う 200）は、503 `Unavailable`（聞く先は書かない）。
#[tokio::test]
async fn 知らせにつなげなければ_503() {
    let nowhere = common::nowhere().await;
    let castlist = common::Fake::start(common::castlist_routes()).await;
    let router = callboard::app(&common::settings(&castlist.url(), &nowhere));
    let (status, headers, body) = common::send(&router, get("/api/pagetray/events")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error_type(&headers), "Unavailable");
    assert!(
        body.contains("pagetray に聞けません（つながりません）"),
        "{body}"
    );
    assert!(!body.contains(&nowhere), "{body}");

    for routes in [
        // 道が無い（404）。
        common::pagetray_routes(),
        common::pagetray_routes().route(
            "/api/events",
            route_get(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
        ),
        // 200 でも、知らせの流れの型でない。
        common::pagetray_routes().route(
            "/api/events",
            route_get(|| async { common::json(StatusCode::OK, "{}") }),
        ),
    ] {
        let app = App::start_with(
            common::castlist_routes(),
            routes,
            callboard::Config::default(),
        )
        .await;
        let (status, headers, body) = app.send(get("/api/pagetray/events")).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        assert_eq!(error_type(&headers), "Unavailable");
        assert!(body.contains("pagetray に聞けません"), "{body}");
    }
}

/// pagetray が答えの頭を返さなければ、時間内（呼ぶ先を待つ長さ）に 503 を返す。
#[tokio::test]
async fn 知らせの頭が来なければ_503() {
    let routes = common::pagetray_routes().route(
        "/api/events",
        route_get(|| async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            StatusCode::OK
        }),
    );
    let app = App::start_with(
        common::castlist_routes(),
        routes,
        callboard::Config {
            events_wait: Duration::from_millis(100),
            ..callboard::Config::default()
        },
    )
    .await;
    let started = std::time::Instant::now();
    let (status, headers, body) = app.send(get("/api/pagetray/events")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(error_type(&headers), "Unavailable");
    assert!(body.contains("時間内に答えがありません"), "{body}");
    assert!(started.elapsed() < Duration::from_secs(2));
}
