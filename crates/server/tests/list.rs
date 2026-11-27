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

/// castlist に送った、一覧の問い（`/api/actors?…` の引数。名と値の並び。% の包みは戻す）。数の問い（`/api/usage`）は [`usage_asked`]。
fn asked(app: &App) -> Vec<(String, String)> {
    asked_at(app, "/api/actors").expect("一覧の問いが無い")
}

/// castlist に送った、数の問い（`/api/usage?…`）。聞かなければ無い。
fn usage_asked(app: &App) -> Option<Vec<(String, String)>> {
    asked_at(app, "/api/usage")
}

/// castlist に送った、`path` への問い（1 度だけのはず）の引数。
fn asked_at(app: &App, path: &str) -> Option<Vec<(String, String)>> {
    let seen: Vec<common::Seen> = app
        .castlist
        .seen()
        .into_iter()
        .filter(|s| s.uri.split('?').next() == Some(path))
        .collect();
    assert!(seen.len() <= 1, "{seen:?}");
    let seen = seen.first()?;
    assert_eq!(seen.method, "GET");
    assert_eq!(seen.header("host"), Some("127.0.0.1"));
    let (_, query) = seen.uri.split_once('?').unwrap_or((&seen.uri, ""));
    let pairs = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (name, value) = p.split_once('=').unwrap();
            (name.to_owned(), unescape(value))
        })
        .collect();
    Some(pairs)
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

// --- 行の額と、いちばん長い文脈（castlist の ListUsage） ---

use common::castlist::{SE2, usage_actor, usage_buckets, usage_page, usage_row, with_usage};

/// se.1 の数の行: 本体が同じ時に 2 行（$1.00 と $0.50。文脈は 6.1 万と 23 万）と、手伝い（$0.25。文脈 90 万は数えない）。
/// 一覧に居ない se.2 の行も在る（一覧には出ない）。pm.1 には行が無い。
fn usage() -> Value {
    let counts = |turns: i64, cost: i64, max_context: i64| json!({"turns": turns, "costMicroUsd": cost, "maxContext": max_context, "buckets": usage_buckets(&[(0, turns, cost)])});
    usage_page(
        vec![
            usage_row(
                "u-1",
                SE1,
                None,
                "2026-11-22T01:40:00Z",
                counts(3, 1_000_000, 61_000),
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
                SE2,
                None,
                "2026-11-22T02:10:00Z",
                counts(1, 9_000_000, 500_000),
            ),
        ],
        vec![
            usage_actor(SE1, "se.1", json!({})),
            usage_actor(SE2, "se.2", json!({})),
        ],
        None,
    )
}

/// 一覧の答えと、数の答えを返す偽の castlist。
async fn start_with_usage(answer: Value, usage: Value) -> App {
    App::start_with(
        with_usage(castlist(answer), vec![usage]),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await
}

/// 行の、数の欄（`<td class="num">` の中身を、左から）。
fn nums(tr: &str) -> Vec<&str> {
    tr.split("<td class=\"num\">")
        .skip(1)
        .map(|cell| &cell[..cell.find("</td>").unwrap()])
        .collect()
}

const NONE: &str = "<span class=\"none\">—</span>";

#[tokio::test]
async fn 一覧の行に_一覧の期間の_席の額と_いちばん長い文脈を出す() {
    let app = start_with_usage(rows(), usage()).await;
    let body = app
        .html("/actors/?label=project:x&active_from=2026-11-22T10:30&active_to=2026-11-22T12:10")
        .await;
    assert!(
        body.contains("<th class=\"num\">額</th><th class=\"num\">いちばん長い文脈</th>"),
        "{body}"
    );
    // se.1: 額は本体と手伝いの計（$1.75）、いちばん長い文脈は本体の回の大きい方（23 万。手伝いの 90 万は数えない）。
    assert_eq!(nums(tr(&body, SE1)), ["$1.75", "23 万"]);
    // 数の無い席は「—」。
    assert_eq!(nums(tr(&body, PM)), [NONE, NONE]);
    // 一覧に居ない se.2 の行は出さない。
    assert!(!body.contains("$9.00"), "{body}");
    // 期間は、一覧の期間を時の境に揃えた物（日本時間の 10:30〜12:10 → UTC の 01:00〜04:00）。ラベルも同じ物で絞る。
    let usage = usage_asked(&app).expect("数を聞いていない");
    assert_eq!(one(&usage, "from"), Some("2026-11-22T01:00:00Z"));
    assert_eq!(one(&usage, "to"), Some("2026-11-22T04:00:00Z"));
    assert_eq!(one(&usage, "label"), Some("project:x"));
    assert_eq!(one(&usage, "actorId"), None);
    // 表の下に、列の訳。
    assert!(
        body.contains("額は、額を数えた期間（時の境に揃えた"),
        "{body}"
    );
}

#[tokio::test]
async fn 一覧に期間が無ければ_数は直近_7_日を時の境に揃えて聞く() {
    let app = start_with_usage(rows(), usage()).await;
    let body = app.html("/actors/").await;
    assert_eq!(nums(tr(&body, SE1)), ["$1.75", "23 万"]);
    let usage = usage_asked(&app).expect("数を聞いていない");
    let at = |name: &str| {
        time::OffsetDateTime::parse(
            one(&usage, name).unwrap(),
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap()
    };
    let (from, to) = (at("from"), at("to"));
    let now = time::OffsetDateTime::now_utc();
    assert_eq!((from.minute(), from.second()), (0, 0));
    assert_eq!((to.minute(), to.second()), (0, 0));
    assert!(
        to >= now - time::Duration::seconds(5) && to - now <= time::Duration::HOUR,
        "{to}"
    );
    // 一覧の既定の窓（今から 7 日前から）を、時の初めに切り下げた所から。
    let span = to - from;
    assert!(
        span > time::Duration::days(7) && span <= time::Duration::days(7) + time::Duration::HOUR,
        "{span}"
    );
    assert_eq!(one(&usage, "label"), None);
}

#[tokio::test]
async fn 一覧の期間が_31_日を越えれば_数を聞かずに_列を空けて注を書く() {
    let app = start_with_usage(rows(), usage()).await;
    let body = app
        .html("/actors/?active_from=2026-10-01T00:00&active_to=2026-11-22T00:00")
        .await;
    assert!(usage_asked(&app).is_none());
    assert_eq!(nums(tr(&body, SE1)), [NONE, NONE]);
    assert!(
        body.contains("一覧の期間が 31 日を越えるので、額といちばん長い文脈は出しません"),
        "{body}"
    );
}

#[tokio::test]
async fn 数を聞けなくても_一覧は_200_で出し_列を空けて訳を書く() {
    // castlist が古い（ListUsage を持たない。404）。
    let app = start(rows()).await;
    let (status, _, body) = app.send(get("/actors/?label=project:x")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(nums(tr(&body, SE1)), [NONE, NONE]);
    assert_eq!(nums(tr(&body, PM)), [NONE, NONE]);
    assert!(
        body.contains("額といちばん長い文脈を読めません（404 が返りました）"),
        "{body}"
    );
    assert!(usage_asked(&app).is_some());

    // 数の口が 503。
    let app = App::start_with(
        castlist(rows()).route(
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
    let (status, _, body) = app.send(get("/actors/?label=project:x")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("額といちばん長い文脈を読めません（503 が返りました）"),
        "{body}"
    );
    assert!(!body.contains("秘密の訳"), "{body}");
}

#[tokio::test]
async fn 数の口が遅くても_一覧は_長く待たずに出し_列を空けて訳を書く() {
    // 数の口だけが 3 秒待たせる castlist（一覧の側の期限は 1 秒。review-13 の #1）。
    let app = App::start_with(
        castlist(rows()).route(
            "/api/usage",
            route_get(|| async {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                common::json(StatusCode::OK, &usage().to_string())
            }),
        ),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let started = std::time::Instant::now();
    let (status, _, body) = app.send(get("/actors/?label=project:x")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(nums(tr(&body, SE1)), [NONE, NONE]);
    assert!(
        body.contains(
            "額といちばん長い文脈を、時間内に読めませんでした。一覧は、そのまま出しています。"
        ),
        "{body}"
    );
}

#[tokio::test]
async fn 額の注は_額を数えた期間と書く() {
    let app = start_with_usage(rows(), usage()).await;
    let body = app.html("/actors/?label=project:x").await;
    assert!(
        body.contains("額は、額を数えた期間（時の境に揃えた "),
        "{body}"
    );
}
