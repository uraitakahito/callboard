//! 課題の筋（`GET /messages/tasks/{key}`）。偽の pagetray（ListMessages・GetMessageFacets）に聞く。
//! 往復の作り方（どの記録がどの頼みを閉じるか）は `src/ui/exchanges.rs` の単体の試験が見る。ここは、記録の読み方（どの Key を
//! 聞くか）・画面への出し方（棒・印・字・link）・絞り・誤りの画面を見る。
mod common;

use axum::Router;
use axum::extract::RawQuery;
use axum::http::StatusCode;
use axum::routing::get as route_get;
use common::{App, get};
use serde_json::{Value, json};

/// メッセージの記録 1 件（届いた物）。`at` は 2026-11-28 の UTC の時刻（`03:14:00`）。
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
        "deliveredAt": format!("2026-11-28T{at}Z"),
        "createdAt": "2026-11-28T00:00:00Z", "updatedAt": "2026-11-28T00:00:00Z",
    })
}

fn reply(mut m: Value, to: &str) -> Value {
    m["replyTo"] = json!(to);
    m
}

/// 済んだ課題 K-1 の記録（依頼書 → レビュー → merged → done → 片付け）と、別の課題の記録。
fn records() -> Vec<Value> {
    vec![
        message(
            "b1",
            "K-1",
            "brief",
            "tpm",
            "K-1",
            "03:00:00",
            "http://127.0.0.1:8801/p/K-1/brief-1.html",
        ),
        message(
            "r1",
            "K-1",
            "ask-review",
            "K-1",
            "K-1.staff",
            "03:10:00",
            "http://127.0.0.1:8801/p/K-1/brief-2.html 頼み: #1 https://github.com/o/roster/pull/7",
        ),
        reply(
            message(
                "v1",
                "K-1.staff",
                "review",
                "K-1.staff",
                "K-1",
                "03:15:00",
                "http://127.0.0.1:8801/p/K-1/review-1.html 判定: よい（approve）",
            ),
            "r1",
        ),
        message(
            "m1",
            "K-1",
            "merged",
            "K-1",
            "tpm",
            "03:20:00",
            "https://github.com/o/roster/pull/7 <script>alert(1)</script>",
        ),
        message(
            "i1",
            "K-1",
            "incident",
            "K-1",
            "tpm",
            "03:25:00",
            "#9 信頼性: 何か",
        ),
        reply(
            message(
                "d1",
                "K-1",
                "done",
                "K-1",
                "tpm",
                "03:30:00",
                "http://127.0.0.1:8801/p/K-1/result-1.html 要点: 入れた",
            ),
            "b1",
        ),
        message(
            "x1",
            "K-1",
            "dismiss",
            "tpm",
            "K-1",
            "03:40:00",
            "この課題に続きは無い。",
        ),
        reply(
            message(
                "e1",
                "K-1",
                "end",
                "K-1",
                "tpm",
                "04:00:00",
                "成果: なし／残っている物: なし",
            ),
            "x1",
        ),
        message("o1", "K-10", "brief", "tpm", "K-10", "03:05:00", "別の課題"),
    ]
}

/// Key の完全一致で絞って返す（ListMessages の `key`。新しい順）偽の pagetray。候補（GetMessageFacets）は `keys`。
fn pagetray(records: Vec<Value>, keys: &[&str]) -> Router {
    let facets = json!({
        "states": [], "to": [], "from": [], "projects": [],
        "keys": keys.iter().map(|k| json!({ "value": k, "count": 1 })).collect::<Vec<_>>(),
    });
    common::pagetray_routes()
        .route(
            "/api/messages",
            route_get(move |RawQuery(query): RawQuery| {
                let records = records.clone();
                async move {
                    let query = query.unwrap_or_default();
                    let key = query
                        .split('&')
                        .find_map(|p| p.strip_prefix("key="))
                        .unwrap_or_default()
                        .to_owned();
                    let mut found: Vec<Value> =
                        records.into_iter().filter(|m| m["key"] == key).collect();
                    found.reverse();
                    common::json(StatusCode::OK, &json!({ "messages": found }).to_string())
                }
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

async fn start(records: Vec<Value>, keys: &[&str]) -> App {
    App::start_with(
        common::castlist_routes(),
        pagetray(records, keys),
        callboard::Config::default(),
    )
    .await
}

/// 画面の、1 行（`<details … data-id="<id>">` から `</details>` まで）。
fn row<'a>(page: &'a str, id: &str) -> &'a str {
    let at = page
        .find(&format!(r#"data-id="{id}""#))
        .unwrap_or_else(|| panic!("{id} の行が無い: {page}"));
    let start = page[..at].rfind("<details").unwrap();
    let rest = &page[start..];
    &rest[..rest.find("</details>").unwrap()]
}

/// 行の並び（id）。
fn shown(page: &str) -> Vec<&str> {
    page.split("<details class=\"xrow")
        .skip(1)
        .map(|rest| {
            let rest = &rest[rest.find("data-id=\"").unwrap() + 9..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect()
}

/// pagetray に聞いた、ListMessages の `key` の値（聞いた順）。
fn asked_keys(app: &App) -> Vec<String> {
    app.pagetray
        .seen()
        .into_iter()
        .filter(|s| s.uri.starts_with("/api/messages?"))
        .map(|s| {
            s.uri
                .split(['?', '&'])
                .find_map(|p| p.strip_prefix("key="))
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

#[tokio::test]
async fn 課題の記録を_席の_key_ごとに完全一致で読み_往復は棒_片道は印で_時刻の順に出す() {
    let app = start(records(), &["K-10", "K-1.staff", "K-1"]).await;
    let page = app.html("/messages/tasks/K-1").await;
    common::check_i18n("thread", &page);
    // 候補から、K-1 と K-1. で始まる Key だけを選んで聞く（K-10 は別の課題）。
    let mut keys = asked_keys(&app);
    keys.sort();
    assert_eq!(keys, ["K-1", "K-1.staff"]);
    // 返事（review・done・end）は、頼みの行に入る。行は、頼みと片道の、送った時刻の順。
    assert_eq!(shown(&page), ["b1", "r1", "m1", "i1", "x1"]);
    assert!(page.contains("<title data-i18n=\"thread.title\" data-i18n-options=\"{&#34;key&#34;:&#34;K-1&#34;}\">K-1 の筋 ・ メッセージの記録 ・ callboard</title>"), "{page}");
    assert!(
        page.contains(r#"<span class="status done" data-i18n="thread.status.done">済み</span>"#),
        "{page}"
    );
    assert!(page.contains(r#"<span data-i18n="thread.summary" data-i18n-options="{&#34;exchanges&#34;:3,&#34;n&#34;:8}">8 通・往復 3</span>"#), "{page}");
    assert!(page.contains(r#"<span data-i18n="thread.singles" data-i18n-options="{&#34;n&#34;:2}">・片道 2</span>"#), "{page}");

    // 依頼書の往復: 03:00〜03:30（課題は 03:00〜04:00 の 60 分。棒は 0 から半分まで）。球は担当。
    let brief = row(&page, "b1");
    for part in [
        r#"<time datetime="2026-11-28T03:00:00.000Z" title="2026-11-28T03:00:00.000Z">11-28 12:00</time>"#,
        r#"<span class="kind" data-i18n="thread.kind.brief">依頼書</span>"#,
        r#"<a class="chip" href="http://127.0.0.1:8801/p/K-1/brief-1.html">brief-1</a>"#,
        r#"<span class="outcome">done</span> <a class="chip" href="http://127.0.0.1:8801/p/K-1/result-1.html">result-1</a>"#,
        r#"<span data-i18n="thread.holder.seat">担当</span> <span data-i18n="thread.duration.minutes" data-i18n-options="{&#34;n&#34;:30}">30 分</span>"#,
        r#"<rect class="bar h-seat" x="0" y="4" width="500" height="12"/>"#,
        // 開くと、頼みと返事の全文。
        "<pre>【brief】http://127.0.0.1:8801/p/K-1/brief-1.html</pre>",
        "<pre>【done】http://127.0.0.1:8801/p/K-1/result-1.html 要点: 入れた</pre>",
        // 依頼書の往復には、時間の軸を絞る link。
        r#"<a class="btn" href="/messages/tasks/K-1?at=b1" data-i18n="thread.zoom">時間の軸を、この往復に絞る</a>"#,
    ] {
        assert!(brief.contains(part), "{part}: {brief}");
    }
    // レビューの往復: 03:10〜03:15。球は Staff。PR は repo#番号 の名にする。
    let review = row(&page, "r1");
    for part in [
        r#"<span class="kind" data-i18n="thread.kind.askReview">レビュー</span>"#,
        r#"<a class="chip" href="https://github.com/o/roster/pull/7">roster#7</a>"#,
        r#"<span class="outcome">review</span> <a class="chip" href="http://127.0.0.1:8801/p/K-1/review-1.html">review-1</a>"#,
        r#"<span data-i18n="thread.holder.staff">Staff（レビュー）</span> <span data-i18n="thread.duration.minutes" data-i18n-options="{&#34;n&#34;:5}">5 分</span>"#,
        r#"<rect class="bar h-staff" x="166" y="4" width="84" height="12"/>"#,
    ] {
        assert!(review.contains(part), "{part}: {review}");
    }
    assert!(!review.contains("thread.zoom"), "{review}");
    // 片付けの往復: 03:40〜04:00（返事は end）。
    assert!(
        row(&page, "x1")
            .contains(r#"<rect class="bar h-seat" x="666" y="4" width="334" height="12"/>"#),
        "{page}"
    );
    // 片道の知らせは印。本文は逃がして出す（HTML として出さない）。
    let merged = row(&page, "m1");
    assert!(
        merged.contains(r#"<rect class="mark ok" x="330" y="4" width="6" height="12"/>"#),
        "{merged}"
    );
    assert!(
        merged.contains("&#60;script&#62;alert(1)&#60;/script&#62;")
            && !merged.contains("<script>"),
        "{merged}"
    );
    assert!(row(&page, "i1").contains(r#"class="mark bad""#), "{page}");
    // 最新の 1 通は、全文を出す。
    assert!(
        page.contains("<pre>【end】成果: なし／残っている物: なし</pre>"),
        "{page}"
    );
    // 球の持ち手の帯: 担当 03:00〜03:10・Staff 〜03:15・担当 〜03:30・（空き）・担当 03:40〜04:00。合計は、担当 45 分・Staff 5 分・空き 10 分。
    for part in [
        r#"<rect class="bar h-seat" x="0" y="2" width="166" height="16"/>"#,
        r#"<rect class="bar h-staff" x="166" y="2" width="84" height="16"/>"#,
        r#"<rect class="bar h-seat" x="250" y="2" width="250" height="16"/>"#,
        r#"<rect class="bar h-seat" x="666" y="2" width="334" height="16"/>"#,
        r#"<span class="held h-seat"><span data-i18n="thread.holder.seat">担当</span> <span data-i18n="thread.duration.minutes" data-i18n-options="{&#34;n&#34;:45}">45 分</span></span>"#,
        r#"<span class="held"><span data-i18n="thread.idle">誰も持っていない</span> <span data-i18n="thread.duration.minutes" data-i18n-options="{&#34;n&#34;:10}">10 分</span></span>"#,
    ] {
        assert!(page.contains(part), "{part}: {page}");
    }
    // 目盛りは 6 つ（10 分ごと。日本の時刻）。
    assert!(page.contains(r#"<time datetime="2026-11-28T03:10:00.000Z" title="2026-11-28T03:10:00.000Z">11-28 12:10</time><time datetime="2026-11-28T03:20:00.000Z""#), "{page}");
    // みなつながっているので、古い記録の注は出さない。script は、新着の印の 1 つ。
    assert!(
        !page.contains("thread.unlinked") && !page.contains("thread.partial"),
        "{page}"
    );
    assert!(
        common::without_i18n_scripts(&page)
            .matches("<script")
            .count()
            == 1
            && page.contains(r#"<script src="/static/tasks.js?v="#),
        "{page}"
    );
}

#[tokio::test]
async fn 返事の無い頼みは_待ちの棒が今まで伸び_課題の様子に_誰の返事待ちかを出す() {
    let records = vec![
        message(
            "b1",
            "K-1",
            "brief",
            "tpm",
            "K-1",
            "03:00:00",
            "http://a/p/K-1/brief-1.html",
        ),
        message(
            "q1",
            "K-1",
            "ask-pm",
            "K-1",
            "tpm",
            "03:10:00",
            "http://a/p/K-1/confirm-1.html 項目: (1) 案",
        ),
    ];
    let app = start(records, &["K-1"]).await;
    let page = app.html("/messages/tasks/K-1").await;
    common::check_i18n("thread waiting", &page);
    // 返事待ちが 2 つ（依頼書は担当、確認は PM）。課題の様子には、帯と同じ順で前の者（PM）を出す。
    assert!(page.contains(r#"<span class="status h-pm" data-i18n="thread.status.waiting" data-i18n-options="{&#34;who&#34;:{&#34;$t&#34;:&#34;thread.holder.pm&#34;}}">PM の返事待ち</span>"#), "{page}");
    let ask = row(&page, "q1");
    assert!(
        ask.starts_with(r#"<details class="xrow waiting" data-id="q1">"#),
        "{ask}"
    );
    for part in [
        r#"<span class="kind" data-i18n="thread.kind.askPm">PM に確認</span>"#,
        r#"<span class="outcome" data-i18n="thread.outcome.waiting">まだ（返事待ち）</span>"#,
        r#"<rect class="bar h-pm open""#,
    ] {
        assert!(ask.contains(part), "{part}: {ask}");
    }
    // 軸は今まで（記録は 2026 年。今はそれより後なので、依頼書の棒は左の端から右の端まで）。
    assert!(
        row(&page, "b1")
            .contains(r#"<rect class="bar h-seat open" x="0" y="4" width="1000" height="12"/>"#),
        "{page}"
    );
    // 候補に無くても、課題の Key と Staff の席の Key は聞く。
    let app = start(Vec::new(), &[]).await;
    let (status, _, body) = app.send(get("/messages/tasks/K-1")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(
        body.contains("その Key のメッセージの記録はありません"),
        "{body}"
    );
    let mut keys = asked_keys(&app);
    keys.sort();
    assert_eq!(keys, ["K-1", "K-1.staff"]);
}

#[tokio::test]
async fn at_で_時間の軸を_その往復に絞る_重ならない行は出さず_全体へ戻る_link_を出す() {
    let app = start(records(), &["K-1", "K-1.staff"]).await;
    // レビューの往復（03:10〜03:15）に絞る。
    let page = app.html("/messages/tasks/K-1?at=r1&x=1").await;
    common::check_i18n("thread zoom", &page);
    // 重なるのは、依頼書の往復（03:00〜03:30）とレビューの往復だけ。
    assert_eq!(shown(&page), ["b1", "r1"]);
    assert!(
        row(&page, "r1")
            .contains(r#"<rect class="bar h-staff" x="0" y="4" width="1000" height="12"/>"#),
        "{page}"
    );
    assert!(page.contains(r#"<a class="btn" href="/messages/tasks/K-1" data-i18n="thread.whole">課題の全体に戻す</a>"#), "{page}");
    // 絞っている時は、絞る link を出さない。読み直す印は、今の URL。
    assert!(!page.contains("thread.zoom"), "{page}");
    assert!(
        page.contains(r#"<a class="btn" id="fresh" href="/messages/tasks/K-1?at=r1" hidden"#),
        "{page}"
    );
    // 無い id は、絞らない（課題の全体）。
    let page = app.html("/messages/tasks/K-1?at=nope").await;
    assert_eq!(shown(&page).len(), 5);
    assert!(!page.contains("thread.whole"), "{page}");
}

#[tokio::test]
async fn project_は_pagetray_に渡し_link_にも付ける() {
    let app = start(records(), &["K-1"]).await;
    let page = app.html("/messages/tasks/K-1?project=p").await;
    let asked: Vec<String> = app
        .pagetray
        .seen()
        .into_iter()
        .map(|s| s.uri)
        .filter(|u| u.starts_with("/api/messages?"))
        .collect();
    assert!(
        asked
            .iter()
            .all(|u| u.contains("project=p") && u.contains("maxResults=1000")),
        "{asked:?}"
    );
    assert!(
        page.contains(r#"href="/messages/tasks/K-1?project=p&amp;at=b1""#)
            || page.contains(r#"href="/messages/tasks/K-1?project=p&#38;at=b1""#),
        "{page}"
    );
}

#[tokio::test]
async fn 返事の先の無い古い記録は_棒にせず_数を注に出す_設定の時刻より前の頼みは_待ちにしない() {
    // 前の版の形: 返事（done）に返事の先が無い。
    let records = vec![
        message(
            "b1",
            "K-1",
            "brief",
            "tpm",
            "K-1",
            "03:00:00",
            "http://a/p/K-1/brief-1.html",
        ),
        message(
            "d1",
            "K-1",
            "done",
            "K-1",
            "tpm",
            "03:30:00",
            "http://a/p/K-1/result-1.html",
        ),
    ];
    // 設定が無ければ、依頼書は待ちに見える。
    let app = start(records.clone(), &["K-1"]).await;
    let page = app.html("/messages/tasks/K-1").await;
    assert!(
        row(&page, "b1").contains(r#"class="bar h-seat open""#),
        "{page}"
    );
    assert!(page.contains(r#"<p class="thread-note" data-i18n="thread.unlinked" data-i18n-options="{&#34;n&#34;:1}">返事の先の無い古い記録が 1 通あります（頼みとつながらないので、棒にしていません）</p>"#), "{page}");

    // 送る側が返事の先を付け始めた時刻（04:00）を設定に書くと、それより前の頼みは、片道の印で出す。
    let pagetray = common::Fake::start(pagetray(records, &["K-1"])).await;
    let castlist = common::Fake::start(common::castlist_routes()).await;
    let settings = callboard::config::Settings {
        // 2026-11-28T04:00:00Z。
        exchanges_since: Some(1_795_838_400_000),
        ..common::settings(&castlist.url(), &pagetray.url())
    };
    let router = callboard::app_with(&settings, callboard::Config::default());
    let (status, _, page) = common::send(&router, get("/messages/tasks/K-1")).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    common::check_i18n("thread since", &page);
    assert_eq!(shown(&page), ["b1", "d1"]);
    assert!(
        row(&page, "b1").contains(r#"<rect class="mark""#)
            && !page.contains("class=\"bar h-seat open\""),
        "{page}"
    );
    assert!(page.contains(r#"<span data-i18n="thread.summary" data-i18n-options="{&#34;exchanges&#34;:0,&#34;n&#34;:2}">2 通・往復 0</span>"#), "{page}");
    assert!(
        page.contains(
            r#"<span class="status idle" data-i18n="thread.status.idle">次の依頼待ち</span>"#
        ),
        "{page}"
    );
}

#[tokio::test]
async fn 形の違う_key_は_404_の画面_pagetray_に聞けなければ_503_の画面() {
    let app = start(records(), &["K-1"]).await;
    for path in [
        "/messages/tasks/K-1.staff",
        "/messages/tasks/a%20b",
        "/messages/tasks/%E3%81%82",
    ] {
        let (status, _, body) = app.send(get(path)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {body}");
        common::check_i18n(path, &body);
    }
    // 形の違う Key では、pagetray に聞かない。
    assert!(app.pagetray.seen().is_empty(), "{:?}", app.pagetray.seen());
    let down = App::start_with(
        common::castlist_routes(),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = down.send(get("/messages/tasks/K-1")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
}
