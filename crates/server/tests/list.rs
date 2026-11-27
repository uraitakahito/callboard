//! アクターの一覧（`GET /actors/`）。castlist の画面の試験（castlist の tests/ui.rs の「一覧」）を、castlist の API
//! （ListActors。偽の castlist）に聞く形に移した物。絞りの中身（どのアクターが当たるか）は castlist の試験が見る。
//! ここは、引数を API にどう渡すか・答えをどう出すか・誤りの画面を見る。
mod common;

use axum::Router;
use axum::http::{StatusCode, header};
use axum::routing::get as route_get;
use common::castlist::{PM, SE1, row};
use common::{App, get};
use serde_json::{Value, json};

/// 一覧の答え（ListActors）を返す偽の castlist。
fn castlist(answer: Value) -> Router {
    common::castlist_routes().route(
        "/api/actors",
        route_get(move || {
            let answer = answer.clone();
            async move { common::json(StatusCode::OK, &answer.to_string()) }
        }),
    )
}

/// 見本の 2 行: 人の pm.1（登場の記録だけ）と、退いた se.1（時刻ずれあり）。
fn rows() -> Value {
    let mut pm = row(PM, "pm.1", "human");
    pm["firstEnteredAt"] = json!("2026-11-22T00:30:00Z");
    let mut se = row(SE1, "se.1", "ai");
    se["labels"] = json!({"tpm.key": "t-23zs", "tpm.role": "software-engineer", "project": "x"});
    se["present"] = json!(false);
    se["inferred"] = json!(true);
    se["presence"] = json!("closed");
    se["skewed"] = json!(true);
    se["model"] = json!("claude-opus-5-5");
    se["effort"] = json!("high");
    se["firstEnteredAt"] = json!("2026-11-22T01:00:01Z");
    se["lastExitedAt"] = json!("2026-11-22T04:00:00.5Z");
    json!({"asOf": "2026-11-22T06:00:00Z", "actors": [pm, se]})
}

async fn start(answer: Value) -> App {
    App::start_with(
        castlist(answer),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await
}

/// castlist に送った問い（`/api/actors?…` の引数。名と値の並び。% の包みは戻す）。
fn asked(app: &App) -> Vec<(String, String)> {
    let seen = app.castlist.seen();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0].method, "GET");
    assert_eq!(seen[0].header("host"), Some("127.0.0.1"));
    let (path, query) = seen[0].uri.split_once('?').unwrap_or((&seen[0].uri, ""));
    assert_eq!(path, "/api/actors");
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (name, value) = p.split_once('=').unwrap();
            (name.to_owned(), unescape(value))
        })
        .collect()
}

fn unescape(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

fn one<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

/// id のアクターの行（`<tr>` の中身）。
fn tr<'a>(body: &'a str, id: &str) -> &'a str {
    let at = body
        .find(&format!("href=\"/actors/{id}\""))
        .unwrap_or_else(|| panic!("{id} の行が無い"));
    let start = body[..at].rfind("<tr>").unwrap();
    &body[start..start + body[start..].find("</tr>").unwrap()]
}

fn unescape_html(s: &str) -> String {
    s.replace("&#38;", "&")
        .replace("&#34;", "\"")
        .replace("&#60;", "<")
        .replace("&#62;", ">")
}

#[tokio::test]
async fn 一覧は_条件が無ければ_この_7_日に稼働した者を新しい順に聞き_そう書く() {
    let app = start(rows()).await;
    let body = app.html("/actors/").await;
    assert!(
        body.contains("<title>アクターの一覧 ・ callboard</title>"),
        "{body}"
    );
    assert!(
        body.contains("条件が無いので、この 7 日に稼働したアクター"),
        "{body}"
    );
    let asked = asked(&app);
    assert_eq!(one(&asked, "order"), Some("desc"));
    assert_eq!(one(&asked, "maxResults"), Some("100"));
    // activeFrom は、今から 7 日前（UTC の字）。
    let from = one(&asked, "activeFrom").expect("activeFrom");
    let from =
        time::OffsetDateTime::parse(from, &time::format_description::well_known::Rfc3339).unwrap();
    let ago = time::OffsetDateTime::now_utc() - from;
    assert!(
        (ago - time::Duration::days(7)).abs() < time::Duration::minutes(1),
        "{ago}"
    );
    assert_eq!(asked.len(), 4, "{asked:?}");
    assert_eq!(one(&asked, "sort"), Some("entered"));
}

#[tokio::test]
async fn 一覧の行は_名_種類_状態_最後の出来事_課題の_key_と役割_model_と_effort_時刻_時刻ずれを出す()
 {
    let app = start(rows()).await;
    let body = app.html("/actors/?label=project:x").await;
    // 条件があれば、既定の窓の断りは出さない。
    assert!(!body.contains("条件が無いので"), "{body}");
    let pm = tr(&body, PM);
    assert!(
        pm.contains(&format!("<a class=\"name\" href=\"/actors/{PM}\">pm.1</a>")),
        "{pm}"
    );
    assert!(pm.contains("<td>人</td>"), "{pm}");
    assert!(pm.contains("<span class=\"state on\">稼働中</span> <span class=\"event\" title=\"entered\">登場</span>"), "{pm}");
    assert!(pm.contains("2026-11-22 09:30:00"), "{pm}");
    assert!(
        !pm.contains("class=\"chip") && !pm.contains("class=\"skew\""),
        "{pm}"
    );
    let se = tr(&body, SE1);
    // AI には種類の印を付けない。
    assert!(se.contains("<td></td>"), "{se}");
    assert!(se.contains("<span class=\"state off\">停止（推し）</span> <span class=\"event\" title=\"closed\">幕引き</span>"), "{se}");
    assert!(
        se.contains(
            "<span class=\"chip\">t-23zs</span> <span class=\"chip role\">software-engineer</span>"
        ),
        "{se}"
    );
    assert!(
        se.contains("<code>claude-opus-5-5</code> · <code>high</code>"),
        "{se}"
    );
    assert!(se.contains("<time datetime=\"2026-11-22T04:00:00.500Z\" title=\"2026-11-22T04:00:00.500Z\">2026-11-22 13:00:00</time>"), "{se}");
    assert!(
        se.contains("class=\"skew\"") && se.contains("時刻ずれ"),
        "{se}"
    );
    // 答えの順のまま。
    assert!(body.find(PM).unwrap() < body.find(SE1).unwrap());
    assert!(
        body.contains("表示した時刻: <time datetime=\"2026-11-22T06:00:00.000Z\""),
        "{body}"
    );
}

#[tokio::test]
async fn 一覧の絞りは_画面の引数を_castlist_の_api_の引数にして渡し_form_に今の値を出す() {
    let app = start(rows()).await;
    let body = app
        .html("/actors/?label=tpm.key:t-23zs&label=tpm.role:se&active=true&kind=human&model=claude-opus-5-5&effort=high&active_from=2026-11-22T13:00&active_to=2026-11-23T00:00:00Z&skewed=true&launched_by=x1&sort=exited&order=asc&limit=7&binding=claude.session:s%201&at=2026-11-22T00:00:00%2B09:00&empty=")
        .await;
    let asked = asked(&app);
    let labels: Vec<&str> = asked
        .iter()
        .filter(|(n, _)| n == "label")
        .map(|(_, v)| v.as_str())
        .collect();
    assert_eq!(labels, ["tpm.key:t-23zs", "tpm.role:se"]);
    for (name, value) in [
        ("active", "true"),
        ("kind", "human"),
        ("model", "claude-opus-5-5"),
        ("effort", "high"),
        // 日時の欄の値（日本時間）は、UTC の字にして渡す。
        ("activeFrom", "2026-11-22T04:00:00Z"),
        ("activeTo", "2026-11-23T00:00:00Z"),
        ("skewed", "true"),
        ("launchedBy", "x1"),
        ("sort", "exited"),
        ("order", "asc"),
        ("maxResults", "7"),
        ("binding", "claude.session:s 1"),
        ("at", "2026-11-21T15:00:00Z"),
    ] {
        assert_eq!(one(&asked, name), Some(value), "{name}: {asked:?}");
    }
    assert_eq!(one(&asked, "nextToken"), None);
    // form: ラベルは付けた数と、空の 1 つ。期間は日本時間の字。欄の無い引数は hidden で送り直す。
    assert_eq!(
        body.matches("<label>ラベル <input name=\"label\"").count(),
        3
    );
    assert!(
        body.contains("name=\"label\" value=\"tpm.role:se\""),
        "{body}"
    );
    assert!(body.contains("<option value=\"true\" selected>稼働中</option>"));
    assert!(body.contains("<option value=\"human\" selected>人</option>"));
    assert!(body.contains("name=\"active_from\" value=\"2026-11-22T13:00\""));
    assert!(body.contains("name=\"active_to\" value=\"2026-11-23T09:00\""));
    assert!(body.contains("name=\"skewed\" value=\"true\" checked"));
    for (name, value) in [
        ("launched_by", "x1"),
        ("sort", "exited"),
        ("order", "asc"),
        ("limit", "7"),
        ("binding", "claude.session:s 1"),
    ] {
        assert!(
            body.contains(&format!(
                "<input type=\"hidden\" name=\"{name}\" value=\"{value}\">"
            )),
            "{name}"
        );
    }
    assert!(body.contains("<form class=\"filters\" method=\"get\" action=\"/actors/\">"));
    assert!(body.contains("<a href=\"/actors/\">条件を消す</a>"));
}

#[tokio::test]
async fn 一覧の次のページの_link_は_条件をそのままに_答えの印を_after_に付ける() {
    let mut answer = rows();
    answer["nextToken"] = json!("6162");
    let app = start(answer).await;
    let body = app.html("/actors/?label=project:x&limit=2").await;
    let at = body.find("rel=\"next\" href=\"").expect("次のページ") + 17;
    let href = unescape_html(&body[at..at + body[at..].find('"').unwrap()]);
    assert_eq!(
        href,
        "/actors/?label=project:x&limit=2&order=desc&after=6162"
    );
    assert!(body.contains(">次のページ</a>"));

    // 既定の窓で出した時は、次の頁も同じ窓で続ける。
    let mut answer = rows();
    answer["nextToken"] = json!("6162");
    let app = start(answer).await;
    let body = app.html("/actors/").await;
    let at = body.find("rel=\"next\" href=\"").unwrap() + 17;
    let href = unescape_html(&body[at..at + body[at..].find('"').unwrap()]);
    assert!(
        href.starts_with("/actors/?order=desc&limit=100&active_from=20")
            && href.ends_with("Z&after=6162"),
        "{href}"
    );

    // 印が無ければ、link は出さない。
    let app = start(rows()).await;
    assert!(
        !app.html("/actors/?label=project:x")
            .await
            .contains("rel=\"next\"")
    );
}

#[tokio::test]
async fn 次のページの印は_そのまま_castlist_に渡す() {
    let app = start(rows()).await;
    // actors・entered・desc・並べる値・seq を改行で繋いだ 16 進（castlist の印の形）。
    let token: String = "actors\nentered\ndesc\n2026-11-22T01:00:01.000Z\n5"
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    app.html(&format!("/actors/?label=project:x&after={token}"))
        .await;
    let asked = asked(&app);
    assert_eq!(one(&asked, "nextToken"), Some(token.as_str()));
    assert_eq!(one(&asked, "order"), Some("desc"));
}

#[tokio::test]
async fn 一覧の引数の誤りは_引数の名を書いた_400_の画面で_castlist_に聞かない() {
    let app = start(rows()).await;
    let other: String = "settings\n5".bytes().map(|b| format!("{b:02x}")).collect();
    let asc: String = "actors\nentered\nasc\nx\n5"
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    for (uri, word) in [
        (
            "/actors/?nope=1".to_owned(),
            "知らない引数: nope（使えるのは label・binding・at・active_from・active_to・kind・model・effort・active・launched_by・skewed・sort・order・limit・after）",
        ),
        (
            "/actors/?kind=ai&kind=human".to_owned(),
            "kind は 1 つだけ付ける",
        ),
        (
            "/actors/?label=nocolon".to_owned(),
            "label は「鍵:値」の形（nocolon）",
        ),
        (
            "/actors/?binding=x".to_owned(),
            "binding は「種類:値」の形（x）",
        ),
        (
            "/actors/?at=2026-11-22T00:00:00Z".to_owned(),
            "at は binding と一緒に付ける（居た間で探すなら active_from・active_to）",
        ),
        (
            "/actors/?binding=a:b&at=きのう".to_owned(),
            "at は RFC 3339 の時刻（例 2026-11-22T13:00:00+09:00）",
        ),
        (
            "/actors/?kind=robot".to_owned(),
            "kind は ai・human のどれか",
        ),
        (
            "/actors/?model=Opus".to_owned(),
            "model は英小文字・数字・- . で 64 字まで",
        ),
        ("/actors/?active=yes".to_owned(), "active は true か false"),
        (
            "/actors/?skewed=false".to_owned(),
            "skewed は true だけ（絞らないなら付けない）",
        ),
        (
            "/actors/?sort=name".to_owned(),
            "sort は entered・exited のどれか",
        ),
        (
            "/actors/?order=up".to_owned(),
            "order は asc・desc のどれか",
        ),
        (
            "/actors/?limit=0".to_owned(),
            "limit は 1 から 1000 までの数",
        ),
        (
            "/actors/?limit=1001".to_owned(),
            "limit は 1 から 1000 までの数",
        ),
        (
            "/actors/?after=zz".to_owned(),
            "after が読めない（前の答えの next を、そのまま渡す）",
        ),
        (
            format!("/actors/?after={other}"),
            "after が読めない（actors の検索の next ではない）",
        ),
        (
            format!("/actors/?after={asc}"),
            "after は、同じ sort と order の答えの next を渡す",
        ),
        // 期間の欄の読めない値は、欄の見出しと直し方を、です・ます で。
        (
            "/actors/?active_from=yesterday".to_owned(),
            "「稼働した期間（から）」の日時を読めません。日時の欄で選ぶか、2026-11-22T13:00 の形で入れてください。",
        ),
        (
            "/actors/?active_to=2026-13-01T00:00".to_owned(),
            "「稼働した期間（まで）」の日時を読めません。",
        ),
    ] {
        let (status, headers, body) = app.send(get(&uri)).await;
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
async fn 条件に合うアクターがいなければ_そう書き_表を出さない() {
    let app = start(json!({"asOf": "2026-11-22T06:00:00Z", "actors": []})).await;
    let body = app.html("/actors/?label=a:b").await;
    assert!(
        body.contains("<p class=\"empty\">条件に合うアクターはいません。</p>"),
        "{body}"
    );
    assert!(!body.contains("<table"), "{body}");
}

#[tokio::test]
async fn 一覧から_タイムラインと相関図へ行け_期間をすぐ選ぶ_link_は一覧を指す() {
    let app = start(rows()).await;
    let body = app.html("/actors/?label=project:x").await;
    assert!(
        body.contains(
            "<a href=\"/graph/timeline\">タイムライン</a> ・ <a href=\"/graph/network\">相関図</a>"
        ),
        "{body}"
    );
    let at = body.find(">今日</a>").unwrap();
    let start = body[..at].rfind("href=\"").unwrap() + 6;
    let href = unescape_html(&body[start..at - 1]);
    assert!(
        href.starts_with("/actors/?label=project:x&active_from=20")
            && href.contains("&active_to=20"),
        "{href}"
    );
    for inline in ["<script", "<style", "style="] {
        assert!(!body.contains(inline), "{inline}");
    }
}

#[tokio::test]
async fn 一覧も利用者の字を_html_で逃がす() {
    let mut answer = rows();
    answer["actors"][1]["name"] = json!("<b>se</b>.1");
    answer["actors"][1]["labels"]["tpm.key"] = json!("<i>k</i>");
    let app = start(answer).await;
    let body = app.html("/actors/?label=a:%3Cscript%3E").await;
    for raw in ["<b>", "<i>", "<script>"] {
        assert!(!body.contains(raw), "{raw}");
    }
    assert!(body.contains("&#60;b&#62;se&#60;/b&#62;.1"), "{body}");
}

#[tokio::test]
async fn castlist_に聞けなければ_503_の画面で_訳を書き_聞く先を出さない() {
    let down = common::nowhere().await;
    let pagetray = common::Fake::start(common::pagetray_routes()).await;
    let router = callboard::app(&common::settings(&down, &pagetray.url()));
    let (status, _, body) = common::send(&router, get("/actors/")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body.contains("castlist に聞けません（つながりません）"),
        "{body}"
    );
    assert!(!body.contains(&down), "{body}");

    // castlist が引数を断った時（callboard の確かめを通った物なので、ふつうは起きない）は、400 の画面で castlist の訳を書く
    // （送り直しても直らないので、「待って開き直す」とは書かない）。
    let app = App::start_with(
        Router::new().route(
            "/api/actors",
            route_get(|| async {
                common::castlist::error(
                    StatusCode::BAD_REQUEST,
                    "ValidationException",
                    "label は 3 つまで",
                )
            }),
        ),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get("/actors/")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.contains("castlist が引数を断りました: label は 3 つまで"),
        "{body}"
    );
    assert!(!body.contains("開き直して"), "{body}");
}
