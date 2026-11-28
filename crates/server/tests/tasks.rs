//! 課題の一覧（`GET /messages/tasks`）。偽の pagetray（ListMessages）に聞く。課題ごとのまとめ方は `src/ui/tasks.rs` の単体の試験が見る。
//! ここは、引数を API にどう渡すか・行の出し方・返事待ちの絞り・続き・ほかの画面からの link を見る。
mod common;

use axum::Router;
use axum::http::{StatusCode, header};
use axum::routing::get as route_get;
use common::{App, get};
use serde_json::{Value, json};

/// メッセージの記録 1 件（届いた物）。`at` は 2026-11-28 の UTC の時刻。
fn message(
    id: &str,
    key: &str,
    heading: &str,
    from: &str,
    to: &str,
    at: &str,
    text: &str,
) -> Value {
    json!({
        "project": "p", "id": id, "key": key, "heading": heading, "from": from, "to": to,
        "text": format!("【{heading}】{text}"), "sentAt": format!("2026-11-28T{at}Z"), "state": "delivered",
        "createdAt": "2026-11-28T00:00:00Z", "updatedAt": "2026-11-28T00:00:00Z",
    })
}

fn reply(mut m: Value, to: &str) -> Value {
    m["replyTo"] = json!(to);
    m
}

/// 新しい順（pagetray の答えの順）。K-2 は PM の返事待ち、K-1 は済み、K-3 は次の依頼待ち。
fn records() -> Value {
    json!([
        message(
            "q2",
            "K-2",
            "ask-pm",
            "K-2",
            "tpm",
            "05:00:00",
            "http://127.0.0.1:8801/p/K-2/confirm-1.html 項目: (1) 案"
        ),
        reply(
            message("e1", "K-1", "end", "K-1", "tpm", "04:00:00", "成果: なし"),
            "x1"
        ),
        message(
            "x1",
            "K-1",
            "dismiss",
            "tpm",
            "K-1",
            "03:40:00",
            "この課題に続きは無い。"
        ),
        reply(
            message(
                "d1",
                "K-1",
                "done",
                "K-1",
                "tpm",
                "03:30:00",
                "http://127.0.0.1:8801/p/K-1/result-1.html 要点: 入れた"
            ),
            "b1"
        ),
        reply(
            message(
                "v1",
                "K-1.staff",
                "review",
                "K-1.staff",
                "K-1",
                "03:15:00",
                "判定: よい"
            ),
            "r1"
        ),
        message(
            "r1",
            "K-1",
            "ask-review",
            "K-1",
            "K-1.staff",
            "03:10:00",
            "頼み: #1"
        ),
        message(
            "n3",
            "K-3",
            "notify",
            "tpm",
            "K-3",
            "03:05:00",
            "知らせ <b>x</b>"
        ),
        message(
            "b2",
            "K-2",
            "brief",
            "tpm",
            "K-2",
            "03:02:00",
            "http://127.0.0.1:8801/p/K-2/brief-1.html"
        ),
        message(
            "b1",
            "K-1",
            "brief",
            "tpm",
            "K-1",
            "03:00:00",
            "http://127.0.0.1:8801/p/K-1/brief-1.html"
        ),
    ])
}

/// 一覧の答え（ListMessages）と、空の候補（GetMessageFacets）を返す偽の pagetray。
fn pagetray(list: Value) -> Router {
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
            route_get(|| async {
                let facets =
                    json!({ "states": [], "to": [], "from": [], "keys": [], "projects": [] });
                common::json(StatusCode::OK, &facets.to_string())
            }),
        )
}

async fn start(list: Value) -> App {
    App::start_with(
        common::castlist_routes(),
        pagetray(list),
        callboard::Config::default(),
    )
    .await
}

/// 画面の、課題 1 つの行（`<li data-key="<Key>">` から `</li>` まで）。
fn row<'a>(page: &'a str, key: &str) -> &'a str {
    let start = page
        .find(&format!(r#"<li data-key="{key}""#))
        .unwrap_or_else(|| panic!("{key} の行が無い: {page}"));
    let rest = &page[start..];
    &rest[..rest.find("</li>").unwrap()]
}

fn shown(page: &str) -> Vec<&str> {
    page.split("<li data-key=\"")
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap())
        .collect()
}

/// pagetray に聞いた、ListMessages の引数（名の順）。
fn asked(app: &App) -> Vec<String> {
    let seen: Vec<_> = app
        .pagetray
        .seen()
        .into_iter()
        .filter(|s| s.uri.starts_with("/api/messages"))
        .collect();
    assert_eq!(seen.len(), 1, "{seen:?}");
    let mut pairs: Vec<String> = seen[0]
        .uri
        .split_once('?')
        .map_or("", |(_, q)| q)
        .split('&')
        .map(str::to_owned)
        .collect();
    pairs.sort();
    pairs
}

#[tokio::test]
async fn 新しい_1000_通を_1_回読み_課題ごとに_最新の順で出す_行は_今の様子_通数_期間_最新の_1_通() {
    let app = start(json!({ "messages": records() })).await;
    let page = app.html("/messages/tasks").await;
    common::check_i18n("tasks", &page);
    assert_eq!(asked(&app), ["maxResults=1000"]);
    assert_eq!(shown(&page), ["K-2", "K-1", "K-3"]);
    assert!(
        page.contains(
            r#"<title data-i18n="tasks.title">課題ごと ・ メッセージの記録 ・ callboard</title>"#
        ),
        "{page}"
    );

    // K-2: 依頼書（担当）と確認（PM）が返事待ち。前の者は PM。いつから（確認を送った時刻）と、ほかの返事待ちの数。
    let waiting = row(&page, "K-2");
    for part in [
        r#"<a class="key" href="/messages/tasks/K-2">K-2</a>"#,
        r#"<span class="status h-pm" data-i18n="thread.status.waiting" data-i18n-options="{&#34;who&#34;:{&#34;$t&#34;:&#34;thread.holder.pm&#34;}}">PM の返事待ち</span>"#,
        r#"<span class="since"><time datetime="2026-11-28T05:00:00.000Z" title="2026-11-28T05:00:00.000Z">11-28 14:00</time> <span data-i18n="tasks.since">から</span></span>"#,
        r#"<span data-i18n="tasks.others" data-i18n-options="{&#34;n&#34;:1}">ほかに返事待ち 1</span>"#,
        r#"<span data-i18n="tasks.count" data-i18n-options="{&#34;n&#34;:2}">2 通</span>"#,
        r#"<time datetime="2026-11-28T03:02:00.000Z" title="2026-11-28T03:02:00.000Z">11-28 12:02</time> 〜 <time datetime="2026-11-28T05:00:00.000Z" title="2026-11-28T05:00:00.000Z">11-28 14:00</time>"#,
        // 最新の 1 通: 見出しと、本文の頭（URL は外す）。
        r#"<span class="name">ask-pm</span> <span class="head">項目: (1) 案</span>"#,
    ] {
        assert!(waiting.contains(part), "{part}: {waiting}");
    }
    // K-1: 済み。Staff の席の記録も、同じ課題に数える（4 + 1 + 1 = 6 通）。返事待ちの行は出さない。
    let done = row(&page, "K-1");
    assert!(
        done.contains(r#"<span class="status done" data-i18n="thread.status.done">済み</span>"#),
        "{done}"
    );
    assert!(
        done.contains(r#"data-i18n-options="{&#34;n&#34;:6}">6 通</span>"#),
        "{done}"
    );
    assert!(
        !done.contains("tasks.since") && !done.contains("tasks.others"),
        "{done}"
    );
    // K-3: 返事待ちも end も無い。本文は逃がして出す。
    let idle = row(&page, "K-3");
    assert!(
        idle.contains(
            r#"<span class="status idle" data-i18n="thread.status.idle">次の依頼待ち</span>"#
        ),
        "{idle}"
    );
    assert!(
        idle.contains("知らせ &#60;b&#62;x&#60;/b&#62;") && !idle.contains("<b>"),
        "{idle}"
    );

    // 操作の帯: 返事待ちの課題の数の釦（押すと、返事待ちだけ）と、1 通ずつの一覧へ。読んだ範囲の注。続きは無い。
    assert!(page.contains(r#"<a class="btn" id="waiting" href="/messages/tasks?waiting=1" data-i18n="tasks.waiting" data-i18n-options="{&#34;n&#34;:1}">返事待ちだけ 1 件</a>"#), "{page}");
    assert!(
        page.contains(
            r#"<a class="btn" href="/messages" data-i18n="thread.toMessages">1 通ずつ</a>"#
        ),
        "{page}"
    );
    assert!(page.contains(r#"<p class="thread-note" data-i18n="tasks.window" data-i18n-options="{&#34;n&#34;:9}">新しい 9 通から作っています</p>"#), "{page}");
    assert!(!page.contains("common.tray.more"), "{page}");
    assert!(
        page.contains(r#"<div class="tray tasks" data-events="/api/pagetray/events">"#)
            && page.contains(r#"<script src="/static/tasks.js?v="#),
        "{page}"
    );
}

#[tokio::test]
async fn waiting_で_返事待ちの課題だけに絞る_もう_1_度押すと外す() {
    let app = start(json!({ "messages": records() })).await;
    let page = app.html("/messages/tasks?waiting=1").await;
    common::check_i18n("tasks waiting", &page);
    assert_eq!(shown(&page), ["K-2"]);
    assert!(page.contains(r#"<a class="btn on" id="waiting" href="/messages/tasks" aria-current="true" data-i18n="tasks.waiting" data-i18n-options="{&#34;n&#34;:1}">返事待ちだけ 1 件</a>"#), "{page}");
    // 返事待ちの課題が無ければ、釦は出さない。絞って 0 件なら、そう書く。
    let app = start(
        json!({ "messages": [message("n", "K-3", "notify", "tpm", "K-3", "03:05:00", "知らせ")] }),
    )
    .await;
    let page = app.html("/messages/tasks?waiting=1").await;
    assert_eq!(shown(&page), Vec::<&str>::new());
    assert!(!page.contains(r#"id="waiting""#), "{page}");
    assert!(
        page.contains(r#"<span class="empty" data-i18n="tasks.empty">合う課題は無い</span>"#),
        "{page}"
    );
}

#[tokio::test]
async fn project_と続きの印は_pagetray_に渡し_link_にも付ける_続きがあれば_link_を出す() {
    let app = start(json!({ "messages": records(), "nextToken": "abc" })).await;
    let page = app
        .html("/messages/tasks?project=p&at=zz&waiting=1&x=1")
        .await;
    assert_eq!(
        asked(&app),
        ["maxResults=1000", "nextToken=zz", "project=p"]
    );
    assert!(
        page.contains(r#"<a class="key" href="/messages/tasks/K-2?project=p">K-2</a>"#),
        "{page}"
    );
    // 続きは、条件を保って次の印へ。釦は、続きの印を外す。
    assert!(page.contains(r#"<a class="more" href="/messages/tasks?project=p&#38;waiting=1&#38;at=abc" data-i18n="common.tray.more">続きを出す</a>"#), "{page}");
    assert!(
        page.contains(r#"id="waiting" href="/messages/tasks?project=p""#),
        "{page}"
    );
}

#[tokio::test]
async fn メッセージの一覧と課題の筋の操作の帯から_課題の一覧へ行ける() {
    let app = start(json!({ "messages": records() })).await;
    let page = app.html("/messages").await;
    assert!(
        page.contains(
            r#"<a class="btn" href="/messages/tasks" data-i18n="messages.toTasks">課題ごと</a>"#
        ),
        "{page}"
    );
    let page = app.html("/messages/tasks/K-1").await;
    assert!(
        page.contains(
            r#"<a class="btn" href="/messages/tasks" data-i18n="messages.toTasks">課題ごと</a>"#
        ),
        "{page}"
    );
}

#[tokio::test]
async fn pagetray_が引数を断れば_400_の画面_聞けなければ_503_の画面() {
    let refused = common::pagetray_routes().route(
        "/api/messages",
        route_get(|| async {
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
        }),
    );
    let app = App::start_with(
        common::castlist_routes(),
        refused,
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get("/messages/tasks?at=zz")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("pagetray が引数を断りました"), "{body}");
    let down = App::start().await;
    let (status, _, body) = down.send(get("/messages/tasks")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
}
