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
    assert!(body.contains("<title>se.1 ・ callboard</title>"), "{body}");
    // AI には種類の印を付けない。
    assert!(body.contains("<h1>se.1</h1>"), "{body}");
    assert!(body.contains(&format!("<dt>id</dt><dd><code>{SE1}</code></dd>")));
    assert!(body.contains("<dt>発行元（issuer）</dt><dd><code>tpm</code></dd>"));
    // 時刻は castlist が受けた時刻を、日本の時刻で（ISO の字は UTC のミリ秒まで）。
    assert!(body.contains(&format!(
        "<dt>登録した時刻</dt><dd>{}</dd>",
        time("2026-11-22T01:00:00.500Z", "2026-11-22 10:00:00")
    )));

    let state = section(&body, "state");
    assert!(
        state.contains("<span class=\"state off\">停止</span>"),
        "{state}"
    );
    assert!(!state.contains("推し"), "{state}");
    // 稼働を替えた最新の出来事（日本語の名。id は title に）。
    assert!(
        state.contains("<span class=\"event\" title=\"cleared\">交代</span>"),
        "{state}"
    );
    assert!(
        state.contains("<code>claude-sonnet-5</code> · <code>high</code>"),
        "{state}"
    );
    assert!(state.contains(&format!(
        "（実際・{}）",
        time("2026-11-22T02:30:00.000Z", "2026-11-22 11:30:00")
    )));
    assert!(state.contains(&format!(
        "<dt>最初の登場</dt><dd>{}</dd>",
        time("2026-11-22T01:00:01.000Z", "2026-11-22 10:00:01")
    )));
    assert!(state.contains(&format!(
        "<dt>最後の退場</dt><dd>{}</dd>",
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
        "表示した時刻: {}",
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
    let pm = format!("<a href=\"/actors/{PM}\">pm.1</a> <span class=\"muted\">人・起動した</span>");
    let tpm = format!("<a href=\"/actors/{TPM}\">tpm.1</a> <span class=\"muted\">起動した</span>");
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
        continued.contains("<dt>引き継ぎ元</dt><dd><span class=\"none\">—</span></dd>"),
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
    let lineage = json!({
        "asOf": "2026-11-22T06:00:00Z", "actorId": SE1,
        "nodes": [common::castlist::row(SE1, "pm.1", "human")], "edges": [],
    });
    let app = start_with(actor, lineage).await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    assert!(
        body.contains("<h1>pm.1 <span class=\"kind\">人</span></h1>"),
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
        state.contains("<dt>最初の登場</dt><dd><span class=\"none\">—</span></dd>"),
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
    assert!(rows[0].contains("<td>指定</td>") && !rows[0].contains("class=\"mismatch\""));
    // 指定は opus、実際は sonnet。
    assert!(
        rows[1].contains("<td>実際</td>") && rows[1].contains("class=\"mismatch\""),
        "{}",
        rows[1]
    );
    // 指定し直した後の実際は、合っている。
    assert!(rows[2].contains("<td>指定</td>"));
    assert!(
        rows[3].contains("<td>実際</td>") && !rows[3].contains("class=\"mismatch\""),
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
    assert!(rows[0].contains("<span class=\"event\" title=\"entered\">登場</span>"));
    assert!(!rows[0].contains("<code"), "{}", rows[0]);
    // reason と source は欄に分け、残りの detail は鍵の順の JSON の字。
    assert!(rows[1].contains("<span class=\"event\" title=\"compacted\">要約</span>"));
    assert!(rows[1].contains("<code>context_full</code>"), "{}", rows[1]);
    assert!(rows[1].contains("<code>auto</code>"), "{}", rows[1]);
    assert!(
        rows[1].contains("<code class=\"muted\">{&#34;ok&#34;:true,&#34;tokens&#34;:1200}</code>"),
        "{}",
        rows[1]
    );
    // 送り手の時刻は別の欄。ずれた行に印。
    assert!(rows[2].contains("<span class=\"event\" title=\"cleared\">交代</span>"));
    assert!(rows[2].contains(&time("2026-11-22T04:00:00.000Z", "2026-11-22 13:00:00")));
    assert!(rows[2].contains(&time("2026-11-22T05:00:00.000Z", "2026-11-22 14:00:00")));
    assert!(rows[2].contains("class=\"skew\"") && rows[2].contains("時刻ずれ"));
    assert!(!rows[1].contains("class=\"skew\""));
}

#[tokio::test]
async fn 時刻ずれの印は_登録_出来事_model_と_effort_のどれか_1_つがずれていれば付く() {
    // 見本の se.1 から、時刻ずれの行を無くした物。
    let clean = || {
        let mut actor = se1_actor();
        actor["events"][2]["clockSkew"] = json!(false);
        actor
    };
    let skewed = |actor: serde_json::Value| async move {
        let app = start_with(actor, se1_lineage()).await;
        let body = app.html(&format!("/actors/{SE1}")).await;
        section(&body, "state").contains("class=\"skew\"")
    };
    assert!(!skewed(clean()).await);
    // 登録だけ。
    let mut actor = clean();
    actor["clockSkew"] = json!(true);
    assert!(skewed(actor).await);
    // 出来事だけ。
    let mut actor = clean();
    actor["events"][0]["clockSkew"] = json!(true);
    assert!(skewed(actor).await);
    // model と effort だけ。
    let mut actor = clean();
    actor["settings"][3]["clockSkew"] = json!(true);
    assert!(skewed(actor).await);
}

#[tokio::test]
async fn 推しの幕引きで終わった者は_停止_推しと書く() {
    let mut actor = se1_actor();
    actor["state"] = json!({"presence": "closed", "present": false, "inferred": true});
    let mut lineage = se1_lineage();
    lineage["nodes"][2]["inferred"] = json!(true);
    let app = start_with(actor, lineage).await;
    let body = app.html(&format!("/actors/{SE1}")).await;
    let state = section(&body, "state");
    assert!(
        state.contains("<span class=\"state off\">停止（推し）</span>"),
        "{state}"
    );
    assert!(
        state.contains("<span class=\"event\" title=\"closed\">幕引き</span>"),
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
    assert!(section(&body, "state").contains("<span class=\"state on\">稼働中</span>"));
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
    for raw in ["<script", "<b>", "<i>", "<u>", "<img"] {
        assert!(!body.contains(raw), "{raw}: {body}");
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
    for inline in ["<script", "<style", "style="] {
        assert!(!body.contains(inline), "{inline}");
    }
    assert!(headers.get(header::CONTENT_SECURITY_POLICY).is_some());
    assert!(body.contains("<a class=\"home\" href=\"/\">callboard</a>"));
    assert!(body.contains("<nav"), "{body}");
}

#[tokio::test]
async fn castlist_には_host_127_0_0_1_で_2_つを聞き_browser_の_header_を渡さない() {
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
    assert_eq!(
        uris,
        [
            format!("/api/actors/{SE1}"),
            format!("/api/actors/{SE1}/lineage")
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
    assert_eq!(app.castlist.seen().len(), 2);

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
