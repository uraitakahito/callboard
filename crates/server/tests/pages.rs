//! ページの一覧（`GET /pages/`）と、file を上げる中継（`POST /api/pagetray/projects/{project}/keys/{key}/files`）。
//! pagetray の画面の試験（pagetray の tests/ui.rs）を、pagetray の API（ListPages・GetPageFacets・ListReviews・ListFiles・
//! GetFileRules。偽の pagetray）に聞く形に移した物。絞りの中身と数え方は pagetray の試験が見る。
//! ここは、引数を API にどう渡すか・答えをどう出すか・誤りの画面・上げる道の守りを見る。
mod common;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{get as route_get, post as route_post};
use common::{App, PAGETRAY_LINK, error_type, get};
use serde_json::{Value, json};

const ACTOR: &str = "019a0000-0000-7000-8000-00000000000a";

/// ページの記録 1 件（要る欄だけ）。
fn page(project: &str, key: &str, kind: &str, n: u32) -> Value {
    json!({
        "project": project, "key": key, "kind": kind, "n": n.to_string(), "title": format!("{kind} の題"),
        "role": "software-engineer", "workspace": "workspace:1", "createdAt": "2026-11-24T00:00:00Z",
        "size": 10, "path": format!("/{project}/{key}/{kind}-{n}.html"),
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

/// 候補の答え（GetPageFacets）。
fn facets() -> Value {
    json!({
        "waiting": 2, "answered": 1, "reviewWaiting": 3, "escalated": 0,
        "senders": [{ "value": ACTOR, "count": 2, "note": "t-1.swe.1" }],
        "roles": [count("software-engineer", 3), count("tpm", 1)],
        "workspaces": [{ "value": "workspace:1", "count": 3, "note": "A-1, B-2" }, count("", 1)],
        "kinds": [count("plan", 2)],
        "keys": [count("A-1", 4)],
        "projects": [count("p", 4), count("q", 2)],
        "verdicts": [count("concerns", 1), count("approve", 2)],
    })
}

fn rules() -> Value {
    json!({
        "types": [{ "ext": "png", "type": "image/png" }, { "ext": "txt", "type": "text/plain; charset=utf-8" }],
        "maxBytes": 16_777_216,
    })
}

/// 決めた答えを返す偽の pagetray（ListPages・GetPageFacets・ListReviews・ListFiles・GetFileRules）。
fn pagetray(pages: Value, reviews: Value, files: Value) -> Router {
    let answer = |value: Value| {
        route_get(move || {
            let value = value.clone();
            async move { common::json(StatusCode::OK, &value.to_string()) }
        })
    };
    common::pagetray_routes()
        .route("/api/pages", answer(pages))
        .route("/api/pages/facets", answer(facets()))
        .route("/api/reviews", answer(reviews))
        .route("/api/files", answer(files))
        .route("/api/files/rules", answer(rules()))
}

async fn start(pages: Value) -> App {
    start_with(pages, json!({ "reviews": [] }), json!({ "files": [] })).await
}

async fn start_with(pages: Value, reviews: Value, files: Value) -> App {
    App::start_with(
        common::castlist_routes(),
        pagetray(pages, reviews, files),
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

/// pagetray に送った問い（`path` の道の、引数の名と値の並び。名の順。% の包みは戻す）。聞いていなければ None。
fn asked(app: &App, path: &str) -> Option<Vec<(String, String)>> {
    let seen: Vec<_> = app
        .pagetray
        .seen()
        .into_iter()
        .filter(|s| s.uri.split('?').next() == Some(path))
        .collect();
    assert!(seen.len() <= 1, "{path}: {seen:?}");
    let seen = seen.first()?;
    assert_eq!(seen.method, "GET");
    assert_eq!(seen.header("host"), Some("127.0.0.1"));
    assert_eq!(seen.header("origin"), None);
    let query = seen.uri.split_once('?').map_or("", |(_, q)| q);
    let mut pairs: Vec<(String, String)> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (name, value) = p.split_once('=').unwrap_or((p, ""));
            (name.to_owned(), unescape(value))
        })
        .collect();
    pairs.sort();
    Some(pairs)
}

fn pairs<const N: usize>(pairs: [(&str, &str); N]) -> Option<Vec<(String, String)>> {
    Some(
        pairs
            .into_iter()
            .map(|(n, v)| (n.to_owned(), v.to_owned()))
            .collect(),
    )
}

/// 画面に出たページ（path）を、出た順に。
fn shown(page: &str) -> Vec<&str> {
    page.split("<li data-path=\"")
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap())
        .collect()
}

/// 画面の、1 件の行（`<li …>` から `</li>` まで）。
fn row<'a>(page: &'a str, path: &str) -> &'a str {
    let start = page
        .find(&format!("<li data-path=\"{path}\""))
        .unwrap_or_else(|| panic!("{path} が無い: {page}"));
    let rest = &page[start..];
    &rest[..rest.find("</li>").unwrap()]
}

// --- 行 ---

/// 行は、置いた時刻（日本の時刻）・Key・種類と番号・判定・返事の印・題・役割と workspace・見た物とレビューの link・送り手。
#[tokio::test]
async fn 行は_記録と印を出す() {
    let pages = json!({ "pages": [
        with(page("p", "A-1", "review", 1), json!({
            "title": "", "role": "staff-engineer", "workspace": "",
            "createdAt": "2026-11-24T15:30:00.5Z",
            "askedAt": "2026-11-24T16:00:00Z", "waitsOn": "tpm", "verdict": "concerns",
            "targets": ["/p/A-1/plan-1.html", "/p/B-2/plan-3.html", "/q/Z-9/brief-2.html", "https://github.com/o/r/pull/191", "https://example.com/x"],
            "sender": { "issuer": "tpm", "ref": "claude.session:a", "actor": { "id": ACTOR, "name": "t-1.swe.1", "kind": "ai" }, "resolvedAt": "2026-11-24T15:30:01Z" },
        })),
        with(page("p", "A-1", "plan", 1), json!({ "sender": { "issuer": "tpm", "ref": "claude.session:b" } })),
        with(page("p", "A-1", "confirm", 2), json!({ "askedAt": "2026-11-24T01:00:00Z", "answeredAt": "2026-11-24T02:00:00Z" })),
        with(page("p", "A-1", "brief", 1), json!({ "askedAt": "2026-11-24T01:00:00Z", "waitsOn": "review", "targets": ["/p/A-1/plan-1.html"] })),
    ]});
    let reviews = json!({ "reviews": [
        { "target": "/p/A-1/plan-1.html", "path": "/p/A-1/review-1.html", "verdict": "concerns" },
        { "target": "/p/A-1/plan-1.html", "path": "/q/Z-9/review-4.html" },
    ]});
    let app = start_with(pages, reviews, json!({ "files": [] })).await;
    let html = app.html("/pages/?kind=x").await;
    assert_eq!(
        shown(&html),
        [
            "/p/A-1/review-1.html",
            "/p/A-1/plan-1.html",
            "/p/A-1/confirm-2.html",
            "/p/A-1/brief-1.html"
        ]
    );
    // レビューのページ: TPM 担当に上げた（待ちの相手の印）・判定・見た物（同じ Key・同じ project・ほかの project・PR・ほかの URL）。
    let line = row(&html, "/p/A-1/review-1.html");
    assert!(
        line.starts_with(r#"<li data-path="/p/A-1/review-1.html" data-at="2026-11-24T15:30:00.500Z" data-reply="waiting" data-on="tpm" data-verdict="concerns">"#),
        "{line}"
    );
    for part in [
        r#"<a href="/pages/?kind=x&#38;open=%2Fp%2FA-1%2Freview-1.html">"#,
        // UTC の 15:30 は、次の日の 00:30。
        r#"<time datetime="2026-11-24T15:30:00.500Z" title="2026-11-24T15:30:00.500Z">11-25 00:30</time><span>A-1</span><span class="name">review-1</span>"#,
        r#"<span class="verdict concerns">気になる所あり</span><span class="reply waiting tpm">上げた</span>"#,
        r#"<span class="title">（題なし）</span><span class="who">staff-engineer</span></a>"#,
        r#"<span class="links targets">見た物: <a href="/pages/?kind=x&#38;open=%2Fp%2FA-1%2Fplan-1.html">plan-1</a> <a href="/pages/?kind=x&#38;open=%2Fp%2FB-2%2Fplan-3.html">B-2/plan-3</a> <a href="/pages/?kind=x&#38;open=%2Fq%2FZ-9%2Fbrief-2.html">q/Z-9/brief-2</a> <a href="https://github.com/o/r/pull/191" target="_blank" rel="noopener">r#191</a> <a href="https://example.com/x" target="_blank" rel="noopener">https://example.com/x</a></span>"#,
        // 送り手: castlist の名（callboard のプロフィールへ）と、付随する id。
        r#"<span class="sender"><a class="actor" href="/actors/019a0000-0000-7000-8000-00000000000a" target="_blank" rel="noopener">t-1.swe.1</a><small>claude.session:a</small></span>"#,
    ] {
        assert!(line.contains(part), "{part}: {line}");
    }
    // 見られたページ: このページを見たレビュー（判定は、ある時だけ）。castlist の答えの無い送り手は「未確認」で、link は無い。
    let line = row(&html, "/p/A-1/plan-1.html");
    for part in [
        r#"<li data-path="/p/A-1/plan-1.html" data-at="2026-11-24T00:00:00.000Z">"#,
        r#"<time datetime="2026-11-24T00:00:00.000Z" title="2026-11-24T00:00:00.000Z">11-24 09:00</time>"#,
        r#"<span class="title">plan の題</span><span class="who">software-engineer ・ workspace:1</span></a>"#,
        r#"<span class="links reviews">レビュー: <a href="/pages/?kind=x&#38;open=%2Fp%2FA-1%2Freview-1.html">review-1</a><span class="verdict concerns">気になる所あり</span> <a href="/pages/?kind=x&#38;open=%2Fq%2FZ-9%2Freview-4.html">q/Z-9/review-4</a></span>"#,
        r#"<span class="sender">未確認<small>claude.session:b</small></span>"#,
    ] {
        assert!(line.contains(part), "{part}: {line}");
    }
    assert!(!line.contains("links targets"), "{line}");
    // 返事の印: 答えた（Product Manager）・レビュー待ち。
    let line = row(&html, "/p/A-1/confirm-2.html");
    assert!(line.contains(r#"data-reply="answered">"#), "{line}");
    assert!(
        line.contains(r#"<span class="reply answered">答えた</span>"#),
        "{line}"
    );
    let line = row(&html, "/p/A-1/brief-1.html");
    assert!(
        line.contains(r#"data-reply="waiting" data-on="review">"#),
        "{line}"
    );
    assert!(
        line.contains(r#"<span class="reply waiting review">レビュー待ち</span>"#),
        "{line}"
    );
    assert!(
        line.contains(r#"<span class="links targets">見る物: <a "#),
        "{line}"
    );
    // 見たレビューを、出た行の path で聞く。
    assert_eq!(
        asked(&app, "/api/reviews"),
        pairs([
            ("target", "/p/A-1/brief-1.html"),
            ("target", "/p/A-1/confirm-2.html"),
            ("target", "/p/A-1/plan-1.html"),
            ("target", "/p/A-1/review-1.html"),
        ])
    );
}

// --- 引数 ---

/// 画面の引数を、ListPages に渡す（`sort=created_at` は `createdAt`、`at` は `nextToken`。1 頁は 200 件）。候補の問いに引数は無い。
/// 空の値・知らない値は、条件が無いのと同じ（workspace だけは、空も条件）。行が無ければ、見たレビューは聞かない。
#[tokio::test]
async fn 引数を_一覧の問いに渡す() {
    let app = start(json!({ "pages": [] })).await;
    let html = app.html("/pages/").await;
    assert_eq!(asked(&app, "/api/pages"), pairs([("maxResults", "200")]));
    assert_eq!(asked(&app, "/api/pages/facets"), pairs([]));
    assert_eq!(asked(&app, "/api/reviews"), None);
    assert_eq!(asked(&app, "/api/files"), None);
    assert!(
        html.contains(r#"<span class="empty">合うページは無い</span>"#),
        "{html}"
    );

    let app = start(json!({ "pages": [] })).await;
    app.html("/pages/?project=p&kind=plan&role=tpm&workspace=&q=%E3%81%BF%E3%81%8B%E3%82%93&reply=waiting&on=review&verdict=approve&actor=019a0000-0000-7000-8000-00000000000a&sort=created_at&at=6d65")
        .await;
    assert_eq!(
        asked(&app, "/api/pages"),
        pairs([
            ("actor", ACTOR),
            ("kind", "plan"),
            ("maxResults", "200"),
            ("nextToken", "6d65"),
            ("on", "review"),
            ("project", "p"),
            ("q", "みかん"),
            ("reply", "waiting"),
            ("role", "tpm"),
            ("sort", "createdAt"),
            ("verdict", "approve"),
            ("workspace", ""),
        ])
    );

    // 空の値・知らない返事と判定と並び・返事の無い待ちの相手・Product Manager の待ちの相手は、渡さない。
    let app = start(json!({ "pages": [] })).await;
    app.html("/pages/?project=&q=%20&reply=maybe&on=tpm&verdict=great&sort=title&nope=1")
        .await;
    assert_eq!(asked(&app, "/api/pages"), pairs([("maxResults", "200")]));
    let app = start(json!({ "pages": [] })).await;
    app.html("/pages/?reply=answered&on=pm").await;
    assert_eq!(
        asked(&app, "/api/pages"),
        pairs([("maxResults", "200"), ("reply", "answered")])
    );
}

// --- 操作の帯と絞り込みの列 ---

/// 操作の帯: 探す form（今の条件を隠した欄で運ぶ）・並びの釦・待ちの数の釦（0 件は隠す）・新着の印。tab の題に、返事待ちの数。
/// 絞り込みの列: 返事・判定・送り手・役割・workspace・種類・Key・project。数は pagetray の答えのまま。
#[tokio::test]
async fn 操作の帯と_絞り込みの列を出す() {
    let app = start(json!({ "pages": [page("p", "A-1", "plan", 1)] })).await;
    let html = app.html("/pages/").await;
    for part in [
        "<title>返事待ち 2 件 ・ ページの置き場 ・ callboard</title>",
        r#"<h1><a href="/pages/">ページの置き場</a></h1>"#,
        r#"<a class="btn" href="/messages">メッセージ</a>"#,
        r#"<form action="/pages/" method="get">"#,
        r#"<input type="search" name="q" value="" placeholder="題と本文を探す" aria-label="題と本文を探す">"#,
        r#"<a class="btn" href="/pages/?sort=created_at" title="並びを替える">並び: 新しい順</a>"#,
        r#"<a class="btn" id="waiting" href="/pages/?reply=waiting" title="Product Manager の返事を待っているページだけに絞る">返事待ち 2 件</a>"#,
        r#"<a class="btn" id="review-waiting" href="/pages/?reply=waiting&#38;on=review" title="レビューの判定を待っている依頼書だけに絞る">レビュー待ち 3 件</a>"#,
        // 0 件の釦は隠す。
        r#"id="escalated" href="/pages/?reply=waiting&#38;on=tpm" title="判定を受けずに TPM 担当へ上げ、まだ決まっていないレビューだけに絞る" hidden>上げた 0 件</a>"#,
        r#"<a class="btn" id="fresh" href="/pages/" hidden>● 新着</a>"#,
        "<h2>返事</h2>",
        r#"<a href="/pages/?reply=waiting"><span>返事待ち</span><span class="n">2</span></a>"#,
        r#"<a href="/pages/?reply=answered"><span>答えた</span><span class="n">1</span></a>"#,
        r#"<a href="/pages/?reply=waiting&#38;on=review"><span>レビュー待ち</span><span class="n">3</span></a>"#,
        "<h2>判定</h2>",
        r#"<a href="/pages/?verdict=concerns"><span>気になる所あり</span><span class="n">1</span></a>"#,
        "<h2>送り手</h2>",
        r#"<a href="/pages/?actor=019a0000-0000-7000-8000-00000000000a"><span>t-1.swe.1</span><span class="n">2</span></a>"#,
        "<h2>担当者（役割）</h2>",
        r#"<a href="/pages/?role=tpm"><span>tpm</span><span class="n">1</span></a>"#,
        "<h2>workspace</h2>",
        // workspace には、その ID で置かれた Key を添える。空の値は「（不明）」。
        r#"<a href="/pages/?workspace=workspace%3A1"><span>workspace:1<small>A-1, B-2</small></span><span class="n">3</span></a>"#,
        r#"<a href="/pages/?workspace="><span>（不明）</span><span class="n">1</span></a>"#,
        "<h2>種類</h2>",
        "<h2>Key</h2>",
        r#"<a href="/pages/?key=A-1"><span>A-1</span><span class="n">4</span></a>"#,
        "<h2>project</h2>",
        r#"<a href="/pages/?project=q"><span>q</span><span class="n">2</span></a>"#,
    ] {
        assert!(html.contains(part), "{part}: {html}");
    }
    // 0 件の「上げた」は、絞り込みの列にも出さない。
    assert!(!html.contains("<span>上げた</span>"), "{html}");
    assert!(!html.contains("絞り込みを外す"), "{html}");

    // 絞っている時: 選んだ候補は印が付き、押すと外れる。ほかの候補は重ねる。続きの印（at）は、条件を替えると外す。
    let app = start(json!({ "pages": [page("p", "A-1", "plan", 1)] })).await;
    let html = app
        .html("/pages/?project=p&q=%E3%81%BF&reply=waiting&on=review&sort=created_at&at=6d65")
        .await;
    for part in [
        // 探す form は、今の条件（検索の字と続きの印のほか）を、隠した欄で運ぶ。
        r#"<input type="hidden" name="project" value="p">"#,
        r#"<input type="hidden" name="reply" value="waiting">"#,
        r#"<input type="hidden" name="on" value="review">"#,
        r#"<input type="hidden" name="sort" value="created_at">"#,
        r#"<input type="search" name="q" value="み" "#,
        r#"<a class="btn" href="/pages/?project=p&#38;q=%E3%81%BF&#38;reply=waiting&#38;on=review" title="並びを替える">並び: 古い順</a>"#,
        r#"<a class="btn on" id="review-waiting" href="/pages/?reply=waiting&#38;on=review&#38;sort=created_at" title="レビューの判定を待っている依頼書だけに絞る" aria-current="true">レビュー待ち 3 件</a>"#,
        // 絞り込みを外しても、並びは保つ。
        r#"<a class="btn" href="/pages/?sort=created_at">絞り込みを外す</a>"#,
        r#"<a class="btn" id="fresh" href="/pages/?project=p&#38;q=%E3%81%BF&#38;reply=waiting&#38;on=review&#38;sort=created_at" hidden>● 新着</a>"#,
        r#"<a href="/pages/?project=p&#38;q=%E3%81%BF&#38;sort=created_at" class="on" aria-current="true"><span>レビュー待ち</span>"#,
        r#"<a href="/pages/?q=%E3%81%BF&#38;reply=waiting&#38;on=review&#38;sort=created_at" class="on" aria-current="true"><span>p</span>"#,
        r#"<a href="/pages/?project=p&#38;key=A-1&#38;q=%E3%81%BF&#38;reply=waiting&#38;on=review&#38;sort=created_at"><span>A-1</span>"#,
    ] {
        assert!(html.contains(part), "{part}: {html}");
    }
    assert!(!html.contains(r#"name="at""#), "{html}");
    assert!(
        !html.contains(r#"name="q" value="み" type="hidden""#),
        "{html}"
    );
}

/// 続きがあれば（答えに nextToken）、同じ条件に続きの印を付けた link を出す。
#[tokio::test]
async fn 続きは_link_で出す() {
    let app = start(json!({ "pages": [page("p", "A-1", "plan", 1)], "nextToken": "6d6573" })).await;
    let html = app.html("/pages/?role=tpm&at=00").await;
    assert!(
        html.contains(r#"<a class="more" href="/pages/?role=tpm&#38;at=6d6573">続きを出す</a>"#),
        "{html}"
    );
}

// --- 本文の枠 ---

/// 選んだページ（`open`）は、本文の枠（iframe）に、pagetray の URL（link の頭）で出す。行に印が付く。
/// ページの path の形でない `open` は、書かないのと同じ。画面の CSP は、枠と図に pagetray の link の頭だけを足す。
#[tokio::test]
async fn 選んだページを_本文の枠に出す() {
    let app = start(json!({ "pages": [page("p", "A-1", "plan", 1)] })).await;
    let (status, headers, html) = app.send(get("/pages/?open=%2Fp%2FA-1%2Fplan-1.html")).await;
    assert_eq!(status, StatusCode::OK, "{html}");
    for part in [
        r#" data-viewer="/p/A-1/plan-1.html""#.to_owned(),
        format!(
            r#"<p><span>/p/A-1/plan-1.html</span><a href="{PAGETRAY_LINK}/p/A-1/plan-1.html" target="_blank" rel="noopener">別の tab で開く</a></p>"#
        ),
        format!(
            r#"<iframe src="{PAGETRAY_LINK}/p/A-1/plan-1.html" sandbox="allow-scripts" title="選んだページ"></iframe>"#
        ),
        r#"<a href="/pages/?open=%2Fp%2FA-1%2Fplan-1.html" class="on" aria-current="true">"#
            .to_owned(),
    ] {
        assert!(html.contains(&part), "{part}: {html}");
    }
    assert_eq!(
        headers[header::CONTENT_SECURITY_POLICY],
        format!(
            "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob: {PAGETRAY_LINK}; connect-src 'self'; frame-src {PAGETRAY_LINK}; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
        )
    );
    // script は callboard が配る 1 つ。inline の script と style は無い。
    assert_eq!(
        common::without_i18n_scripts(&html)
            .matches("<script")
            .count(),
        1,
        "{html}"
    );
    assert!(
        html.contains(r#"<script src="/static/pages.js?v="#),
        "{html}"
    );
    assert!(!html.contains("<style"), "{html}");
    assert!(
        html.contains(r#"data-events="/api/pagetray/events""#),
        "{html}"
    );

    for open in [
        "https%3A%2F%2Fevil.example%2Fx.html",
        "%2Fp%2FA-1%2Fplan-01.html",
        "%2Fp%2F..%2Fplan-1.html",
        "%2Fp%2FA-1%2Ffiles%2Fa-1.png",
        "javascript%3Aalert(1)",
    ] {
        let app = start(json!({ "pages": [page("p", "A-1", "plan", 1)] })).await;
        let html = app.html(&format!("/pages/?open={open}")).await;
        assert!(!html.contains("<iframe"), "{open}: {html}");
        assert!(!html.contains("data-viewer"), "{open}: {html}");
        assert!(
            html.contains("<p>ページを選ぶと、ここに 1 枚だけ出る</p>"),
            "{open}: {html}"
        );
    }
    // ほかの画面の CSP は、前のまま（枠も、pagetray の図も許さない）。
    let (_, headers, _) = app.send(get("/")).await;
    assert_eq!(
        headers[header::CONTENT_SECURITY_POLICY],
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
    );
}

// --- Key の画面 ---

/// project と Key を 1 つずつ選んだ時（Key の画面）だけ、file の枠と上げる枠を出す。file は ListFiles、受ける種類は GetFileRules。
#[tokio::test]
async fn key_の画面は_file_の枠と上げる枠を出す() {
    let files = json!({ "files": [
        { "project": "p", "key": "A-1", "name": "shot", "ext": "png", "n": "2", "type": "image/png", "size": 48230, "sha256": "ab",
          "role": "product-manager", "workspace": "", "createdAt": "2026-11-24T00:00:00Z", "path": "/p/A-1/files/shot-2.png" },
        { "project": "p", "key": "A-1", "name": "notes", "ext": "txt", "n": "1", "type": "text/plain; charset=utf-8", "size": 512, "sha256": "cd",
          "role": "tpm", "workspace": "workspace:1", "createdAt": "2026-11-23T15:00:00Z", "path": "/p/A-1/files/notes-1.txt",
          "sender": { "issuer": "tpm", "ref": "claude.session:a", "actor": { "id": ACTOR, "name": "t-1.swe.1", "kind": "ai" } } },
    ]});
    let app = start_with(json!({ "pages": [] }), json!({ "reviews": [] }), files).await;
    let html = app.html("/pages/?project=p&key=A-1").await;
    assert_eq!(
        asked(&app, "/api/files"),
        pairs([("key", "A-1"), ("maxResults", "200"), ("project", "p")])
    );
    assert_eq!(asked(&app, "/api/files/rules"), pairs([]));
    for part in [
        r#"<div id="files" class="box" role="region" aria-label="file の枠">"#.to_owned(),
        // 図は、pagetray の URL で小さく出す。ほかは拡張子の字。
        format!(
            r#"<li data-file="/p/A-1/files/shot-2.png"><a class="thumb" href="{PAGETRAY_LINK}/p/A-1/files/shot-2.png" target="_blank" rel="noopener" tabindex="-1" aria-hidden="true"><img src="{PAGETRAY_LINK}/p/A-1/files/shot-2.png" loading="lazy" alt=""></a>"#
        ),
        format!(r#"<a class="name" href="{PAGETRAY_LINK}/p/A-1/files/shot-2.png" target="_blank" rel="noopener">shot-2.png</a>"#),
        r#"<span class="who">47.1 KiB ・ product-manager ・ <time datetime="2026-11-24T00:00:00.000Z" title="2026-11-24T00:00:00.000Z">11-24 09:00</time></span><code class="ref">files/shot-2.png</code>"#.to_owned(),
        format!(r#"aria-hidden="true">txt</a><span class="meta"><a class="name" href="{PAGETRAY_LINK}/p/A-1/files/notes-1.txt""#),
        r#"<span class="who">512 B ・ tpm ・ <time datetime="2026-11-23T15:00:00.000Z" title="2026-11-23T15:00:00.000Z">11-24 00:00</time></span>"#.to_owned(),
        r#"<span class="sender"><a class="actor" href="/actors/019a0000-0000-7000-8000-00000000000a" target="_blank" rel="noopener">t-1.swe.1</a><small>claude.session:a</small></span>"#.to_owned(),
        // 上げる枠: 送る先は callboard の中継の道。受ける種類と上限は、pagetray の答え。
        r#"<div id="upload" class="box" role="region" aria-label="上げる枠" data-project="p" data-key="A-1" data-types="[[&#34;png&#34;,&#34;image/png&#34;],[&#34;txt&#34;,&#34;text/plain; charset=utf-8&#34;]]" data-max="16777216" data-upload="/api/pagetray/projects/p/keys/A-1/files">"#.to_owned(),
        r#"<input type="file" accept=".png,.txt">"#.to_owned(),
    ] {
        assert!(html.contains(&part), "{part}: {html}");
    }

    // file が無ければ、そう書く。
    let app = start(json!({ "pages": [] })).await;
    let html = app.html("/pages/?project=p&key=A-1").await;
    assert!(
        html.contains(r#"<span class="empty">この Key の file は無い</span>"#),
        "{html}"
    );
    // project か Key が 1 つだけ・project が api・形の違う名では、枠を出さず、file も聞かない。
    for query in [
        "project=p",
        "key=A-1",
        "project=api&key=A-1",
        "project=p&key=a%2Fb",
        "project=..&key=A-1",
    ] {
        let app = start(json!({ "pages": [] })).await;
        let (_, _, html) = app.send(get(&format!("/pages/?{query}"))).await;
        assert!(!html.contains(r#"id="upload""#), "{query}: {html}");
        assert_eq!(asked(&app, "/api/files"), None, "{query}");
    }
}

// --- 画面の間の link と誤り ---

/// 頭の帯と入口の「ページ」は callboard の一覧を、メッセージの一覧の操作の帯の「ページ」も callboard の一覧を指す。
#[tokio::test]
async fn 画面の間の_link() {
    let app = start(json!({ "pages": [] })).await;
    let html = app.html("/pages/").await;
    assert!(
        html.contains(r#"<a href="/pages/" data-i18n="nav.pages">ページ</a>"#),
        "{html}"
    );
    let home = app.html("/").await;
    assert!(
        home.contains(r#"<a href="/pages/">ページの一覧</a>"#),
        "{home}"
    );
    let (status, headers, body) = app.send(get("/static/pages.js")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_TYPE],
        "application/javascript; charset=utf-8"
    );
    assert!(body.contains("EventSource"), "{body}");
    // 後ろに / の無い道は、一覧へ送る。
    let (status, headers, _) = app.send(get("/pages")).await;
    assert_eq!(status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(headers[header::LOCATION], "/pages/");
}

/// 同じ名の引数を 2 度書いた物は 400 の画面。pagetray が引数を断れば、その訳を書いた 400 の画面。聞けなければ 503 の画面。
#[tokio::test]
async fn 誤りの画面() {
    let app = start(json!({ "pages": [] })).await;
    let (status, _, body) = app.send(get("/pages/?kind=a&kind=b")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.contains("引数を読めません（kind を 2 度書いています）"),
        "{body}"
    );
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
            .route("/api/pages", route_get(refuse))
            .route(
                "/api/pages/facets",
                route_get(|| async { common::json(StatusCode::OK, &facets().to_string()) }),
            ),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get("/pages/?at=zz")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.contains("pagetray が引数を断りました: nextToken が読めない"),
        "{body}"
    );

    // 候補の操作の無い古い pagetray（1.3.0 より前）。
    let app = App::start_with(
        common::castlist_routes(),
        common::pagetray_routes().route(
            "/api/pages",
            route_get(|| async { common::json(StatusCode::OK, r#"{"pages":[]}"#) }),
        ),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get("/pages/")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body.contains("pagetray に聞けません（404 が返りました）"),
        "{body}"
    );
}

// --- file を上げる中継 ---

/// 偽の pagetray の、file を置く道。受けた要求（引数・header・本文）を残し、決めた答えを返す。
#[derive(Clone, Default)]
struct Uploads(Arc<Mutex<Vec<(String, HeaderMap, Bytes)>>>);

fn upload_pagetray(
    uploads: Uploads,
    status: StatusCode,
    kind: Option<&'static str>,
    body: Value,
) -> Router {
    common::pagetray_routes().route(
        "/api/projects/{project}/keys/{key}/files",
        route_post(move |request: Request| {
            let uploads = uploads.clone();
            let body = body.clone();
            async move {
                let (parts, bytes) = request.into_parts();
                let bytes = axum::body::to_bytes(bytes, usize::MAX).await.unwrap();
                uploads
                    .0
                    .lock()
                    .unwrap()
                    .push((parts.uri.to_string(), parts.headers, bytes));
                let mut response = common::json(status, &body.to_string());
                if let Some(kind) = kind {
                    response
                        .headers_mut()
                        .insert("x-amzn-errortype", kind.parse().unwrap());
                }
                response.headers_mut().insert(
                    header::LOCATION,
                    "http://127.0.0.1/p/A-1/files/a-1.png".parse().unwrap(),
                );
                response
            }
        }),
    )
}

/// 一覧の画面の form からの上げ（`Origin` が、この callboard 自身）。
fn upload(path: &str, content_type: &str, body: impl Into<Body>) -> Request {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(header::HOST, "127.0.0.1:8803")
        .header(header::ORIGIN, "http://127.0.0.1:8803")
        .header("sec-fetch-site", "same-origin")
        .header(header::CONTENT_TYPE, content_type)
        .header(header::COOKIE, "a=b")
        .body(body.into())
        .unwrap()
}

const UPLOAD: &str = "/api/pagetray/projects/p/keys/A-1/files";

/// 上げた file は、pagetray の CreateFile に、役割 product-manager で渡す。browser からは名だけを受け、役割・workspace・番号・
/// 時刻・付随する id は渡さない。pagetray の答え（状態・誤りの型の名・本文）を、そのまま返す。
#[tokio::test]
async fn 上げた_file_を_役割_product_manager_で渡す() {
    let uploads = Uploads::default();
    let created = json!({ "project": "p", "key": "A-1", "name": "shot", "ext": "png", "n": "3", "path": "/p/A-1/files/shot-3.png" });
    let app = App::start_with(
        common::castlist_routes(),
        upload_pagetray(uploads.clone(), StatusCode::CREATED, None, created.clone()),
        callboard::Config::default(),
    )
    .await;
    let request = upload(
        &format!(
            "{UPLOAD}?name=shot.png&role=tpm&workspace=workspace%3A9&n=7&createdAt=2020-01-01T00%3A00%3A00Z&actorIssuer=tpm&actorRef=claude.session%3Aa"
        ),
        "image/png",
        vec![0x89, b'P', b'N', b'G'],
    );
    let (status, headers, body) = app.send(request).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(headers[header::CONTENT_TYPE], "application/json");
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), created);
    // pagetray が組んだ URL（要求の Host から組むので、browser からは開けない）は返さない。
    assert_eq!(headers.get(header::LOCATION), None);
    let seen = uploads.0.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    let (uri, sent, bytes) = &seen[0];
    assert_eq!(
        uri,
        "/api/projects/p/keys/A-1/files?name=shot.png&role=product-manager"
    );
    assert_eq!(sent[header::HOST], "127.0.0.1");
    assert_eq!(sent[header::CONTENT_TYPE], "image/png");
    assert_eq!(sent.get(header::ORIGIN), None);
    assert_eq!(sent.get(header::COOKIE), None);
    assert_eq!(bytes.as_ref(), [0x89, b'P', b'N', b'G']);
}

/// pagetray が断った訳（種類を受けない 415 など）は、状態・型の名・本文のまま返る（画面の script が message を出す）。
#[tokio::test]
async fn pagetray_が断った上げは_そのまま返す() {
    let uploads = Uploads::default();
    let app = App::start_with(
        common::castlist_routes(),
        upload_pagetray(
            uploads,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Some("UnsupportedMediaType"),
            json!({ "message": "中身が png でない" }),
        ),
        callboard::Config::default(),
    )
    .await;
    let (status, headers, body) = app
        .send(upload(&format!("{UPLOAD}?name=a.png"), "image/png", "x"))
        .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(error_type(&headers), "UnsupportedMediaType");
    assert!(body.contains("中身が png でない"), "{body}");

    // pagetray に届かなければ 503（聞く先は書かない）。
    let nowhere = common::nowhere().await;
    let castlist = common::Fake::start(common::castlist_routes()).await;
    let router = callboard::app(&common::settings(&castlist.url(), &nowhere));
    let (status, headers, body) = common::send(
        &router,
        upload(&format!("{UPLOAD}?name=a.png"), "image/png", "x"),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error_type(&headers), "Unavailable");
    assert!(!body.contains(&nowhere), "{body}");
}

/// pagetray の答えが時間内（4 秒）に来なければ 503 で、置かれたかもしれないと書く（本文は渡し終えているかもしれない）。
#[tokio::test]
async fn 上げの答えが来なければ_置かれたかもしれないと書く() {
    let routes = common::pagetray_routes().route(
        "/api/projects/{project}/keys/{key}/files",
        route_post(|| async {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            StatusCode::CREATED
        }),
    );
    let app = App::start_with(
        common::castlist_routes(),
        routes,
        callboard::Config {
            // 守りの層の時間切れより先に、上げの道が自分の訳で答えることを見る。
            timeout: std::time::Duration::from_secs(60),
            ..callboard::Config::default()
        },
    )
    .await;
    let (status, headers, body) = app
        .send(upload(&format!("{UPLOAD}?name=a.png"), "image/png", "x"))
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(error_type(&headers), "Unavailable");
    assert!(
        body.contains("時間内に答えがありません。file は置かれたかもしれません"),
        "{body}"
    );
}

/// 上げられるのは、この callboard の画面の form からだけ（`Origin` が `http://<Host>` と同じで、`Sec-Fetch-Site` が無いか
/// `same-origin`）。ほかの site・本文の枠の中のページ（`Origin: null`）・`Origin` の無い要求は 403 で、pagetray に渡さない。
/// 上げる道のほかは、`Origin` の付いた書く要求を、前のとおり断る。
#[tokio::test]
async fn 上げられるのは_この画面の_form_からだけ() {
    let uploads = Uploads::default();
    let app = App::start_with(
        common::castlist_routes(),
        upload_pagetray(uploads.clone(), StatusCode::CREATED, None, json!({})),
        callboard::Config::default(),
    )
    .await;
    let path = format!("{UPLOAD}?name=a.png");
    let request = |origin: Option<&str>, site: Option<&str>, host: &str, path: &str| {
        let mut builder = Request::builder()
            .method("POST")
            .uri(path)
            .header(header::HOST, host)
            .header(header::CONTENT_TYPE, "image/png");
        if let Some(origin) = origin {
            builder = builder.header(header::ORIGIN, origin);
        }
        if let Some(site) = site {
            builder = builder.header("sec-fetch-site", site);
        }
        builder.body(Body::from("x")).unwrap()
    };
    for (origin, site, host) in [
        (
            Some("https://evil.example"),
            Some("cross-site"),
            "127.0.0.1:8803",
        ),
        (Some("https://evil.example"), None, "127.0.0.1:8803"),
        // 置き場のページ（本文の枠の中。pagetray の origin か、sandbox の null）。
        (
            Some("http://127.0.0.1:8801"),
            Some("same-site"),
            "127.0.0.1:8803",
        ),
        (Some("null"), None, "127.0.0.1:8803"),
        // 同じ origin の字でも、browser が「ほかの site から」と言う要求。
        (
            Some("http://127.0.0.1:8803"),
            Some("cross-site"),
            "127.0.0.1:8803",
        ),
        // port の違う callboard の字。
        (
            Some("http://127.0.0.1:8803"),
            Some("same-origin"),
            "127.0.0.1:9999",
        ),
        // browser でない要求（Origin が無い）。
        (None, None, "127.0.0.1:8803"),
    ] {
        let (status, headers, body) = app.send(request(origin, site, host, &path)).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{origin:?} {site:?} {host}: {body}"
        );
        assert_eq!(error_type(&headers), "Forbidden");
    }
    assert!(uploads.0.lock().unwrap().is_empty(), "pagetray に渡した");
    // Sec-Fetch-Site の無い browser（古い物）と、localhost で開いた画面からは通る。
    for (origin, site, host) in [
        ("http://127.0.0.1:8803", None, "127.0.0.1:8803"),
        (
            "http://localhost:8803",
            Some("same-origin"),
            "localhost:8803",
        ),
    ] {
        let (status, _, body) = app.send(request(Some(origin), site, host, &path)).await;
        assert_eq!(status, StatusCode::CREATED, "{origin}: {body}");
    }
    // 上げる道でない道は、同じ origin からでも書かせない。
    for other in [
        "/api/pagetray/projects/p/keys/A-1/files/x",
        "/api/pagetray/events",
        "/pages/",
        "/api/pagetray/projects/p/keys/A-1/pages",
    ] {
        let (status, _, _) = app
            .send(request(
                Some("http://127.0.0.1:8803"),
                Some("same-origin"),
                "127.0.0.1:8803",
                other,
            ))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{other}");
    }
    // 名の無い要求と、形の違う project・Key は 400 で、pagetray に渡さない。
    let before = uploads.0.lock().unwrap().len();
    for bad in [
        UPLOAD.to_owned(),
        format!("{UPLOAD}?name="),
        "/api/pagetray/projects/api/keys/A-1/files?name=a.png".to_owned(),
        "/api/pagetray/projects/p/keys/a%20b/files?name=a.png".to_owned(),
        "/api/pagetray/projects/../keys/A-1/files?name=a.png".to_owned(),
    ] {
        let (status, headers, body) = app.send(upload(&bad, "image/png", "x")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}: {body}");
        assert_eq!(error_type(&headers), "ValidationException", "{bad}");
    }
    assert_eq!(uploads.0.lock().unwrap().len(), before);
}

/// 上げる道の本文は 16 MiB まで（pagetray と同じ。ほかの道の 2 MiB より大きい）。超えれば 413 で、pagetray に渡さない。
#[tokio::test]
async fn 上げる_file_は_16_mib_まで() {
    let uploads = Uploads::default();
    let app = App::start_with(
        common::castlist_routes(),
        upload_pagetray(uploads.clone(), StatusCode::CREATED, None, json!({})),
        callboard::Config::default(),
    )
    .await;
    let path = format!("{UPLOAD}?name=a.txt");
    let (status, _, body) = app
        .send(upload(&path, "text/plain", vec![b'a'; 16 * 1024 * 1024]))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(uploads.0.lock().unwrap()[0].2.len(), 16 * 1024 * 1024);
    let (status, headers, _) = app
        .send(upload(
            &path,
            "text/plain",
            vec![b'a'; 16 * 1024 * 1024 + 1],
        ))
        .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(error_type(&headers), "PayloadTooLarge");
    assert_eq!(uploads.0.lock().unwrap().len(), 1);
}
