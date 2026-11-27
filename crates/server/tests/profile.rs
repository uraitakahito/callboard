//! プロフィール（`GET /actors/{id}`）。castlist の画面の試験（castlist の tests/ui.rs の「プロフィール」）を、
//! castlist の API の答え（偽の castlist）から組む形に移した物。
mod common;

use axum::Router;
use axum::http::{Request, StatusCode, header};
use axum::routing::get as route_get;
use common::castlist::{MISSING, PM, SE1, SE2, SUB, TPM, se1_actor, se1_lineage};
use common::{App, get};
use serde_json::json;

/// 偽の castlist（se.1 の答え）で起こす。
async fn start() -> App {
    start_with(se1_actor(), se1_lineage()).await
}

async fn start_with(actor: serde_json::Value, lineage: serde_json::Value) -> App {
    App::start_with(
        common::castlist::routes(actor, lineage),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await
}

/// `id` の `<section>` の中身。
fn section<'a>(body: &'a str, id: &str) -> &'a str {
    let open = format!("<section id=\"{id}\">");
    let start = body
        .find(&open)
        .unwrap_or_else(|| panic!("{id} の節が無い: {body}"));
    let rest = &body[start..];
    &rest[..rest.find("</section>").expect("節の終わり")]
}

/// 時刻の字（`<time>`。ISO の字を datetime と title に、日本の時刻を中に）。
fn time(iso: &str, local: &str) -> String {
    format!("<time datetime=\"{iso}\" title=\"{iso}\">{local}</time>")
}

#[tokio::test]
async fn プロフィールは_名_id_issuer_ラベル_今の状態を出す() {
    let app = start().await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    common::check_i18n("profile.rs:44", &body);
    assert!(
        body.contains(r#"<title data-i18n="profile.title" data-i18n-options="{&#34;name&#34;:&#34;se.1&#34;}">se.1 ・ callboard</title>"#),
        "{body}"
    );
    // AI には種類の印を付けない。
    assert!(body.contains("<h1>se.1</h1>"), "{body}");
    assert!(body.contains(&format!("<dt>id</dt><dd><code>{SE1}</code></dd>")));
    assert!(body.contains(
        "<dt data-i18n=\"profile.issuer\">発行元（issuer）</dt><dd><code>tpm</code></dd>"
    ));
    // 時刻は castlist が受けた時刻を、日本の時刻で（ISO の字は UTC のミリ秒まで）。
    assert!(body.contains(&format!(
        "<dt data-i18n=\"profile.registered\">登録した時刻</dt><dd>{}</dd>",
        time("2026-11-22T01:00:00.500Z", "2026-11-22 10:00:00")
    )));

    let state = section(&body, "state");
    assert!(
        state.contains("<span class=\"state off\" data-i18n=\"common.state.stopped\">停止</span>"),
        "{state}"
    );
    assert!(!state.contains("推し"), "{state}");
    // 稼働を替えた最新の出来事（日本語の名。id は title に）。
    assert!(
        state.contains(
            "<span class=\"event\" title=\"cleared\" data-i18n=\"events.cleared\">交代</span>"
        ),
        "{state}"
    );
    assert!(
        state.contains("<code>claude-sonnet-5</code> · <code>high</code>"),
        "{state}"
    );
    assert!(state.contains(&format!(
        "（<span data-i18n=\"profile.source.observed\">実際</span>・{}）",
        time("2026-11-22T02:30:00.000Z", "2026-11-22 11:30:00")
    )));
    assert!(state.contains(&format!(
        "<dt data-i18n=\"list.col.firstEntered\">最初の登場</dt><dd>{}</dd>",
        time("2026-11-22T01:00:01.000Z", "2026-11-22 10:00:01")
    )));
    assert!(state.contains(&format!(
        "<dt data-i18n=\"list.col.lastExited\">最後の退場</dt><dd>{}</dd>",
        time("2026-11-22T04:00:00.000Z", "2026-11-22 13:00:00")
    )));
    // 出来事に時刻ずれの行があるので、状態に印が付く。
    assert!(state.contains("class=\"skew\""), "{state}");

    // ラベルは鍵の順。
    let labels = section(&body, "labels");
    let at = |key: &str| {
        labels
            .find(&format!("<span class=\"muted\">{key}</span>"))
            .unwrap()
    };
    assert!(
        at("note") < at("tpm.key") && at("tpm.key") < at("tpm.role"),
        "{labels}"
    );
    assert!(
        labels.contains("<span class=\"muted\">tpm.key</span> t-23zs"),
        "{labels}"
    );

    // 表示した時刻は、castlist の答えの時刻。
    assert!(body.contains(&format!(
        "<span data-i18n=\"common.asOf\">表示した時刻:</span> {}",
        time("2026-11-22T06:00:00.000Z", "2026-11-22 15:00:00")
    )));
}

#[tokio::test]
async fn プロフィールの外部の_id_は_受けた順に出し_鍵と今の物に印を付ける() {
    let app = start().await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    let bindings = section(&body, "bindings");
    let rows: Vec<&str> = bindings.split("<tr").skip(2).collect();
    assert_eq!(rows.len(), 3, "{bindings}");
    assert!(
        rows[0].contains("<code>s-se1</code>")
            && rows[0].contains("キー")
            && rows[0].contains(">今<")
    );
    // 替わる前の値は、過ぎた行。
    assert!(rows[1].starts_with(" class=\"past\"") && rows[1].contains("<code>workspace:1</code>"));
    assert!(!rows[1].contains(">今<") && !rows[1].contains("キー"));
    assert!(rows[2].contains("<code>workspace:2</code>") && rows[2].contains(">今<"));
}

#[tokio::test]
async fn プロフィールは_起動した人を人の起点まで_起動したアクター_引き継ぎを出す() {
    let app = start().await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    let chain = section(&body, "launched-by");
    let pm = format!(
        "<a href=\"/actors/{PM}\">pm.1</a> <span class=\"muted\"><span data-i18n=\"common.kind.human\">人</span>・<span data-i18n=\"profile.chain.launched\">起動した</span></span>"
    );
    let tpm = format!(
        "<a href=\"/actors/{TPM}\">tpm.1</a> <span class=\"muted\"><span data-i18n=\"profile.chain.launched\">起動した</span></span>"
    );
    assert!(chain.contains(&pm), "{chain}");
    assert!(chain.contains(&tpm), "{chain}");
    assert!(
        chain.find(&pm).unwrap() < chain.find(&tpm).unwrap(),
        "{chain}"
    );
    assert!(chain.contains("<li><strong>se.1</strong></li>"), "{chain}");

    let children = section(&body, "children");
    assert!(
        children.contains(&format!("<a href=\"/actors/{SUB}\">sub.1</a>")),
        "{children}"
    );
    assert!(!children.contains("se.2"), "{children}");

    let continued = section(&body, "continued");
    assert!(
        continued.contains("<dt data-i18n=\"profile.continued.before\">引き継ぎ元</dt><dd><span class=\"none\">—</span></dd>"),
        "{continued}"
    );
    assert!(
        continued.contains(&format!("<a href=\"/actors/{SE2}\">se.2</a>")),
        "{continued}"
    );
}

#[tokio::test]
async fn 起点のアクターは_起動した人が無いと書き_人には種類の印を付ける() {
    // pm.1（人。起点）。辿った先は自分だけ。
    let mut actor = se1_actor();
    actor["id"] = json!(SE1);
    actor["name"] = json!("pm.1");
    actor["kind"] = json!("human");
    actor["linksIn"] = json!([]);
    actor["linksOut"] = json!([]);
    actor["settings"] = json!([]);
    actor.as_object_mut().unwrap().remove("currentSettings");
    actor["events"] = json!([]);
    actor["state"] = json!({"present": false, "inferred": false});
    let mut me = common::castlist::row(SE1, "pm.1", "human");
    me["present"] = json!(false);
    me.as_object_mut().unwrap().remove("presence");
    let lineage = json!({
        "asOf": "2026-11-22T06:00:00Z", "actorId": SE1, "nodes": [me], "edges": [],
    });
    let app = start_with(actor, lineage).await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    assert!(
        body.contains(
            "<h1>pm.1 <span class=\"kind\" data-i18n=\"common.kind.human\">人</span></h1>"
        ),
        "{body}"
    );
    assert!(section(&body, "launched-by").contains("ありません（起点）。"));
    assert!(section(&body, "children").contains("ありません。"));
    assert!(section(&body, "settings").contains("ありません。"));
    assert!(section(&body, "events").contains("ありません。"));
    let state = section(&body, "state");
    // 稼働を替える出来事も、model と effort も、登場も無い。
    assert!(!state.contains("class=\"event\""), "{state}");
    assert!(
        state.contains("<dt>model · effort</dt><dd><span class=\"none\">—</span></dd>"),
        "{state}"
    );
    assert!(
        state.contains("<dt data-i18n=\"list.col.firstEntered\">最初の登場</dt><dd><span class=\"none\">—</span></dd>"),
        "{state}"
    );
    assert!(!state.contains("class=\"skew\""), "{state}");
}

#[tokio::test]
async fn プロフィールの_model_と_effort_の履歴は_指定と違う実際の行に食い違いの印を付ける() {
    let app = start().await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    let settings = section(&body, "settings");
    let rows: Vec<&str> = settings.split("<tr>").skip(2).collect();
    assert_eq!(rows.len(), 4, "{settings}");
    assert!(
        rows[0].contains("<td data-i18n=\"profile.source.requested\">指定</td>")
            && !rows[0].contains("class=\"mismatch\"")
    );
    // 指定は opus、実際は sonnet。
    assert!(
        rows[1].contains("<td data-i18n=\"profile.source.observed\">実際</td>")
            && rows[1].contains("class=\"mismatch\""),
        "{}",
        rows[1]
    );
    // 指定し直した後の実際は、合っている。
    assert!(rows[2].contains("<td data-i18n=\"profile.source.requested\">指定</td>"));
    assert!(
        rows[3].contains("<td data-i18n=\"profile.source.observed\">実際</td>")
            && !rows[3].contains("class=\"mismatch\""),
        "{}",
        rows[3]
    );
}

#[tokio::test]
async fn プロフィールの出来事は_日本語の名と理由ときっかけと送り手の時刻と_時刻ずれの印を出す() {
    let app = start().await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    let events = section(&body, "events");
    let rows: Vec<&str> = events.split("<tr>").skip(2).collect();
    assert_eq!(rows.len(), 3, "{events}");
    assert!(rows[0].contains(
        "<span class=\"event\" title=\"entered\" data-i18n=\"events.entered\">登場</span>"
    ));
    assert!(!rows[0].contains("<code"), "{}", rows[0]);
    // reason と source は欄に分け、残りの detail は鍵の順の JSON の字。
    assert!(rows[1].contains(
        "<span class=\"event\" title=\"compacted\" data-i18n=\"events.compacted\">要約</span>"
    ));
    assert!(rows[1].contains("<code>context_full</code>"), "{}", rows[1]);
    assert!(rows[1].contains("<code>auto</code>"), "{}", rows[1]);
    assert!(
        rows[1].contains("<code class=\"muted\">{&#34;ok&#34;:true,&#34;tokens&#34;:1200}</code>"),
        "{}",
        rows[1]
    );
    // 送り手の時刻は別の欄。ずれた行に印。
    assert!(rows[2].contains(
        "<span class=\"event\" title=\"cleared\" data-i18n=\"events.cleared\">交代</span>"
    ));
    assert!(rows[2].contains(&time("2026-11-22T04:00:00.000Z", "2026-11-22 13:00:00")));
    assert!(rows[2].contains(&time("2026-11-22T05:00:00.000Z", "2026-11-22 14:00:00")));
    assert!(rows[2].contains("class=\"skew\"") && rows[2].contains("時刻ずれ"));
    assert!(!rows[1].contains("class=\"skew\""));
}

#[tokio::test]
async fn 時刻ずれの印と_稼働を替えた出来事は_辿った先の自分の行から読む() {
    // castlist 1.1.0 から、辿った先の行（ActorRow）に skewed と presence がある（一覧の行と同じ値）。
    let state = |lineage: serde_json::Value| async move {
        let app = start_with(se1_actor(), lineage).await;
        let body = app.html(&format!("/actors/{SE1}")).await;
        section(&body, "state").to_owned()
    };
    let with = |skewed: Option<bool>, presence: Option<&str>| {
        let mut lineage = se1_lineage();
        let me = lineage["nodes"][2].as_object_mut().unwrap();
        match skewed {
            Some(v) => me.insert("skewed".to_owned(), json!(v)),
            None => me.remove("skewed"),
        };
        match presence {
            Some(v) => me.insert("presence".to_owned(), json!(v)),
            None => me.remove("presence"),
        };
        lineage
    };
    let on = state(with(Some(true), Some("exited"))).await;
    assert!(on.contains("class=\"skew\""), "{on}");
    assert!(
        on.contains(
            "<span class=\"event\" title=\"exited\" data-i18n=\"events.exited\">退場</span>"
        ),
        "{on}"
    );
    let off = state(with(Some(false), Some("cleared"))).await;
    assert!(!off.contains("class=\"skew\""), "{off}");
    // 欄が無い答え（castlist が 1.1.0 より古い）では、印も出来事も出さない（出来事の行にずれがあっても）。
    let old = state(with(None, None)).await;
    assert!(
        !old.contains("class=\"skew\"") && !old.contains("class=\"event\""),
        "{old}"
    );
}

#[tokio::test]
async fn 推しの幕引きで終わった者は_停止_推しと書く() {
    let mut actor = se1_actor();
    actor["state"] = json!({"presence": "closed", "present": false, "inferred": true});
    let mut lineage = se1_lineage();
    lineage["nodes"][2]["inferred"] = json!(true);
    lineage["nodes"][2]["presence"] = json!("closed");
    let app = start_with(actor, lineage).await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    let state = section(&body, "state");
    assert!(
        state.contains("<span class=\"state off\" data-i18n=\"common.state.stoppedInferred\">停止（推し）</span>"),
        "{state}"
    );
    assert!(
        state.contains(
            "<span class=\"event\" title=\"closed\" data-i18n=\"events.closed\">幕引き</span>"
        ),
        "{state}"
    );
}

#[tokio::test]
async fn 稼働中の者は_稼働中と書く() {
    let mut actor = se1_actor();
    actor["state"] = json!({"presence": "entered", "present": true, "inferred": false});
    let mut lineage = se1_lineage();
    lineage["nodes"][2]["present"] = json!(true);
    let app = start_with(actor, lineage).await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    assert!(
        section(&body, "state")
            .contains("<span class=\"state on\" data-i18n=\"common.state.running\">稼働中</span>")
    );
}

#[tokio::test]
async fn 名を引けないアクターは_id_のまま出す() {
    // 辿った先に、線の向こうの端がいない（2 つの読みの間に足された線）。
    let mut lineage = se1_lineage();
    lineage["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|n| n["id"] != SUB);
    let app = start_with(se1_actor(), lineage).await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    let children = section(&body, "children");
    assert!(
        children.contains(&format!("<a href=\"/actors/{SUB}\">{SUB}</a>")),
        "{children}"
    );
}

#[tokio::test]
async fn 利用者の字は_html_で逃がす() {
    let mut actor = se1_actor();
    actor["issuer"] = json!("<i>x</i>");
    actor["events"][1]["detail"] = json!({"reason": "<script>alert(1)</script>", "x": "<b>"});
    actor["bindings"][0]["value"] = json!("<img src=x>");
    let mut lineage = se1_lineage();
    lineage["nodes"][2]["name"] = json!("<u>se</u>.1");
    lineage["nodes"][1]["name"] = json!("<u>tpm</u>.1");
    let app = start_with(actor, lineage).await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    let page = common::without_i18n_scripts(&body);
    for raw in ["<script", "<b>", "<i>", "<u>", "<img"] {
        assert!(!page.contains(raw), "{raw}: {body}");
    }
    assert!(body.contains("&#60;u&#62;se&#60;/u&#62;.1"), "{body}");
    assert!(
        body.contains("&#60;b&#62;太字&#60;/b&#62; &#38; &#34;引用&#34;"),
        "{body}"
    );
}

#[tokio::test]
async fn プロフィールは_inline_の_script_と_style_が無く_頭の_link_を出す() {
    let app = start().await;
    let (_, headers, body) = app.send(get(&format!("/actors/{SE1}"))).await;
    let page = common::without_i18n_scripts(&body);
    for inline in ["<script", "<style", "style="] {
        assert!(!page.contains(inline), "{inline}");
    }
    assert!(headers.get(header::CONTENT_SECURITY_POLICY).is_some());
    assert!(body.contains("<a class=\"home\" href=\"/\">callboard</a>"));
    assert!(body.contains("<nav"), "{body}");
}

#[tokio::test]
async fn castlist_には_host_127_0_0_1_で_3_つを聞き_browser_の_header_を渡さない() {
    let app = start().await;
    let request = Request::get(format!("/actors/{SE1}"))
        .header(header::HOST, "localhost:8803")
        .header(header::COOKIE, "a=b")
        .header(header::REFERER, "http://localhost:8803/graph/network")
        .body(axum::body::Body::empty())
        .unwrap();
    assert_eq!(app.send(request).await.0, StatusCode::OK);
    let seen = app.castlist.seen();
    let mut uris: Vec<&str> = seen.iter().map(|s| s.uri.as_str()).collect();
    uris.sort_unstable();
    // 3 つ目は、使用量の節の数（ListUsage）。
    let uris: Vec<&str> = uris.iter().map(|u| u.split('?').next().unwrap()).collect();
    assert_eq!(
        uris,
        [
            format!("/api/actors/{SE1}"),
            format!("/api/actors/{SE1}/lineage"),
            "/api/usage".to_owned(),
        ]
    );
    for s in &seen {
        assert_eq!(s.method, "GET");
        assert_eq!(s.header("host"), Some("127.0.0.1"));
        for name in ["origin", "cookie", "referer"] {
            assert_eq!(s.header(name), None, "{name}");
        }
    }
    assert!(app.pagetray.seen().is_empty());
}

#[tokio::test]
async fn 知らない_id_のプロフィールは_404_の画面() {
    let app = start().await;
    let (status, _, body) = app.send(get(&format!("/actors/{MISSING}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(
        body.contains("<h1>404</h1>") && body.contains("そのアクターは無い"),
        "{body}"
    );
    // アクター 1 つ・辿った先・数の 3 つを、一緒に聞く。
    assert_eq!(app.castlist.seen().len(), 3);

    // id の形でない物は、castlist に聞かずに 404。
    let app = start().await;
    for id in ["no-such", "019A0000-0000-7000-8000-000000000003", "..%2Fx"] {
        let (status, _, body) = app.send(get(&format!("/actors/{id}"))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{id}: {body}");
        assert!(body.contains("そのアクターは無い"), "{body}");
    }
    assert!(app.castlist.seen().is_empty());
}

#[tokio::test]
async fn 辿った先に自分がいなければ_404_の画面() {
    let mut lineage = se1_lineage();
    lineage["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|n| n["id"] != SE1);
    let app = start_with(se1_actor(), lineage).await;
    let (status, _, body) = app.send(get(&format!("/actors/{SE1}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn castlist_に聞けなければ_503_の画面で_訳を書き_聞く先を出さない() {
    // 止まっている。
    let down = common::nowhere().await;
    let pagetray = common::Fake::start(common::pagetray_routes()).await;
    let router = callboard::app(&common::settings(&down, &pagetray.url()));
    let (status, headers, body) = common::send(&router, get(&format!("/actors/{SE1}"))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    assert!(body.contains("<h1>503</h1>"), "{body}");
    assert!(
        body.contains("castlist に聞けません（つながりません）"),
        "{body}"
    );
    assert!(!body.contains(&down), "{body}");

    // 中で失敗した（500）。castlist の誤りの字は出さない。
    let failing = || async {
        common::castlist::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            "秘密の訳",
        )
    };
    let app = App::start_with(
        Router::new()
            .route("/api/actors/{id}", route_get(failing))
            .route("/api/actors/{id}/lineage", route_get(failing)),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get(&format!("/actors/{SE1}"))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body.contains("castlist に聞けません（500 が返りました）"),
        "{body}"
    );
    assert!(!body.contains("秘密の訳"), "{body}");
}

// --- 節「使用量」（castlist の ListUsage。直近 31 日の、そのアクターの行） ---

use common::castlist::{usage_actor, usage_buckets, usage_page, usage_row, with_usage};

/// se.1 の数の行: 01 時（UTC）に本体が 2 行（$1.50 と $0.50。文脈は 6.1 万と 23 万）と手伝い（$0.25。文脈 90 万は数えない）、
/// 02 時は無し、03 時に本体（$1.00。文脈 12 万）。
fn usage() -> serde_json::Value {
    let counts = |turns: i64, cost: i64, max_context: i64| json!({"turns": turns, "costMicroUsd": cost, "maxContext": max_context, "buckets": usage_buckets(&[(0, turns, cost)])});
    usage_page(
        vec![
            usage_row(
                "u-1",
                SE1,
                None,
                "2026-11-22T01:40:00Z",
                counts(3, 1_500_000, 61_000),
            ),
            usage_row(
                "u-2",
                SE1,
                None,
                "2026-11-22T01:50:00Z",
                counts(2, 500_000, 230_000),
            ),
            usage_row(
                "u-3",
                SE1,
                Some("Explore"),
                "2026-11-22T01:40:00Z",
                counts(4, 250_000, 900_000),
            ),
            usage_row(
                "u-4",
                SE1,
                None,
                "2026-11-22T03:20:00Z",
                counts(1, 1_000_000, 120_000),
            ),
        ],
        vec![usage_actor(SE1, "se.1", json!({}))],
        None,
    )
}

/// 要約の出来事（03:00）の detail に、前と後の文脈の長さを足した se.1。
fn compacted_actor() -> serde_json::Value {
    let mut actor = se1_actor();
    actor["events"][1]["detail"]["preTokens"] = json!(180_000);
    actor["events"][1]["detail"]["postTokens"] = json!(30_000);
    actor
}

async fn start_with_usage(actor: serde_json::Value, usage: serde_json::Value) -> App {
    App::start_with(
        with_usage(common::castlist::routes(actor, se1_lineage()), vec![usage]),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await
}

/// `<dt>名</dt><dd>値</dd>` の値（`<dt>` には字の鍵の属性が付く）。
fn fact<'a>(section: &'a str, name: &str) -> &'a str {
    let open = format!("\">{name}</dt><dd>");
    let start = section
        .find(&open)
        .unwrap_or_else(|| panic!("{name} が無い: {section}"))
        + open.len();
    &section[start..start + section[start..].find("</dd>").unwrap()]
}

#[tokio::test]
async fn 使用量の節は_直近_31_日の_そのアクターの額と回と_いちばん長い文脈を出す() {
    let app = start_with_usage(compacted_actor(), usage()).await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    common::check_i18n("profile.rs:580", &body);
    let usage = section(&body, "usage");
    assert!(
        usage.contains("<h2 data-i18n=\"profile.usage.heading\">使用量（直近 31 日）</h2>"),
        "{usage}"
    );
    assert_eq!(
        fact(usage, "額"),
        r#"$3.25<span data-i18n="profile.usage.costSplit" data-i18n-options="{&#34;helper&#34;:&#34;$0.25&#34;,&#34;main&#34;:&#34;$3.00&#34;}">（本体 $3.00・手伝い $0.25）</span>"#
    );
    assert_eq!(
        fact(usage, "回"),
        r#"10<span data-i18n="profile.usage.helperTurns" data-i18n-options="{&#34;n&#34;:&#34;4&#34;}">（手伝い 4）</span>"#
    );
    assert_eq!(fact(usage, "いちばん長い文脈"), common::tokens("23", "230"));
    // castlist には、そのアクターの行を、今を時の境に切り上げた時刻までの 31 日で聞く。
    let seen = app.castlist.seen();
    let usage_uri = seen
        .iter()
        .find(|s| s.uri.starts_with("/api/usage?"))
        .map(|s| s.uri.clone())
        .expect("数を聞いていない");
    let query: Vec<(String, String)> = usage_uri
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .map(|p| {
            let (n, v) = p.split_once('=').unwrap();
            (n.to_owned(), v.replace("%3A", ":"))
        })
        .collect();
    let one = |name: &str| {
        query
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(one("actorId"), Some(SE1));
    assert_eq!(one("label"), None);
    let at = |name: &str| {
        time::OffsetDateTime::parse(
            one(name).unwrap(),
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap()
    };
    let (from, to) = (at("from"), at("to"));
    let now = time::OffsetDateTime::now_utc();
    assert_eq!((to.minute(), to.second()), (0, 0));
    assert!(
        to >= now - time::Duration::seconds(5) && to - now <= time::Duration::HOUR,
        "{to}"
    );
    assert_eq!(to - from, time::Duration::days(31));
}

#[tokio::test]
async fn 使用量の節の線は_時ごとの本体のいちばん長い文脈の段々で_圧縮の印を立てる() {
    let app = start_with_usage(compacted_actor(), usage()).await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    common::check_i18n("profile.rs:640", &body);
    let usage = section(&body, "usage");
    let start = usage.find("<svg class=\"chart\"").expect("図が無い");
    let svg = &usage[start..start + usage[start..].find("</svg>").unwrap()];
    assert!(svg.contains("role=\"img\""), "{svg}");
    assert!(
        svg.contains(
            "aria-label=\"文脈の長さの段々の線（1 時間ごとの、本体の回のいちばん長い文脈）。"
        ),
        "{svg}"
    );
    // 01 時（23 万）と 03 時（12 万）。02 時は記録が無いので、線を切る（M が 2 つ）。
    let at = svg.find("<path class=\"context\" d=\"").expect("線が無い") + 25;
    let d = &svg[at..at + svg[at..].find('"').unwrap()];
    assert_eq!(d.matches('M').count(), 2, "{d}");
    // 時にマウスを重ねると、時と値。
    assert!(
        svg.contains(r#"<title data-i18n="usage.line.hit" data-i18n-options="{&#34;from&#34;:&#34;2026-11-22 10:00&#34;,&#34;to&#34;:&#34;11:00&#34;,&#34;value&#34;:{&#34;$t&#34;:&#34;common.tokens&#34;,&#34;values&#34;:{&#34;k&#34;:&#34;230&#34;,&#34;man&#34;:&#34;23&#34;}}}">2026-11-22 10:00〜11:00: 23 万</title>"#),
        "{svg}"
    );
    assert!(
        svg.contains(">2026-11-22 12:00〜13:00: 12 万</title>"),
        "{svg}"
    );
    // 20 万の点線（横）。
    assert_eq!(svg.matches("<line class=\"limit\"").count(), 1, "{svg}");
    // 圧縮（要約）の印は 1 つ。detail の前と後の文脈の長さを出す。
    assert_eq!(svg.matches("<line class=\"compact\"").count(), 1, "{svg}");
    assert!(
        svg.contains(r#"<title data-i18n="profile.compaction.tokens" data-i18n-options="{&#34;at&#34;:&#34;2026-11-22 12:00:00&#34;,&#34;name&#34;:{&#34;$t&#34;:&#34;events.compacted&#34;},&#34;post&#34;:{&#34;$t&#34;:&#34;common.tokens&#34;,&#34;values&#34;:{&#34;k&#34;:&#34;30&#34;,&#34;man&#34;:&#34;3&#34;}},&#34;pre&#34;:{&#34;$t&#34;:&#34;common.tokens&#34;,&#34;values&#34;:{&#34;k&#34;:&#34;180&#34;,&#34;man&#34;:&#34;18&#34;}}}">要約 · 2026-11-22 12:00:00（18 万 → 3 万）</title>"#),
        "{svg}"
    );
    assert!(
        !common::without_i18n_scripts(&body).contains("<script"),
        "{body}"
    );

    // detail に前と後の長さが無ければ、時刻だけ。
    let app = start_with_usage(se1_actor(), usage_answer_only_main()).await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    assert!(
        section(&body, "usage").contains(r#"<title data-i18n="common.eventAt" data-i18n-options="{&#34;at&#34;:&#34;2026-11-22 12:00:00&#34;,&#34;name&#34;:{&#34;$t&#34;:&#34;events.compacted&#34;}}">要約 · 2026-11-22 12:00:00</title>"#),
        "{body}"
    );
}

/// 03 時の本体の行だけ（要約の出来事の時刻を含む）。
fn usage_answer_only_main() -> serde_json::Value {
    usage_page(
        vec![usage_row(
            "u-4",
            SE1,
            None,
            "2026-11-22T03:20:00Z",
            json!({"turns": 1, "costMicroUsd": 1_000_000, "maxContext": 120_000, "buckets": usage_buckets(&[(4, 1, 1_000_000)])}),
        )],
        vec![usage_actor(SE1, "se.1", json!({}))],
        None,
    )
}

#[tokio::test]
async fn 使用量の節は_行が無ければ_そう書く() {
    let app = start_with_usage(se1_actor(), usage_page(vec![], vec![], None)).await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    common::check_i18n("profile.rs:704", &body);
    let usage = section(&body, "usage");
    assert!(
        usage.contains("<p class=\"empty\" data-i18n=\"profile.usage.empty\">この 31 日に、数の記録はありません。</p>"),
        "{usage}"
    );
    assert!(!usage.contains("<svg"), "{usage}");
}

#[tokio::test]
async fn 数を聞けなくても_頁は_200_で出し_使用量の節だけに訳を書く() {
    // castlist が古い（ListUsage を持たない。404）。
    let app = start().await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    let usage = section(&body, "usage");
    assert!(
        usage.contains("<p class=\"error\" data-i18n=\"profile.usage.error\" data-i18n-options=\"{&#34;why&#34;:{&#34;$t&#34;:&#34;upstream.returned&#34;,&#34;values&#34;:{&#34;status&#34;:404}}}\">数を読めません（404 が返りました）。</p>"),
        "{usage}"
    );
    // ほかの節は、今までどおり。
    assert_eq!(section(&body, "events").matches("<tr>").count(), 4);

    // 数の口だけが 503。
    let app = App::start_with(
        common::castlist::routes(se1_actor(), se1_lineage()).route(
            "/api/usage",
            route_get(|| async {
                common::castlist::error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "ServiceUnavailable",
                    "秘密の訳",
                )
            }),
        ),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get(&format!("/actors/{SE1}"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        section(&body, "usage").contains("数を読めません（503 が返りました）"),
        "{body}"
    );
    assert!(!body.contains("秘密の訳"), "{body}");
    assert!(body.contains("<h1>se.1</h1>"), "{body}");
}
