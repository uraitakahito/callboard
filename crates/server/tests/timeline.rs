//! タイムライン（`GET /graph/timeline`）。castlist の画面の試験（castlist の tests/ui.rs の「タイムライン」）のうち、
//! castlist の API（GetTimeline。偽の castlist）の答えから図を組む所を見る。帯・矢印・目盛りの置き場の細かい決まりは、
//! 並べる計算の単体の試験（`src/ui/timeline_layout.rs`。castlist から移した物）が見る。
mod common;

use axum::Router;
use axum::http::{StatusCode, header};
use axum::routing::get as route_get;
use common::castlist::{PM, SE1, SE2, TPM, link};
use common::{App, get};
use serde_json::{Value, json};

fn node(id: &str, name: &str, kind: &str, received: &str, entered: Option<&str>) -> Value {
    let mut state = json!({"present": entered.is_some(), "inferred": false});
    if let Some(entered) = entered {
        state["presence"] = json!("entered");
        state["enteredAt"] = json!(entered);
    }
    json!({
        "id": id, "name": name, "kind": kind, "issuer": "tpm",
        "sentAt": received, "receivedAt": received, "clockSkew": false, "state": state,
    })
}

fn event(actor: &str, kind: &str, received: &str) -> Value {
    json!({"actorId": actor, "type": kind, "receivedAt": received, "inferred": false})
}

fn setting(actor: &str, source: &str, model: &str, received: &str) -> Value {
    json!({
        "actorId": actor, "model": model, "effort": "high", "source": source,
        "sentAt": received, "receivedAt": received, "clockSkew": false,
    })
}

/// 見本: pm.1（人）─起動した→ tpm.1 ─起動した→ se.1 ─引き継いだ→ se.2。
/// se.1 は opus を指定して登場し、実際は sonnet に替わり、要約の後、roster が落ちたと推して閉じた。se.2 は稼働中。
fn answer() -> Value {
    json!({
        "asOf": "2026-11-22T06:00:00Z",
        "nodes": [
            node(TPM, "tpm.1", "ai", "2026-11-22T00:10:00Z", Some("2026-11-22T00:10:01Z")),
            node(PM, "pm.1", "human", "2026-11-22T00:00:00Z", Some("2026-11-22T00:00:01Z")),
            node(SE1, "se.1", "ai", "2026-11-22T01:00:00Z", Some("2026-11-22T01:00:01Z")),
            node(SE2, "se.2", "ai", "2026-11-22T04:00:00Z", Some("2026-11-22T04:00:01Z")),
        ],
        "edges": [
            link("l-pm-tpm", "launched", PM, TPM),
            link("l-tpm-se1", "launched", TPM, SE1),
            link("l-se1-se2", "continued", SE1, SE2),
        ],
        "events": [
            event(PM, "entered", "2026-11-22T00:00:01Z"),
            event(TPM, "entered", "2026-11-22T00:10:01Z"),
            event(SE1, "entered", "2026-11-22T01:00:01Z"),
            event(SE1, "compacted", "2026-11-22T02:00:00Z"),
            {"actorId": SE1, "type": "closed", "receivedAt": "2026-11-22T03:30:00Z", "inferred": true, "reason": "workspace_gone"},
            event(SE2, "entered", "2026-11-22T04:00:01Z"),
        ],
        "settings": [
            setting(TPM, "requested", "claude-opus-5-5", "2026-11-22T00:10:00Z"),
            setting(SE1, "requested", "claude-opus-5-5", "2026-11-22T01:00:00Z"),
            setting(SE1, "observed", "claude-sonnet-5", "2026-11-22T01:30:00Z"),
            setting(SE2, "requested", "claude-haiku-5", "2026-11-22T04:00:00Z"),
        ],
    })
}

fn castlist(answer: Value) -> Router {
    common::castlist_routes().route(
        "/api/timeline",
        route_get(move || {
            let answer = answer.clone();
            async move { common::json(StatusCode::OK, &answer.to_string()) }
        }),
    )
}

async fn start(answer: Value) -> App {
    App::start_with(
        castlist(answer),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await
}

/// 図（`<svg class="timeline" …>` から `</svg>` まで）。
fn svg(body: &str) -> &str {
    let start = body.find("<svg class=\"timeline\"").expect("svg");
    &body[start..start + body[start..].find("</svg>").unwrap()]
}

/// id のアクターの行（`<g class="lane …">` の中身）。
fn lane<'a>(svg: &'a str, id: &str) -> &'a str {
    let at = svg
        .find(&format!("<a href=\"/actors/{id}\">"))
        .unwrap_or_else(|| panic!("{id} の行が無い"));
    let start = svg[..at].rfind("<g class=\"lane").unwrap();
    &svg[start..start + svg[start..].find("</g>").unwrap()]
}

/// 行の帯（`<rect class="seg …">` の class と title）。
fn segs(lane: &str) -> Vec<(String, String)> {
    lane.split("<rect class=\"")
        .skip(1)
        .map(|s| {
            let class = &s[..s.find('"').unwrap()];
            let at = s.find("<title>").unwrap() + 7;
            (
                class.to_owned(),
                s[at..at + s[at..].find("</title>").unwrap()].to_owned(),
            )
        })
        .collect()
}

#[tokio::test]
async fn タイムラインは_アクターごとに_1_行で_人を上に初めの登場の順に並べ_名はプロフィールへの_link()
 {
    let app = start(answer()).await;
    let body = app.html("/graph/timeline").await;
    assert!(
        body.contains("<title>タイムライン ・ callboard</title>"),
        "{body}"
    );
    assert!(body.contains("4 のアクター。"), "{body}");
    let svg = svg(&body);
    // 答えの順は tpm.1 が先でも、人の pm.1 を上に、ほかは初めの登場の順に。
    let order: Vec<usize> = [PM, TPM, SE1, SE2]
        .iter()
        .map(|id| svg.find(&format!("<a href=\"/actors/{id}\">")).unwrap())
        .collect();
    assert!(order.is_sorted(), "{order:?}");
    assert!(lane(svg, PM).starts_with("<g class=\"lane human\">"));
    assert!(lane(svg, PM).contains("<text class=\"sub\"") && lane(svg, PM).contains(">人</text>"));
    assert!(lane(svg, SE1).starts_with("<g class=\"lane ai\">"));
    assert!(!lane(svg, SE1).contains("class=\"sub\""));
    assert!(lane(svg, SE2).contains(">se.2</text></a>"));
}

#[tokio::test]
async fn タイムラインは_model_の替わり目で帯を区切り_推しの幕引きと_稼働中の端と_印と_線を描く() {
    let app = start(answer()).await;
    let body = app.html("/graph/timeline").await;
    let svg = svg(&body);
    // se.1: opus（指定）→ sonnet（実際。指定と違う）→ 最後の便り（要約）の後は推しの帯。
    let se1 = segs(lane(svg, SE1));
    assert_eq!(se1.len(), 3, "{se1:?}");
    assert_eq!(se1[0].0, "seg m-opus");
    assert!(
        se1[0].1.starts_with("se.1 · opus-5-5 · high（指定）"),
        "{}",
        se1[0].1
    );
    assert_eq!(se1[1].0, "seg m-sonnet");
    assert!(se1[1].1.contains("（実際・指定と違う）"), "{}", se1[1].1);
    assert_eq!(se1[2].0, "seg m-sonnet inferred");
    assert!(
        se1[2].1.ends_with("（推し: 居場所が消えた）"),
        "{}",
        se1[2].1
    );
    // 時刻は日本時間。
    assert!(se1[0].1.contains("2026-11-22 10:00:01"), "{}", se1[0].1);
    // 印: 要約。
    assert!(
        lane(svg, SE1).contains("<line class=\"mark compacted\""),
        "{svg}"
    );
    assert!(lane(svg, SE1).contains("<title>要約 · 2026-11-22 11:00:00</title>"));
    // se.2 は稼働中: 右端を尖らせる。
    let se2 = lane(svg, SE2);
    assert!(se2.contains("class=\"open-end\""), "{se2}");
    assert_eq!(segs(se2)[0].0, "seg m-haiku");
    // 人の pm.1 は、model の記録が無い。
    assert_eq!(segs(lane(svg, PM))[0].0, "seg m-none");
    // 線: 起動した 2 本（実線）と、引き継いだ 1 本（点線）。
    assert_eq!(
        svg.matches("<path class=\"link launched\"").count(),
        2,
        "{svg}"
    );
    assert_eq!(
        svg.matches("<path class=\"link continued\"").count(),
        1,
        "{svg}"
    );
    // 今（castlist が答えた時刻）の線と、表示した時刻。
    assert!(svg.contains("<line class=\"now\""), "{svg}");
    assert!(
        body.contains("表示した時刻: <time datetime=\"2026-11-22T06:00:00.000Z\""),
        "{body}"
    );
    // 凡例は、出てくる model（短い名の順）。
    let at = body.find("<ul class=\"keys\">").unwrap();
    let keys = &body[at..at + body[at..].find("</ul>").unwrap()];
    let names: Vec<usize> = ["haiku-5", "opus-5-5", "sonnet-5", "記録なし"]
        .iter()
        .map(|m| keys.find(m).unwrap())
        .collect();
    assert!(names.is_sorted(), "{keys}");
    assert!(
        keys.contains("<span class=\"swatch m-haiku\"></span>haiku-5"),
        "{keys}"
    );
}

#[tokio::test]
async fn タイムラインの絞りは_castlist_の_api_に渡し_期間を付ければ横もその窓にする() {
    let app = start(answer()).await;
    let body = app
        .html("/graph/timeline?label=tpm.key:t-23zs&from=2026-11-22T10:30&to=2026-11-22T03:00:00Z")
        .await;
    let seen = app.castlist.seen();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0].header("host"), Some("127.0.0.1"));
    assert_eq!(
        seen[0].uri,
        "/api/timeline?label=tpm.key%3At-23zs&from=2026-11-22T01%3A30%3A00Z&to=2026-11-22T03%3A00%3A00Z"
    );
    // 窓（10:30〜12:00 日本時間）の外は切る: 窓の左で切った帯は、左端を尖らせる。今の線は窓の外。
    let svg = svg(&body);
    assert!(lane(svg, SE1).contains("class=\"open-start\""), "{svg}");
    assert!(!svg.contains("<line class=\"now\""), "{svg}");
    // form に今の値（期間は日本時間の字）。
    assert!(body.contains("name=\"label\" value=\"tpm.key:t-23zs\""));
    assert!(body.contains("name=\"from\" value=\"2026-11-22T10:30\""));
    assert!(body.contains("name=\"to\" value=\"2026-11-22T12:00\""));
    assert!(body.contains("<form class=\"filters\" method=\"get\" action=\"/graph/timeline\">"));
    assert!(
        body.contains("<a href=\"/actors/\">一覧へ</a> ・ <a href=\"/graph/network\">相関図へ</a>"),
        "{body}"
    );
    assert!(
        body.contains("href=\"/graph/timeline?label=tpm.key:t-23zs&#38;from=20"),
        "{body}"
    );
}

#[tokio::test]
async fn タイムラインに当たる者が無ければ_そう書き_svg_を出さない() {
    let app = start(json!({"asOf": "2026-11-22T06:00:00Z", "nodes": [], "edges": [], "events": [], "settings": []})).await;
    let body = app.html("/graph/timeline?label=a:b").await;
    assert!(body.contains("条件に合うアクターはいません。条件を減らすか、<a href=\"/graph/timeline\">条件を消して</a>ください。"), "{body}");
    assert!(!body.contains("<svg"), "{body}");
}

#[tokio::test]
async fn タイムラインの引数の誤りは_引数の名を書いた_400_の画面で_castlist_に聞かない() {
    let app = start(answer()).await;
    for (uri, word) in [
        (
            "/graph/timeline?kind=ai",
            "知らない引数: kind（使えるのは label・from・to）",
        ),
        (
            "/graph/timeline?label=a:b&label=c:d",
            "label は 1 つだけ付ける",
        ),
        (
            "/graph/timeline?label=nocolon",
            "label は「鍵:値」の形（nocolon）",
        ),
        (
            "/graph/timeline?from=yesterday",
            "「登録した期間（から）」の日時を読めません。",
        ),
    ] {
        let (status, headers, body) = app.send(get(uri)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        assert!(body.contains(word), "{uri}: {word}: {body}");
    }
    assert!(app.castlist.seen().is_empty());
}

#[tokio::test]
async fn タイムラインは_script_も_inline_の_style_も使わず_利用者の字を_html_で逃がす() {
    let mut answer = answer();
    answer["nodes"][2]["name"] = json!("<b>se</b>.1");
    answer["settings"][2]["model"] = json!("claude-<i>x</i>");
    let app = start(answer).await;
    let (_, headers, body) = app.send(get("/graph/timeline")).await;
    assert!(headers.get(header::CONTENT_SECURITY_POLICY).is_some());
    for raw in ["<script", "<style", "style=", "<b>", "<i>"] {
        assert!(!body.contains(raw), "{raw}");
    }
    assert!(body.contains("&#60;b&#62;se&#60;/b&#62;.1"), "{body}");
}

#[tokio::test]
async fn castlist_に聞けなければ_503_の画面で_訳を書き_聞く先を出さない() {
    let down = common::nowhere().await;
    let pagetray = common::Fake::start(common::pagetray_routes()).await;
    let router = callboard::app(&common::settings(&down, &pagetray.url()));
    let (status, _, body) = common::send(&router, get("/graph/timeline")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body.contains("<h1>503</h1>") && body.contains("castlist に聞けません（つながりません）"),
        "{body}"
    );
    assert!(!body.contains(&down), "{body}");

    // 1.1.0 より古い castlist（GetTimeline を持たない。404）でも、503 の画面で訳を書く。
    let app = App::start_with(
        common::castlist_routes(),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get("/graph/timeline")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body.contains("castlist に聞けません（404 が返りました）"),
        "{body}"
    );
}
