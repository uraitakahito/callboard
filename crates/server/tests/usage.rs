//! 使用量（`GET /usage/`）。castlist の API（ListUsage。偽の castlist）の行を、全体・役割ごと・課題ごとにまとめて出す所を見る。
//! まとめる計算の細かい決まり（中央・上から 1 割・20 万以上の回）は、単体の試験（`src/ui/usage_summary.rs`）が見る。
//! ここは、引数を API にどう渡すか（時の境に揃えた期間・頁をまたいで引くこと）・答えをどう出すか・誤りの画面を見る。
mod common;

use axum::http::{StatusCode, header};
use common::castlist::{
    PM, SE1, SE2, TPM, actor_row, usage_actor, usage_buckets, usage_page, usage_routes,
    usage_routes_slow, usage_row, with_actors, with_actors_down, with_actors_more,
    with_actors_slow,
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

/// 期間に稼働した席（ListActors の答え。絞りの候補の元）。役割は se 2 人・tpm 1 人、課題は t-j4pp に 2 人・t-23zs に 1 人。
/// 起こした回の id（`tpm.launch`）は候補に出ない。
fn active() -> Vec<Value> {
    vec![
        actor_row(
            SE1,
            "se.1",
            json!({"tpm.role": "software-engineer", "tpm.subject": "t-j4pp", "tpm.key": "t-j4pp", "tpm.launch": "0f8f5700-639d-411f-b170-b4346dfcebff"}),
        ),
        actor_row(
            SE2,
            "se.2",
            json!({"tpm.role": "software-engineer", "tpm.subject": "t-23zs", "tpm.key": "t-23zs"}),
        ),
        actor_row(
            TPM,
            "tpm.1",
            json!({"tpm.role": "tpm", "tpm.subject": "t-j4pp", "tpm.key": "t-j4pp.staff"}),
        ),
        actor_row(PM, "pm.1", json!({})),
    ]
}

async fn start(pages: Vec<Value>) -> App {
    App::start_with(
        with_actors(usage_routes(pages), active()),
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
    asked_at(app, "/api/usage")
}

/// castlist の `path` に送った問いの引数を、問いごとに（[`asked`] と同じ形）。
fn asked_at(app: &App, at: &str) -> Vec<Vec<(String, String)>> {
    app.castlist
        .seen()
        .iter()
        .filter(|seen| {
            seen.uri
                .split_once('?')
                .map_or(seen.uri.as_str(), |(p, _)| p)
                == at
        })
        .map(|seen| {
            assert_eq!(seen.method, "GET");
            assert_eq!(seen.header("host"), Some("127.0.0.1"));
            let (_, query) = seen.uri.split_once('?').unwrap_or((&seen.uri, ""));
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
        .map(|cell| common::unwrap_text(&cell[..cell.find("</td>").unwrap()]).to_owned())
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

/// 全体の数（`<dt>名</dt><dd>値</dd>` の値。`<dt>` には字の鍵の属性が付く）。
fn fact<'a>(body: &'a str, name: &str) -> &'a str {
    let total = section(body, "total");
    let at = total
        .find(&format!("\">{name}</dt><dd>"))
        .unwrap_or_else(|| panic!("{name} が無い: {total}"));
    let start = at + format!("\">{name}</dt><dd>").len();
    &total[start..start + total[start..].find("</dd>").unwrap()]
}

#[tokio::test]
async fn 使用量は_全体の数を出し_本体と手伝いを分ける() {
    let app = sample().await;
    let body = app.html(WINDOW).await;
    assert!(
        body.contains("<title data-i18n=\"usage.title\">使用量 ・ callboard</title>"),
        "{body}"
    );
    assert_eq!(fact(&body, "席（アクター）"), "4");
    // 回の数は、同じ時の 2 行（3 回と 2 回）も足す: 3 + 2 + 4 + 5 + 1 + 1。
    assert_eq!(fact(&body, "回"), "16");
    assert_eq!(fact(&body, "額"), "$7.26");
    // 手伝いの回は 4 回・$0.25（全体の 3.4%）。
    assert_eq!(
        fact(&body, "手伝い（subagent）の回"),
        r#"4<span data-i18n="usage.total.helperCostShare" data-i18n-options="{&#34;cost&#34;:&#34;$0.25&#34;,&#34;share&#34;:&#34;3.4%&#34;}">（額 $0.25。全体の額の 3.4%）</span>"#
    );
    assert!(
        body.contains("<span data-i18n=\"common.asOf\">表示した時刻:</span> <time datetime=\"2026-11-22T06:00:00.000Z\""),
        "{body}"
    );
}

#[tokio::test]
async fn 役割ごとの表は_本体の回の数と額と_段から出した文脈の長さを_額の大きい順に出す() {
    let app = sample().await;
    let body = app.html(WINDOW).await;
    common::check_i18n("usage.rs:223", &body);
    let roles = section(&body, "roles");
    assert_eq!(
        names(roles),
        ["software-engineer", "technical-program-manager", "（無し）"]
    );
    // 席・回（本体）・額（本体）・全体の額に対する比・中央・上から 1 割・いちばん長い文脈・20 万以上の回・その額・手伝いの額。
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
        roles.contains("<span class=\"none\" data-i18n=\"usage.noName\">（無し）</span>"),
        "{roles}"
    );
    // 20 万の段（8 段目）から数えるので、見出しは「以上」（review-9 の #4）。
    assert!(
        roles.contains(
            "<th class=\"num\" data-i18n=\"usage.col.overTurns\">20 万以上の回</th><th class=\"num\" data-i18n=\"usage.col.overCost\">20 万以上の回の額</th>"
        ),
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
        window.contains("<time datetime=\"2026-11-22T01:00:00.000Z\" title=\"2026-11-22T01:00:00.000Z\">2026-11-22 10:00:00</time><span data-i18n=\"list.costs.middle\"> から"),
        "{window}"
    );
    assert!(
        window.contains("<time datetime=\"2026-11-22T04:00:00.000Z\" title=\"2026-11-22T04:00:00.000Z\">2026-11-22 13:00:00</time><span data-i18n=\"usage.window.after\"> まで"),
        "{window}"
    );
    assert!(window.contains("時の境に揃えて"), "{window}");
    // form に今の値（期間は、入れたとおりの日本時間の字）。役割は一覧で選び、ほかのラベルは付けた数と、空の 1 つ。
    assert!(body.contains("<form class=\"filters\" method=\"get\" action=\"/usage/\">"));
    assert!(body.contains("name=\"from\" value=\"2026-11-22T10:30\""));
    assert!(body.contains("name=\"to\" value=\"2026-11-22T12:10\""));
    assert_eq!(
        selected(&select(&body, "role")),
        Some("tpm.role:software-engineer".to_owned())
    );
    assert_eq!(fields(&body), ["project:x", ""]);
    assert!(body.contains("<a href=\"/usage/\" data-i18n=\"filters.clear\">条件を消す</a>"));
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

    // 終わりだけなら、揃えた終わりの 7 日前から。
    let app = sample().await;
    app.html("/usage/?to=2026-11-22T12:10").await;
    let asked = self::asked(&app);
    assert_eq!(all(&asked[0], "from"), ["2026-11-15T04:00:00Z"]);
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
    assert_eq!(asked(&app).len(), 50);
    assert!(
        body.contains("<p class=\"error\" data-i18n=\"usage.tooMany\" data-i18n-options=\"{&#34;read&#34;:&#34;50&#34;}\">記録が多すぎて、全部を読めませんでした"),
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
        body.contains(
            "<p class=\"empty\" data-i18n=\"usage.empty\">この期間に、数の記録はありません。</p>"
        ),
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
async fn 使用量の引数の誤りは_訳を書いた_400_の画面で_castlist_に聞かない() {
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
        // 始めだけを、先の時刻にした（終わりは今）。打っていない「期間（まで）」の名を出さない（review-9 の #3）。
        (
            "/usage/?from=2099-01-01T00:00",
            "「期間（から）」が、先の時刻です。",
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
async fn 使用量は_script_も_inline_の_style_も使わず_利用者の字を_html_で逃がす() {
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
    let page = common::without_i18n_scripts(&body);
    for raw in ["<script", "<style", "style=", "<b>", "<i>", "<u>"] {
        assert!(!page.contains(raw), "{raw}: {body}");
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
async fn 頭の帯に_使用量の_link_が在る() {
    let app = sample().await;
    let (_, _, missing) = app.send(get("/no/such")).await;
    for page in [app.html("/").await, app.html(WINDOW).await, missing] {
        let nav = &page[page.find("<nav").unwrap()..page.find("</nav>").unwrap()];
        assert!(
            nav.contains("<a href=\"/usage/\" data-i18n=\"nav.usage\">使用量</a>"),
            "{nav}"
        );
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

    // 2 頁目より後で castlist が頁の印を断った時は、人の打った問いの誤りでなく、人には直せないので、503 の画面にする（review-9 の #2）。
    let app = start(vec![usage_page(rows(), actors(), Some("zz"))]).await;
    let (status, _, body) = app.send(get(WINDOW)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body.contains("castlist に聞けません（次の頁を断られました: nextToken が読めない）"),
        "{body}"
    );
}

#[tokio::test]
async fn 途中の頁で時間切れになっても_読めた分を出し_時間内に読めた分だと書く() {
    // 初めの頁はすぐ答え、2 頁目（p1）は、callboard の待つ時間（4 秒）より長く待たせる castlist。
    let pages = vec![
        usage_page(rows(), actors(), Some("p1")),
        usage_page(rows(), actors(), None),
    ];
    let app = App::start_with(
        usage_routes_slow(pages, 1, std::time::Duration::from_secs(8)),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get(WINDOW)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("<p class=\"error\" data-i18n=\"usage.tooSlow\" data-i18n-options=\"{&#34;read&#34;:&#34;6&#34;}\">時間内に、全部を読めませんでした"),
        "{body}"
    );
    assert!(body.contains("期間を狭めてください"), "{body}");
    // 読めた分（初めの頁）だけを数える（見本の 1 頁は 16 回）。
    assert_eq!(fact(&body, "回"), "16");
}

// --- 図（文脈の長さの段ごとの額・日ごとか時ごとの額） ---

/// 図の SVG（`<svg …>` から `</svg>` まで）。
fn svg(section: &str) -> &str {
    let start = section
        .find("<svg class=\"chart\"")
        .unwrap_or_else(|| panic!("図が無い: {section}"));
    &section[start..start + section[start..].find("</svg>").unwrap()]
}

#[tokio::test]
async fn 文脈の長さの段ごとの額は_本体の回の段を棒にし_20_万に点線を引く() {
    let app = sample().await;
    let body = app.html(WINDOW).await;
    let chart = section(&body, "by-context");
    assert!(
        chart.contains(
            "<h2 data-i18n=\"usage.byContext.heading\">文脈の長さの段ごとの額（本体の回）</h2>"
        ),
        "{chart}"
    );
    let svg = svg(chart);
    assert!(svg.contains("role=\"img\""), "{svg}");
    assert!(
        svg.contains("aria-label=\"文脈の長さの段ごとの額（本体の回）。"),
        "{svg}"
    );
    // 本体の回の段は 0・1・2・4・9・39 の 6 つ（手伝いの段 1 の $0.25 は混ぜない）。0 の段の棒は描かない。
    assert_eq!(svg.matches("<rect class=\"bar main\"").count(), 6, "{svg}");
    assert!(!svg.contains("bar helper"), "{svg}");
    // 棒にマウスを重ねると、段と額と回の数（段 0 は、pm.1 と se.2 の計）。
    for title in [
        ">0〜2.5 万: $0.11（2 回）</title>",
        ">2.5 万〜5 万: $0.50（1 回）</title>",
        ">22.5 万〜25 万: $0.50（2 回）</title>",
        ">97.5 万〜100 万: $3.00（1 回）</title>",
    ] {
        assert!(svg.contains(title), "{title}: {svg}");
    }
    // 鍵と値（英語は回の数で言い分ける。段の範囲と万の数は、鍵と値のまま入れ子で配る）。
    assert!(
        svg.contains(r#"<title data-i18n="usage.byContext.bar" data-i18n-options="{&#34;cost&#34;:&#34;$0.11&#34;,&#34;count&#34;:2,&#34;range&#34;:{&#34;$t&#34;:&#34;common.tokenRange&#34;,&#34;values&#34;:{&#34;high&#34;:{&#34;$t&#34;:&#34;common.tokens&#34;,&#34;values&#34;:{&#34;k&#34;:&#34;25&#34;,&#34;man&#34;:&#34;2.5&#34;}},&#34;low&#34;:&#34;0&#34;}},&#34;turns&#34;:&#34;2&#34;}">"#),
        "{svg}"
    );
    // 縦の目盛りは $0 から $3（$1 の刻み）。横の目盛りは 10 万ごと。
    for tick in [">$0</text>", ">$1</text>", ">$3</text>"] {
        assert!(svg.contains(tick), "{tick}: {svg}");
    }
    assert!(!svg.contains(">$4</text>"), "{svg}");
    for tick in [
        ">0</text>",
        ">10 万</text>",
        ">20 万</text>",
        ">90 万</text>",
        ">100 万〜</text>",
    ] {
        assert!(svg.contains(tick), "{tick}: {svg}");
    }
    assert!(!svg.contains(">2.5 万</text>"), "{svg}");
    // 20 万の点線と字。
    assert_eq!(svg.matches("<line class=\"limit\"").count(), 1, "{svg}");
    assert!(svg.contains("class=\"limit-label\""), "{svg}");
}

#[tokio::test]
async fn 期間が_1_日までなら_時ごとの額を_本体と手伝いを積んだ棒で出す() {
    let app = sample().await;
    let body = app.html(WINDOW).await;
    common::check_i18n("usage.rs:758", &body);
    let chart = section(&body, "by-day");
    assert!(
        chart.contains("<h2 data-i18n=\"usage.bySlot.heading.hour\">時ごとの額（日本時間）</h2>"),
        "{chart}"
    );
    let svg = svg(chart);
    assert!(
        svg.contains("role=\"img\"") && svg.contains("aria-label=\"時ごとの額"),
        "{svg}"
    );
    // 集めた期間は 10:00〜13:00 の 3 時間。行の時は、区間の終わりの時刻（toAt）で決める:
    // 10 時は本体 $5.01（se.1 の 2 行・tpm.1・pm.1）と手伝い $0.25、11 時は本体 $2.00（se.2）、12 時は無し。
    assert_eq!(svg.matches("<rect class=\"bar main\"").count(), 2, "{svg}");
    assert_eq!(
        svg.matches("<rect class=\"bar helper\"").count(),
        1,
        "{svg}"
    );
    for title in [
        r#"<title data-i18n="usage.bySlot.bar.main" data-i18n-options="{&#34;cost&#34;:&#34;$5.01&#34;,&#34;name&#34;:&#34;11-22 10:00&#34;,&#34;sum&#34;:&#34;$5.26&#34;}">11-22 10:00: 本体 $5.01（本体と手伝いの計 $5.26）</title>"#,
        r#"<title data-i18n="usage.bySlot.bar.helper" data-i18n-options="{&#34;cost&#34;:&#34;$0.25&#34;,&#34;name&#34;:&#34;11-22 10:00&#34;,&#34;sum&#34;:&#34;$5.26&#34;}">11-22 10:00: 手伝い $0.25（本体と手伝いの計 $5.26）</title>"#,
        ">11-22 11:00: 本体 $2.00（本体と手伝いの計 $2.00）</title>",
    ] {
        assert!(svg.contains(title), "{title}: {svg}");
    }
    for tick in [">10:00</text>", ">11:00</text>", ">12:00</text>"] {
        assert!(svg.contains(tick), "{tick}: {svg}");
    }
    // 手伝いの棒は、同じ時の本体の棒の上に積む（手伝いの棒の下の端が、本体の棒の上の端）。
    let attr = |rect: &str, name: &str| -> u32 {
        let at = rect.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
        rect[at..at + rect[at..].find('"').unwrap()]
            .parse()
            .unwrap()
    };
    let rect = |class: &str| {
        let at = svg.find(&format!("<rect class=\"{class}\"")).unwrap();
        &svg[at..at + svg[at..].find('>').unwrap()]
    };
    let (main, helper) = (rect("bar main"), rect("bar helper"));
    assert_eq!(attr(helper, "y") + attr(helper, "height"), attr(main, "y"));
    assert_eq!(attr(helper, "x"), attr(main, "x"));
    // 凡例。
    assert!(
        chart.contains("swatch main") && chart.contains("swatch helper"),
        "{chart}"
    );
}

#[tokio::test]
async fn 期間が_1_日を越えれば_日本時間の日ごとの額() {
    let app = sample().await;
    let body = app
        .html("/usage/?from=2026-11-20T00:00&to=2026-11-23T00:00")
        .await;
    let chart = section(&body, "by-day");
    assert!(
        chart.contains("<h2 data-i18n=\"usage.bySlot.heading.day\">日ごとの額（日本時間）</h2>"),
        "{chart}"
    );
    let svg = svg(chart);
    for tick in [">11-20</text>", ">11-21</text>", ">11-22</text>"] {
        assert!(svg.contains(tick), "{tick}: {svg}");
    }
    // 行はみな 11-22（日本時間）の物: 本体 $7.01・手伝い $0.25。
    assert_eq!(svg.matches("<rect class=\"bar main\"").count(), 1, "{svg}");
    assert_eq!(
        svg.matches("<rect class=\"bar helper\"").count(),
        1,
        "{svg}"
    );
    assert!(
        svg.contains(
            r#"<title data-i18n="usage.bySlot.bar.main" data-i18n-options="{&#34;cost&#34;:&#34;$7.01&#34;,&#34;name&#34;:&#34;11-22&#34;,&#34;sum&#34;:&#34;$7.26&#34;}">11-22: 本体 $7.01（本体と手伝いの計 $7.26）</title>"#
        ),
        "{svg}"
    );
}

#[tokio::test]
async fn 図は_script_を使わず_行が無ければ出さない() {
    let app = sample().await;
    let body = app.html(WINDOW).await;
    common::check_i18n("usage.rs:841", &body);
    assert_eq!(body.matches("<svg class=\"chart\"").count(), 2, "{body}");
    assert!(
        !common::without_i18n_scripts(&body).contains("<script"),
        "{body}"
    );
    assert!(!body.contains("style="), "{body}");

    let app = start(vec![usage_page(vec![], vec![], None)]).await;
    let body = app.html(WINDOW).await;
    assert!(!body.contains("<svg"), "{body}");
    assert!(
        !body.contains("by-context") && !body.contains("by-day"),
        "{body}"
    );

    // 手伝いの回だけなら、段の図は出さずにそう書く（日ごとの図は出す）。
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
    let chart = section(&body, "by-context");
    assert!(!chart.contains("<svg"), "{chart}");
    assert!(
        chart.contains("<p class=\"empty\" data-i18n=\"usage.byContext.empty\">この期間に、本体の回の記録はありません。</p>"),
        "{chart}"
    );
    assert_eq!(
        svg(section(&body, "by-day"))
            .matches("<rect class=\"bar helper\"")
            .count(),
        1
    );
}

#[tokio::test]
async fn 日ごとの額は_図の下に畳んだ字の表でも読める() {
    let app = sample().await;
    let body = app.html(WINDOW).await;
    let day = section(&body, "by-day");
    assert!(
        day.contains("<details class=\"slots\"><summary data-i18n=\"usage.bySlot.table.hour\">時ごとの額を、表で見る</summary>"),
        "{day}"
    );
    // 額の在る区切りだけを出す。表の計の欄の和は、全体の額（$7.26）と同じ。
    let table = day.split("<details").nth(1).unwrap();
    let totals: Vec<f64> = table
        .split("<tr><td>")
        .skip(1)
        .map(|tr| {
            let last = tr.split("<td class=\"num\">").last().unwrap();
            last[1..last.find("</td>").unwrap()]
                .replace(',', "")
                .parse::<f64>()
                .unwrap()
        })
        .collect();
    assert!(!totals.is_empty(), "{table}");
    assert!(totals.iter().all(|t| *t > 0.0), "{table}");
    let sum: f64 = totals.iter().sum();
    assert!((sum - 7.26).abs() < 0.015, "{sum}: {table}");
}

// --- 絞りの候補（t-nd3w。役割と課題は選ぶ一覧、ほかのラベルは候補付きの欄） ---

/// 一覧の option（値・字・選んであるか）。`key` は欄の名の鍵の末尾（`role`・`subject`）。
fn select(body: &str, key: &str) -> Vec<(String, String, bool)> {
    let at = body
        .find(&format!("data-i18n=\"usage.filters.{key}\""))
        .unwrap_or_else(|| panic!("{key} の一覧が無い: {body}"));
    let start = at + body[at..].find("<select").unwrap();
    let end = start + body[start..].find("</select>").unwrap();
    options(&body[start..end])
}

/// 選んである値（「すべて」なら無い）。
fn selected(options: &[(String, String, bool)]) -> Option<String> {
    let chosen: Vec<_> = options.iter().filter(|(_, _, s)| *s).collect();
    assert!(chosen.len() <= 1, "{options:?}");
    chosen
        .first()
        .map(|(v, _, _)| v.clone())
        .filter(|v| !v.is_empty())
}

/// 「ほかのラベル」の欄の値（上から）。
fn fields(body: &str) -> Vec<String> {
    body.split("<input name=\"label\" value=\"")
        .skip(1)
        .map(|rest| common::unwrap_text(&rest[..rest.find('"').unwrap()]).to_owned())
        .collect()
}

/// 「ほかのラベル」の欄の候補（値と字）。
fn datalist(body: &str) -> Vec<(String, String)> {
    let start = body
        .find("<datalist id=\"label-choices\">")
        .unwrap_or_else(|| panic!("候補が無い: {body}"));
    let end = start + body[start..].find("</datalist>").unwrap();
    options(&body[start..end])
        .into_iter()
        .map(|(v, t, _)| (v, t))
        .collect()
}

fn options(html: &str) -> Vec<(String, String, bool)> {
    html.split("<option value=\"")
        .skip(1)
        .map(|rest| {
            let value = rest[..rest.find('"').unwrap()].to_owned();
            let tag = &rest[..rest.find('>').unwrap()];
            let text = &rest[rest.find('>').unwrap() + 1..rest.find("</option>").unwrap()];
            (value, text.to_owned(), tag.contains(" selected"))
        })
        .collect()
}

fn texts(options: &[(String, String, bool)]) -> Vec<(&str, &str)> {
    options
        .iter()
        .map(|(v, t, _)| (v.as_str(), t.as_str()))
        .collect()
}

#[tokio::test]
async fn 役割と課題は_期間に稼働した席から_席の多い順の一覧で選び_ほかのラベルは欄の候補に出す() {
    let app = sample().await;
    let body = app.html(WINDOW).await;
    let roles = select(&body, "role");
    assert_eq!(
        texts(&roles),
        [
            ("", "すべて"),
            ("tpm.role:software-engineer", "software-engineer（2 席）"),
            ("tpm.role:tpm", "tpm（1 席）"),
        ]
    );
    assert_eq!(selected(&roles), None);
    assert_eq!(
        texts(&select(&body, "subject")),
        [
            ("", "すべて"),
            ("tpm.subject:t-j4pp", "t-j4pp（2 席）"),
            ("tpm.subject:t-23zs", "t-23zs（1 席）"),
        ]
    );
    // 一覧の名は label で、送る値は「鍵:値」（URL は欄に打った時と同じ形）。
    assert_eq!(body.matches("<select name=\"label\">").count(), 2, "{body}");
    // ほかの鍵は欄の候補。起こした回の id（tpm.launch）は出さない。字にも値を書く（Firefox は候補に値の代わりに字を出すため）。
    assert_eq!(
        datalist(&body),
        [
            (
                "tpm.key:t-23zs".to_owned(),
                "tpm.key:t-23zs（1 席）".to_owned()
            ),
            (
                "tpm.key:t-j4pp".to_owned(),
                "tpm.key:t-j4pp（1 席）".to_owned()
            ),
            (
                "tpm.key:t-j4pp.staff".to_owned(),
                "tpm.key:t-j4pp.staff（1 席）".to_owned()
            ),
        ]
    );
    assert!(!body.contains("tpm.launch"), "{body}");
    // 欄は候補を指し、空の 1 つだけ。
    assert_eq!(fields(&body), [""]);
    assert!(
        body.contains("<label><span data-i18n=\"usage.filters.otherLabel\">ほかのラベル</span> <input name=\"label\" value=\"\" list=\"label-choices\""),
        "{body}"
    );
    // 英語は、ブラウザーが同じ鍵と値で差し替える（数は count で言い分ける）。
    assert!(
        body.contains("data-i18n=\"usage.filters.choice\" data-i18n-options=\"{&#34;count&#34;:2,&#34;name&#34;:&#34;software-engineer&#34;}\""),
        "{body}"
    );
    assert!(
        body.contains(
            "data-i18n=\"usage.filters.choice\" data-i18n-options=\"{&#34;count&#34;:1,&#34;name&#34;:&#34;tpm.key:t-23zs&#34;}\""
        ),
        "{body}"
    );
    assert!(
        body.contains("<option value=\"\" data-i18n=\"filters.all\">すべて</option>"),
        "{body}"
    );
}

#[tokio::test]
async fn 候補は_表示と同じ期間で_ラベルの絞りを付けずに_listactors_に_1_度聞く() {
    let app = sample().await;
    app.html(
        "/usage/?from=2026-11-22T10:30&to=2026-11-22T12:10&label=tpm.role:tpm&label=tpm.key:t-1",
    )
    .await;
    // castlist に聞くのは、使用量の行と候補の 2 つの道だけ。
    for seen in app.castlist.seen() {
        let path = seen
            .uri
            .split_once('?')
            .map_or(seen.uri.as_str(), |(p, _)| p);
        assert!(
            path == "/api/usage" || path == "/api/actors",
            "{}",
            seen.uri
        );
    }
    let usage = asked(&app);
    let actors = asked_at(&app, "/api/actors");
    assert_eq!(actors.len(), 1, "{actors:?}");
    // 期間は、使用量の行を聞いた期間（時の境に揃えた物）と同じ。
    assert_eq!(all(&actors[0], "activeFrom"), all(&usage[0], "from"));
    assert_eq!(all(&actors[0], "activeTo"), all(&usage[0], "to"));
    assert_eq!(all(&actors[0], "activeFrom"), ["2026-11-22T01:00:00Z"]);
    // 絞りは付けない（絞っても候補が減らない）。新しく登場した席から 1000 まで。
    assert!(all(&actors[0], "label").is_empty(), "{actors:?}");
    assert_eq!(all(&actors[0], "maxResults"), ["1000"]);
    assert_eq!(all(&actors[0], "sort"), ["entered"]);
    assert_eq!(all(&actors[0], "order"), ["desc"]);
    // 使用量の行は、今までどおりラベルで絞る。
    assert_eq!(all(&usage[0], "label"), ["tpm.role:tpm", "tpm.key:t-1"]);
}

#[tokio::test]
async fn url_の初めの役割と課題を一覧で選び_残りは_url_の順に欄に出し_候補に無い値も残す() {
    let app = sample().await;
    let body = app
        .html("/usage/?from=2026-11-22T10:30&to=2026-11-22T12:10&label=tpm.key:t-1&label=tpm.subject:t-23zs&label=tpm.role:auditor&label=tpm.role:tpm")
        .await;
    let roles = select(&body, "role");
    assert_eq!(selected(&roles), Some("tpm.role:auditor".to_owned()));
    // 候補に無い今の値は、数を付けずに後ろに足す。
    assert_eq!(
        roles.last().map(|(v, t, _)| (v.as_str(), t.as_str())),
        Some(("tpm.role:auditor", "auditor"))
    );
    assert_eq!(
        selected(&select(&body, "subject")),
        Some("tpm.subject:t-23zs".to_owned())
    );
    assert_eq!(fields(&body), ["tpm.key:t-1", "tpm.role:tpm", ""]);
    // 送り直すと、同じ条件になる（一覧の値も欄の値も、名は label）。
    assert_eq!(
        all(&asked(&app)[0], "label"),
        [
            "tpm.key:t-1",
            "tpm.subject:t-23zs",
            "tpm.role:auditor",
            "tpm.role:tpm"
        ]
    );
}

#[tokio::test]
async fn 候補を引けなくても_使用量の画面は出し_一覧は今の値だけ() {
    let app = App::start_with(
        with_actors_down(usage_routes(vec![usage_page(rows(), actors(), None)])),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get(&format!("{WINDOW}&label=tpm.role:tpm"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("<section id=\"total\">"), "{body}");
    assert_eq!(
        texts(&select(&body, "role")),
        [("", "すべて"), ("tpm.role:tpm", "tpm")]
    );
    assert_eq!(texts(&select(&body, "subject")), [("", "すべて")]);
    assert!(datalist(&body).is_empty());
}

#[tokio::test]
async fn 候補の字は_html_で逃がす() {
    let app = App::start_with(
        with_actors(
            usage_routes(vec![usage_page(rows(), actors(), None)]),
            vec![actor_row(
                SE1,
                "se.1",
                json!({"tpm.role": "<i>r</i>", "tpm.subject": "\"x\" & y", "a": "<b>"}),
            )],
        ),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let body = app.html(WINDOW).await;
    let page = common::without_i18n_scripts(&body);
    for raw in ["<script", "<i>", "<b>"] {
        assert!(!page.contains(raw), "{raw}: {body}");
    }
    assert!(
        body.contains("<option value=\"tpm.role:&#60;i&#62;r&#60;/i&#62;\""),
        "{body}"
    );
    assert!(
        body.contains("<option value=\"tpm.subject:&#34;x&#34; &#38; y\""),
        "{body}"
    );
    assert!(body.contains("<option value=\"a:&#60;b&#62;\""), "{body}");
}

#[tokio::test]
async fn 候補を引くのが遅くても_使用量の画面は待たせず_一覧は今の値だけ() {
    let app = App::start_with(
        with_actors_slow(
            usage_routes(vec![usage_page(rows(), actors(), None)]),
            active(),
            std::time::Duration::from_secs(3),
        ),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let started = std::time::Instant::now();
    let (status, _, body) = app.send(get(&format!("{WINDOW}&label=tpm.role:tpm"))).await;
    // 候補は 1 秒で切る（使用量の数は ListUsage だけで決まる）。
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("<section id=\"total\">"), "{body}");
    assert_eq!(
        texts(&select(&body, "role")),
        [("", "すべて"), ("tpm.role:tpm", "tpm")]
    );
    assert!(datalist(&body).is_empty());
}

// --- model と役割ごとの行列（t-dues） ---

/// 行列の見本の行: [`rows`] の se.2 の本体の行だけを Fable にする（ほかは Opus）。額の計は $7.26 のまま。
fn model_rows() -> Vec<Value> {
    let mut rows = rows();
    rows[3]["model"] = json!("claude-fable-5-2");
    rows
}

/// [`active`] の席に、最新の effort を付けた物（se.1 は medium・se.2 は high・tpm.1 は medium。pm.1 は無し）。
fn active_efforts() -> Vec<Value> {
    let mut active = active();
    for (row, effort) in active.iter_mut().zip(["medium", "high", "medium"]) {
        row["model"] = json!("claude-opus-5-5");
        row["effort"] = json!(effort);
    }
    active
}

/// 表の行ごとの欄の字（`<td>`。`<br>` は空白にし、ほかの tag は外す）。`part` は `thead`・`tbody`・`tfoot`。
fn grid(section: &str, part: &str) -> Vec<Vec<String>> {
    let start = section
        .find(&format!("<{part}>"))
        .unwrap_or_else(|| panic!("{part} が無い: {section}"));
    let body = &section[start..start + section[start..].find(&format!("</{part}>")).unwrap()];
    body.split("<tr>")
        .skip(1)
        .map(|row| {
            row.split("<t")
                .skip(1)
                .filter(|cell| cell.starts_with('d') || cell.starts_with('h'))
                .map(|cell| {
                    let inner = &cell[cell.find('>').unwrap() + 1..];
                    let inner = &inner[..inner.find("</t").unwrap_or(inner.len())];
                    strip(&inner.replace("<br>", " "))
                })
                .collect()
        })
        .collect()
}

/// tag を外し、空白を 1 つにまとめる（字の実体参照は戻さない）。
fn strip(html: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 表の本体の行ごとの、升の class（`heat-N` だけ。無ければ空の字）。
fn heats(section: &str) -> Vec<Vec<String>> {
    let start = section.find("<tbody>").unwrap();
    let body = &section[start..start + section[start..].find("</tbody>").unwrap()];
    body.split("<tr>")
        .skip(1)
        .map(|row| {
            row.split("<td class=\"num")
                .skip(1)
                .map(|cell| {
                    let class = &cell[..cell.find('"').unwrap()];
                    class
                        .split_whitespace()
                        .find(|c| c.starts_with("heat-"))
                        .unwrap_or("")
                        .to_owned()
                })
                .collect()
        })
        .collect()
}

async fn matrix_app(actors_route: fn(axum::Router, Vec<Value>) -> axum::Router) -> App {
    App::start_with(
        actors_route(
            usage_routes(vec![usage_page(model_rows(), actors(), None)]),
            active_efforts(),
        ),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await
}

#[tokio::test]
async fn model_と役割ごとの行列は_役割と手伝いの種類を行に_model_を額の大きい順の列に出す() {
    let app = matrix_app(with_actors).await;
    let (status, _, body) = app.send(get(WINDOW)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // 節は「日ごと」と「役割ごと」の間。
    let at = |id: &str| body.find(&format!("<section id=\"{id}\">")).unwrap();
    assert!(
        at("by-day") < at("models") && at("models") < at("roles"),
        "{body}"
    );
    let models = section(&body, "models");
    assert!(models.contains(">model と役割ごと</h2>"), "{models}");
    assert_eq!(
        grid(models, "thead"),
        [["役割・手伝い", "claude-opus-5-5", "claude-fable-5-2", "計"]]
    );
    // 上の組は役割ごとの表と同じ並び、effort は席の数の多い順（同じなら名の順）。下の組は手伝いの種類ごと。
    assert_eq!(
        grid(models, "tbody"),
        [
            vec![
                "software-engineer high（1 席）・medium（1 席）",
                "$2.00 5 回",
                "$2.00 5 回",
                "$4.00"
            ],
            vec![
                "technical-program-manager medium",
                "$3.00 1 回",
                "—",
                "$3.00"
            ],
            vec!["（無し）", "$0.01 1 回", "—", "$0.01"],
            vec!["手伝い: Explore", "$0.25 4 回", "—", "$0.25"],
        ]
    );
    // 下の行: model ごとの計・全体の額に対する比・1 回あたり（小数 3 桁）。
    assert_eq!(
        grid(models, "tfoot"),
        [
            ["計", "$5.26", "$2.00", "$7.26"],
            ["全体の額に対する比", "72.5%", "27.5%", "100.0%"],
            ["1 回あたり", "$0.478", "$0.400", "$0.454"],
        ]
    );
    // 地の色: 全体の額 $7.26 に対して、10% 以上は 3・1% 以上は 2・それより下は 1。升の無い所は無し。
    assert_eq!(
        heats(models),
        [
            vec!["heat-3", "heat-3", ""],
            vec!["heat-3", "", ""],
            vec!["heat-1", "", ""],
            vec!["heat-2", "", ""],
        ]
    );
    // 升にマウスを重ねると 1 回あたり（英語にも替わるよう、鍵と値を付ける）。
    assert!(
        models
            .contains("title=\"1 回あたり $0.400\" data-i18n-attr=\"title:usage.models.perTurn\""),
        "{models}"
    );
    // 凡例は、段の境の数を値で受ける。effort と手伝いの model の注。
    assert!(models.contains("10% 以上"), "{models}");
    assert!(
        models.contains("effort は、役割の行の席の今の値"),
        "{models}"
    );
    assert!(models.contains("本体の model を受け継ぐ物"), "{models}");
    assert!(!models.contains("時間内に引けませんでした"), "{models}");
    // 色は class で付け、inline の style は使わない。
    assert!(!models.contains("style="), "{models}");
}

#[tokio::test]
async fn effort_を引けなければ_effort_を出さず注を書き_ほかの数は出す() {
    let app = App::start_with(
        with_actors_slow(
            usage_routes(vec![usage_page(model_rows(), actors(), None)]),
            active_efforts(),
            std::time::Duration::from_secs(3),
        ),
        common::pagetray_routes(),
        callboard::Config::default(),
    )
    .await;
    let (status, _, body) = app.send(get(WINDOW)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let models = section(&body, "models");
    assert_eq!(
        grid(models, "tbody")[0],
        ["software-engineer", "$2.00 5 回", "$2.00 5 回", "$4.00"]
    );
    assert!(
        models.contains("effort を、時間内に引けませんでした"),
        "{models}"
    );
}

#[tokio::test]
async fn 席が_1_頁の上限を越えたら_effort_は新しく登場した席の分だけと書く() {
    let app = matrix_app(with_actors_more).await;
    let (status, _, body) = app.send(get(WINDOW)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let models = section(&body, "models");
    // 引けた分の effort は出す。
    assert_eq!(
        grid(models, "tbody")[1][0],
        "technical-program-manager medium"
    );
    assert!(
        models.contains("effort は、新しく登場した 1000 席の分だけです"),
        "{models}"
    );
}

#[tokio::test]
async fn 行が無ければ_行列の節も出さない() {
    let app = start(vec![usage_page(vec![], vec![], None)]).await;
    let (status, _, body) = app.send(get(WINDOW)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("<section id=\"models\">"), "{body}");
}
