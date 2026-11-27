//! 費え（`GET /usage/`）。castlist の API（ListUsage。偽の castlist）の行を、全体・役割ごと・課題ごとにまとめて出す所を見る。
//! まとめる計算の細かい決まり（中央・上から 1 割・20 万を越えた回）は、単体の試験（`src/ui/usage_summary.rs`）が見る。
//! ここは、引数を API にどう渡すか（時の境に揃えた期間・頁をまたいで引くこと）・答えをどう出すか・誤りの画面を見る。
mod common;

use axum::http::{StatusCode, header};
use common::castlist::{
    PM, SE1, SE2, TPM, usage_actor, usage_buckets, usage_page, usage_routes, usage_row,
};
use common::{App, get};
use serde_json::{Value, json};

/// 見本のアクター。se.1 は課題のラベルが 2 つ（`tpm.subject` を先に見る）、se.2 は `tpm.key` だけ、tpm.1 は役割だけ、pm.1 は無し。
fn actors() -> Vec<Value> {
    vec![
        usage_actor(
            SE1,
            "se.1",
            json!({"tpm.role": "software-engineer", "tpm.subject": "t-j4pp", "tpm.key": "t-old"}),
        ),
        usage_actor(
            SE2,
            "se.2",
            json!({"tpm.role": "software-engineer", "tpm.key": "t-23zs"}),
        ),
        usage_actor(
            TPM,
            "tpm.1",
            json!({"tpm.role": "technical-program-manager"}),
        ),
        usage_actor(PM, "pm.1", json!({})),
    ]
}

fn counts(turns: i64, cost: i64, max_context: i64, buckets: &[(i32, i64, i64)]) -> Value {
    json!({"turns": turns, "costMicroUsd": cost, "maxContext": max_context, "buckets": usage_buckets(buckets)})
}

/// 見本の行。額の計は $7.26。
/// - se.1 の本体: 同じ時・同じ種類の行が 2 つ（足す。いちばん長い文脈は大きい方の 23 万）。
/// - se.1 の手伝い（Explore）: 4 回・$0.25。
/// - se.2 の本体・tpm.1 の本体（97.5 万〜の段に 1 回）・pm.1 の本体。
fn rows() -> Vec<Value> {
    let hour = "2026-11-22T01:40:00Z";
    vec![
        usage_row(
            "u-1",
            SE1,
            None,
            hour,
            counts(3, 1_500_000, 61_000, &[(1, 1, 500_000), (2, 2, 1_000_000)]),
        ),
        usage_row(
            "u-2",
            SE1,
            None,
            hour,
            counts(2, 500_000, 230_000, &[(9, 2, 500_000)]),
        ),
        usage_row(
            "u-3",
            SE1,
            Some("Explore"),
            hour,
            counts(4, 250_000, 30_000, &[(1, 4, 250_000)]),
        ),
        usage_row(
            "u-4",
            SE2,
            None,
            "2026-11-22T02:10:00Z",
            counts(5, 2_000_000, 120_000, &[(0, 1, 100_000), (4, 4, 1_900_000)]),
        ),
        usage_row(
            "u-5",
            TPM,
            None,
            hour,
            counts(1, 3_000_000, 990_000, &[(39, 1, 3_000_000)]),
        ),
        usage_row(
            "u-6",
            PM,
            None,
            hour,
            counts(1, 10_000, 10_000, &[(0, 1, 10_000)]),
        ),
    ]
}

async fn start(pages: Vec<Value>) -> App {
    App::start_with(
        usage_routes(pages),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await
}

async fn sample() -> App {
    start(vec![usage_page(rows(), actors(), None)]).await
}

/// 期間を決めた見本の URL（日本時間の 10:30〜12:10）。
const WINDOW: &str = "/usage/?from=2026-11-22T10:30&to=2026-11-22T12:10";

/// castlist に送った問い（`/api/usage?…` の引数。名と値の並び。% の包みは戻す）を、問いごとに。
fn asked(app: &App) -> Vec<Vec<(String, String)>> {
    app.castlist
        .seen()
        .iter()
        .map(|seen| {
            assert_eq!(seen.method, "GET");
            assert_eq!(seen.header("host"), Some("127.0.0.1"));
            let (path, query) = seen.uri.split_once('?').unwrap_or((&seen.uri, ""));
            assert_eq!(path, "/api/usage");
            query
                .split('&')
                .filter(|p| !p.is_empty())
                .map(|p| {
                    let (name, value) = p.split_once('=').unwrap();
                    (name.to_owned(), unescape(value))
                })
                .collect()
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

fn all<'a>(pairs: &'a [(String, String)], name: &str) -> Vec<&'a str> {
    pairs
        .iter()
        .filter(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
        .collect()
}

/// 画面の節（`<section id="…">` の中身）。
fn section<'a>(body: &'a str, id: &str) -> &'a str {
    let start = body
        .find(&format!("<section id=\"{id}\">"))
        .unwrap_or_else(|| panic!("{id} の節が無い: {body}"));
    &body[start..start + body[start..].find("</section>").unwrap()]
}

/// 表の、名（役割・課題）の行の、数の欄（`<td class="num">` の中身を、左から）。
fn cells(section: &str, name: &str) -> Vec<String> {
    let at = section
        .find(&format!(">{name}</span></td>"))
        .unwrap_or_else(|| panic!("{name} の行が無い: {section}"));
    let start = section[..at].rfind("<tr>").unwrap();
    let row = &section[start..start + section[start..].find("</tr>").unwrap()];
    row.split("<td class=\"num\">")
        .skip(1)
        .map(|cell| cell[..cell.find("</td>").unwrap()].to_owned())
        .collect()
}

/// 表の行の名（上から）。
fn names(section: &str) -> Vec<String> {
    section
        .split("<tr><td><span class=\"")
        .skip(1)
        .map(|row| {
            let start = row.find('>').unwrap() + 1;
            row[start..start + row[start..].find("</span>").unwrap()].to_owned()
        })
        .collect()
}

/// 全体の数（`<dt>名</dt><dd>値</dd>` の値）。
fn fact<'a>(body: &'a str, name: &str) -> &'a str {
    let total = section(body, "total");
    let at = total
        .find(&format!("<dt>{name}</dt><dd>"))
        .unwrap_or_else(|| panic!("{name} が無い: {total}"));
    let start = at + format!("<dt>{name}</dt><dd>").len();
    &total[start..start + total[start..].find("</dd>").unwrap()]
}

#[tokio::test]
async fn 費えは_全体の数を出し_本体と手伝いを分ける() {
    let app = sample().await;
    let body = app.html(WINDOW).await;
    assert!(body.contains("<title>費え ・ callboard</title>"), "{body}");
    assert_eq!(fact(&body, "席（アクター）"), "4");
    // 回の数は、同じ時の 2 行（3 回と 2 回）も足す: 3 + 2 + 4 + 5 + 1 + 1。
    assert_eq!(fact(&body, "回"), "16");
    assert_eq!(fact(&body, "額"), "$7.26");
    // 手伝いの回は 4 回・$0.25（全体の 3.4%）。
    assert_eq!(
        fact(&body, "手伝い（subagent）の回"),
        "4（額 $0.25。全体の額の 3.4%）"
    );
    assert!(
        body.contains("表示した時刻: <time datetime=\"2026-11-22T06:00:00.000Z\""),
        "{body}"
    );
}

#[tokio::test]
async fn 役割ごとの表は_本体の回の数と額と_段から出した文脈の長さを_額の大きい順に出す() {
    let app = sample().await;
    let body = app.html(WINDOW).await;
    let roles = section(&body, "roles");
    assert_eq!(
        names(roles),
        ["software-engineer", "technical-program-manager", "（無し）"]
    );
    // 席・回（本体）・額（本体）・全体の額に対する比・中央・上から 1 割・いちばん長い文脈・20 万を越えた回・その額・手伝いの額。
    // se.1 の同じ時の 2 行を足す（回は 3 + 2 + 5 = 10、額は $4.00）。いちばん長い文脈は大きい方（23 万）。
    // 段: 0 に 1・1 に 1・2 に 2・4 に 4・9 に 2。5 番目は段 4（10 万〜）、9 番目は段 9（22.5 万〜）。手伝いの段は混ぜない。
    assert_eq!(
        cells(roles, "software-engineer"),
        [
            "2",
            "10",
            "$4.00",
            "55.1%",
            "10 万〜",
            "22.5 万〜",
            "23 万",
            "20.0%",
            "12.5%",
            "$0.25"
        ]
    );
    assert_eq!(
        cells(roles, "technical-program-manager"),
        [
            "1",
            "1",
            "$3.00",
            "41.3%",
            "97.5 万〜",
            "97.5 万〜",
            "99 万",
            "100.0%",
            "100.0%",
            "$0.00"
        ]
    );
    assert_eq!(
        cells(roles, "（無し）"),
        [
            "1", "1", "$0.01", "0.1%", "0 万〜", "0 万〜", "1 万", "0.0%", "0.0%", "$0.00"
        ]
    );
    assert!(
        roles.contains("<span class=\"chip role\">software-engineer</span>"),
        "{roles}"
    );
    assert!(
        roles.contains("<span class=\"none\">（無し）</span>"),
        "{roles}"
    );
}

#[tokio::test]
async fn 課題ごとの表は_tpm_subject_を先に見て_本体と手伝いの計を_額の大きい順に出す() {
    let app = sample().await;
    let body = app.html(WINDOW).await;
    let subjects = section(&body, "subjects");
    assert_eq!(names(subjects), ["（無し）", "t-j4pp", "t-23zs"]);
    // 席・回（本体と手伝いの計）・額（計）・いちばん長い文脈（本体）。
    assert_eq!(cells(subjects, "t-j4pp"), ["1", "9", "$2.25", "23 万"]);
    assert_eq!(cells(subjects, "t-23zs"), ["1", "5", "$2.00", "12 万"]);
    assert_eq!(cells(subjects, "（無し）"), ["2", "2", "$3.01", "99 万"]);
    // `tpm.subject` が在れば、`tpm.key` は見ない。
    assert!(!body.contains("t-old"), "{body}");
    assert!(
        subjects.contains("<span class=\"chip\">t-j4pp</span>"),
        "{subjects}"
    );
}

#[tokio::test]
async fn 手伝いの回だけの役割は_文脈の長さの欄を出さない() {
    let app = start(vec![usage_page(
        vec![usage_row(
            "u-1",
            SE1,
            Some("Explore"),
            "2026-11-22T01:40:00Z",
            counts(4, 250_000, 30_000, &[(1, 4, 250_000)]),
        )],
        actors(),
        None,
    )])
    .await;
    let body = app.html(WINDOW).await;
    let none = "<span class=\"none\">—</span>";
    assert_eq!(
        cells(section(&body, "roles"), "software-engineer"),
        [
            "1", "0", "$0.00", "0.0%", none, none, none, none, none, "$0.25"
        ]
    );
    assert_eq!(
        cells(section(&body, "subjects"), "t-j4pp"),
        ["1", "4", "$0.25", none]
    );
}

#[tokio::test]
async fn 期間は時の境に揃えて_castlist_に送り_送った期間を日本時間で出す() {
    let app = sample().await;
    let body = app
        .html("/usage/?from=2026-11-22T10:30&to=2026-11-22T03:10:00Z&label=tpm.role:software-engineer&label=project:x&empty=")
        .await;
    let asked = asked(&app);
    assert_eq!(asked.len(), 1, "{asked:?}");
    // 日本時間の 10:30 は時の初めに切り下げ、12:10 は時の境に切り上げる（UTC の字で送る）。
    assert_eq!(all(&asked[0], "from"), ["2026-11-22T01:00:00Z"]);
    assert_eq!(all(&asked[0], "to"), ["2026-11-22T04:00:00Z"]);
    assert_eq!(
        all(&asked[0], "label"),
        ["tpm.role:software-engineer", "project:x"]
    );
    assert_eq!(all(&asked[0], "maxResults"), ["1000"]);
    assert!(all(&asked[0], "nextToken").is_empty());
    assert!(all(&asked[0], "actorId").is_empty());
    // 送った期間（揃えた後）を、日本時間で出す。
    let at = body.find("<p class=\"note\" id=\"window\">").expect("期間");
    let window = &body[at..at + body[at..].find("</p>").unwrap()];
    assert!(
        window.contains("<time datetime=\"2026-11-22T01:00:00.000Z\" title=\"2026-11-22T01:00:00.000Z\">2026-11-22 10:00:00</time> から"),
        "{window}"
    );
    assert!(
        window.contains("<time datetime=\"2026-11-22T04:00:00.000Z\" title=\"2026-11-22T04:00:00.000Z\">2026-11-22 13:00:00</time> まで"),
        "{window}"
    );
    assert!(window.contains("時の境に揃えて"), "{window}");
    // form に今の値（期間は、入れたとおりの日本時間の字）。ラベルは付けた数と、空の 1 つ。
    assert!(body.contains("<form class=\"filters\" method=\"get\" action=\"/usage/\">"));
    assert!(body.contains("name=\"from\" value=\"2026-11-22T10:30\""));
    assert!(body.contains("name=\"to\" value=\"2026-11-22T12:10\""));
    assert_eq!(
        body.matches("<label>ラベル <input name=\"label\"").count(),
        3
    );
    assert!(
        body.contains("name=\"label\" value=\"tpm.role:software-engineer\""),
        "{body}"
    );
    assert!(body.contains("<a href=\"/usage/\">条件を消す</a>"));
    // 期間をすぐ選ぶ link は、ラベルを残して、この画面を指す。
    let at = body.find(">今日</a>").unwrap();
    let start = body[..at].rfind("href=\"").unwrap() + 6;
    let href = body[start..at - 1].replace("&#38;", "&");
    assert!(
        href.starts_with("/usage/?label=tpm.role:software-engineer&label=project:x&from=20")
            && href.contains("&to=20"),
        "{href}"
    );

    // 時の境ちょうどの期間は、そのまま送る。
    let app = sample().await;
    app.html("/usage/?from=2026-11-22T10:00&to=2026-11-22T12:00")
        .await;
    let asked = self::asked(&app);
    assert_eq!(all(&asked[0], "from"), ["2026-11-22T01:00:00Z"]);
    assert_eq!(all(&asked[0], "to"), ["2026-11-22T03:00:00Z"]);
}

#[tokio::test]
async fn 期間が無ければ_直近_7_日を_時の境に揃えて聞き_そう書く() {
    let app = sample().await;
    let body = app.html("/usage/").await;
    let asked = asked(&app);
    assert_eq!(asked.len(), 1, "{asked:?}");
    let time = |name: &str| {
        let value = all(&asked[0], name);
        assert_eq!(value.len(), 1, "{name}: {asked:?}");
        time::OffsetDateTime::parse(value[0], &time::format_description::well_known::Rfc3339)
            .unwrap()
    };
    let (from, to) = (time("from"), time("to"));
    // 終わりは、今を時の境に切り上げた時刻。始めは、その 7 日前。
    let now = time::OffsetDateTime::now_utc();
    assert!(
        to >= now - time::Duration::seconds(5) && to - now <= time::Duration::HOUR,
        "{to}"
    );
    assert_eq!((to.minute(), to.second(), to.nanosecond()), (0, 0, 0));
    assert_eq!(to - from, time::Duration::days(7));
    assert!(body.contains("期間が無いので、直近 7 日"), "{body}");
    assert!(body.contains("name=\"from\" value=\"\""), "{body}");

    // 片方だけなら: 始めだけなら今まで、終わりだけなら、その 7 日前から。
    let app = sample().await;
    app.html("/usage/?to=2026-11-22T12:10").await;
    let asked = self::asked(&app);
    assert_eq!(all(&asked[0], "from"), ["2026-11-15T03:00:00Z"]);
    assert_eq!(all(&asked[0], "to"), ["2026-11-22T04:00:00Z"]);
}

#[tokio::test]
async fn 行は_次の頁の印で_みな引き_アクターは頁をまたいで_1_人に数える() {
    let all_rows = rows();
    let app = start(vec![
        usage_page(all_rows[..2].to_vec(), actors()[..1].to_vec(), Some("p1")),
        // 2 頁目にも se.1 の行が在り、アクターの並びにも、もう 1 度出る。
        usage_page(all_rows[2..4].to_vec(), actors()[..2].to_vec(), Some("p2")),
        usage_page(all_rows[4..].to_vec(), actors()[2..].to_vec(), None),
    ])
    .await;
    let body = app.html(WINDOW).await;
    let asked = asked(&app);
    assert_eq!(asked.len(), 3, "{asked:?}");
    assert!(all(&asked[0], "nextToken").is_empty());
    assert_eq!(all(&asked[1], "nextToken"), ["p1"]);
    assert_eq!(all(&asked[2], "nextToken"), ["p2"]);
    // どの頁も、同じ期間と数で聞く。
    for page in &asked {
        assert_eq!(all(page, "from"), ["2026-11-22T01:00:00Z"]);
        assert_eq!(all(page, "to"), ["2026-11-22T04:00:00Z"]);
        assert_eq!(all(page, "maxResults"), ["1000"]);
    }
    // 1 頁で受けた時と同じ数。
    assert_eq!(fact(&body, "席（アクター）"), "4");
    assert_eq!(fact(&body, "回"), "16");
    assert_eq!(fact(&body, "額"), "$7.26");
    assert_eq!(
        cells(section(&body, "roles"), "software-engineer")[..3],
        ["2", "10", "$4.00"]
    );
    assert_eq!(
        cells(section(&body, "subjects"), "t-j4pp"),
        ["1", "9", "$2.25", "23 万"]
    );
    assert!(!body.contains("多すぎて"), "{body}");
}

#[tokio::test]
async fn 頁が多すぎれば_50_頁で止め_誤りにせず_期間を狭めるように書く() {
    // どの問いにも、次の頁の印を付けて答える castlist。
    let pages: Vec<Value> = (0..60)
        .map(|n| {
            usage_page(
                vec![usage_row(
                    &format!("u-{n}"),
                    SE1,
                    None,
                    "2026-11-22T01:40:00Z",
                    counts(1, 1_000_000, 10_000, &[(0, 1, 1_000_000)]),
                )],
                actors()[..1].to_vec(),
                Some(&format!("p{}", n + 1)),
            )
        })
        .collect();
    let app = start(pages).await;
    let body = app.html(WINDOW).await;
    assert_eq!(app.castlist.seen().len(), 50);
    assert!(
        body.contains("<p class=\"error\">記録が多すぎて、全部を読めませんでした"),
        "{body}"
    );
    assert!(body.contains("期間を狭めてください"), "{body}");
    // 読めた分（50 行）は出す。
    assert_eq!(fact(&body, "回"), "50");
    assert_eq!(fact(&body, "額"), "$50.00");
}

#[tokio::test]
async fn 行が無ければ_そう書き_表を出さない() {
    let app = start(vec![usage_page(vec![], vec![], None)]).await;
    let body = app.html(WINDOW).await;
    assert!(
        body.contains("<p class=\"empty\">この期間に、数の記録はありません。</p>"),
        "{body}"
    );
    assert!(!body.contains("<table"), "{body}");
    assert!(!body.contains("<section id=\"total\">"), "{body}");
}

#[tokio::test]
async fn 表の下に_額と_近い値と_遅れの注を書く() {
    let app = sample().await;
    let body = app.html(WINDOW).await;
    for note in [
        "額は、使った量を API の値段に直した目安で、払う額ではありません。",
        "中央と上から 1 割は、2.5 万刻みの段から出した近い値です",
        "稼働中の席の数は、長くて 1 時間ほど遅れて出ます。",
    ] {
        assert!(body.contains(note), "{note}: {body}");
    }
}

#[tokio::test]
async fn 費えの引数の誤りは_訳を書いた_400_の画面で_castlist_に聞かない() {
    let app = sample().await;
    for (uri, word) in [
        (
            "/usage/?nope=1",
            "知らない引数: nope（使えるのは label・from・to）",
        ),
        (
            "/usage/?from=2026-11-22T10:00&from=2026-11-22T11:00",
            "from は 1 つだけ付ける",
        ),
        ("/usage/?label=nocolon", "label は「鍵:値」の形（nocolon）"),
        (
            "/usage/?from=yesterday",
            "「期間（から）」の日時を読めません。日時の欄で選ぶか、2026-11-22T13:00 の形で入れてください。",
        ),
        (
            "/usage/?to=2026-13-01T00:00",
            "「期間（まで）」の日時を読めません。",
        ),
        // 31 日を越える期間（時の境に揃えた後の長さで見る）。
        (
            "/usage/?from=2026-10-01T00:00&to=2026-11-01T01:00",
            "期間は 31 日までです",
        ),
        (
            "/usage/?from=2026-10-01T00:30&to=2026-11-01T00:10",
            "期間は 31 日までです",
        ),
        // 終わりが始めより前。
        (
            "/usage/?from=2026-11-22T12:00&to=2026-11-22T10:00",
            "「期間（まで）」は、「期間（から）」より後にしてください。",
        ),
        (
            "/usage/?from=2026-11-22T12:30&to=2026-11-22T12:10",
            "「期間（まで）」は、「期間（から）」より後にしてください。",
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

    // ちょうど 31 日は通る。
    let (status, _, body) = app
        .send(get("/usage/?from=2026-10-01T00:00&to=2026-11-01T00:00"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn 費えは_script_も_inline_の_style_も使わず_利用者の字を_html_で逃がす() {
    let mut actors = actors();
    actors[0]["name"] = json!("<b>se</b>.1");
    actors[0]["labels"]["tpm.role"] = json!("<i>role</i>");
    actors[0]["labels"]["tpm.subject"] = json!("<u>subject</u> & \"x\"");
    let app = start(vec![usage_page(rows(), actors, None)]).await;
    let (status, headers, body) = app
        .send(get(
            "/usage/?from=2026-11-22T10:30&to=2026-11-22T12:10&label=a:%3Cscript%3E",
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // CSP は、ほかの画面と同じ（変えない）。
    assert_eq!(
        headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
    );
    for raw in ["<script", "<style", "style=", "<b>", "<i>", "<u>"] {
        assert!(!body.contains(raw), "{raw}: {body}");
    }
    assert!(
        body.contains("<span class=\"chip role\">&#60;i&#62;role&#60;/i&#62;</span>"),
        "{body}"
    );
    assert!(
        body.contains("&#60;u&#62;subject&#60;/u&#62; &#38; &#34;x&#34;"),
        "{body}"
    );
    assert!(
        body.contains("name=\"label\" value=\"a:&#60;script&#62;\""),
        "{body}"
    );
}

#[tokio::test]
async fn 頭の帯に_費えの_link_が在る() {
    let app = sample().await;
    let (_, _, missing) = app.send(get("/no/such")).await;
    for page in [app.html("/").await, app.html(WINDOW).await, missing] {
        let nav = &page[page.find("<nav").unwrap()..page.find("</nav>").unwrap()];
        assert!(nav.contains("<a href=\"/usage/\">費え</a>"), "{nav}");
    }
}

#[tokio::test]
async fn castlist_に聞けなければ_503_の画面で_訳を書き_聞く先を出さない() {
    let down = common::nowhere().await;
    let pagetray = common::Fake::start(common::pagetray_routes()).await;
    let router = callboard::app(&common::settings(&down, &pagetray.url()));
    let (status, _, body) = common::send(&router, get("/usage/")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body.contains("<h1>503</h1>") && body.contains("castlist に聞けません（つながりません）"),
        "{body}"
    );
    assert!(!body.contains(&down), "{body}");

    // 2.1.0 より古い castlist（ListUsage を持たない。404）でも、503 の画面で訳を書く。
    let app = App::start().await;
    let (status, _, body) = app.send(get("/usage/")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body.contains("castlist に聞けません（404 が返りました）"),
        "{body}"
    );

    // castlist が引数を断った時は、400 の画面で castlist の訳を書く（偽の castlist は、知らない次の頁の印を断る）。
    let app = start(vec![usage_page(rows(), actors(), Some("zz"))]).await;
    let (status, _, body) = app.send(get(WINDOW)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.contains("castlist が引数を断りました: nextToken が読めない"),
        "{body}"
    );
    assert!(!body.contains("開き直して"), "{body}");
}
