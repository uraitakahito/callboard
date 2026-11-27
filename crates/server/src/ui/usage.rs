//! 費え（`GET /usage/`）: 席（アクター）ごとのトークンの数と額を、全体・役割ごと・課題ごとの表で出す。
//!
//! castlist の API の `ListUsage`（castlist 2.1.0 から。アクター 1 人の、時計の 1 時間の内の、同じ種類の回のまとめの行）を、
//! 期間の内でみな引き、まとめる計算（[`super::usage_summary`]。外に触らない純な計算）で数を出す。script は使わない。
//!
//! - 期間（`from`・`to`）は、時の境に揃えてから castlist に送る（始めは時の初めに切り下げ、終わりは時の境に切り上げる）。
//!   行が期間に入るかは、行の終わりの時刻で決まるので、時の途中で切ると、端の 1 時間の行が欠けたり余ったりする。
//!   日本時間（UTC+9）と UTC は、時の境が同じ。送った期間を、画面に出す。
//! - まとめる計算と HTML を組むのは、タイムラインと同じく `spawn_blocking` の糸で行う（行は 5 万まで在りうる）。
use std::sync::Arc;

use askama::Template;
use aws_smithy_types::DateTime;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::params::{self, PageError, Pairs, Params, bad_request};
use super::time::{Time, local, stamp, time};
use super::usage_summary::{self as summary, BUCKET_WIDTH, Summary};
use super::{CSS_HREF, Nav, error_page, render};
use crate::App;
use crate::upstream::Failure;
use crate::upstream::castlist::{Cut, UsageAnswer, UsageSearch};

/// 費えの道。
pub const PATH: &str = "/usage/";

/// 引数の名。`label` は何度でも。
const USAGE_PARAMS: &[&str] = &["label", "from", "to"];

/// 期間の欄の引数の名と、画面の見出しの字（読めない値の 400 の画面に書く）。
const PERIOD: [&str; 2] = ["from", "to"];
const PERIOD_FIELDS: [(&str, &str); 2] = [("from", "期間（から）"), ("to", "期間（まで）")];

/// 期間が無い時の長さ（直近の日数）と、castlist が受ける期間の上限（日数）。
const DEFAULT_DAYS: i64 = 7;
const MAX_DAYS: i64 = 31;

/// 時の初めに切り下げる。
fn floor_hour(t: OffsetDateTime) -> OffsetDateTime {
    let seconds = t.unix_timestamp();
    OffsetDateTime::from_unix_timestamp(seconds - seconds.rem_euclid(3600)).unwrap_or(t)
}

/// 時の境に切り上げる（ちょうどの時は、そのまま）。
fn ceil_hour(t: OffsetDateTime) -> OffsetDateTime {
    let floor = floor_hour(t);
    if floor == t {
        t
    } else {
        floor + time::Duration::HOUR
    }
}

/// castlist に送る期間（時の境に揃えた始めと終わり）。終わりが無ければ今まで、始めが無ければ、揃えた終わりの
/// [`DEFAULT_DAYS`] 日前から（どちらも無ければ、直近 [`DEFAULT_DAYS`] 日）。終わりが始めより後でない・揃えた後で [`MAX_DAYS`] 日を越える期間は 400。
fn window(
    from: Option<OffsetDateTime>,
    to: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> Result<(OffsetDateTime, OffsetDateTime), PageError> {
    let end = to.unwrap_or(now);
    // 終わりを打たずに、始めだけを先の時刻にした時は、打っていない「期間（まで）」の名を出さない。
    if to.is_none() && from.is_some_and(|from| from >= now) {
        return Err(bad_request("「期間（から）」が、先の時刻です。"));
    }
    let to = ceil_hour(end);
    // 始めが無ければ、揃えた終わりから、ちょうど DEFAULT_DAYS 日前。
    let start = from.unwrap_or(to - time::Duration::days(DEFAULT_DAYS));
    if end <= start {
        return Err(bad_request(
            "「期間（まで）」は、「期間（から）」より後にしてください。",
        ));
    }
    let from = floor_hour(start);
    if to - from > time::Duration::days(MAX_DAYS) {
        return Err(bad_request(format!(
            "期間は {MAX_DAYS} 日までです（時の境に揃えた後の長さ）。期間を狭めてください。"
        )));
    }
    Ok((from, to))
}

// --- 数の字 ---

/// 数を 3 桁ごとに区切る（`12,345`）。
fn count(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    if n < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// 額（100 万分の 1 ドル）を、ドルの小数 2 桁にする（`$1,234.56`。四捨五入）。
fn dollars(micro: i64) -> String {
    let cents = micro.max(0).saturating_add(5_000) / 10_000;
    format!("${}.{:02}", count(cents / 100), cents % 100)
}

/// 比を % の小数 1 桁にする（`12.5%`。四捨五入）。元が 0 なら無い。
fn percent(part: i64, whole: i64) -> Option<String> {
    if whole <= 0 {
        return None;
    }
    let tenths = (i128::from(part.max(0)) * 2_000 + i128::from(whole)) / (i128::from(whole) * 2);
    Some(format!("{}.{}%", tenths / 10, tenths % 10))
}

/// トークンの数を「万」の単位にする（`22.5 万`・`97 万`。小数 1 桁に四捨五入し、`.0` は書かない）。
fn man(tokens: i64) -> String {
    let tenths = tokens.max(0).saturating_add(500) / 1_000;
    if tenths % 10 == 0 {
        format!("{} 万", tenths / 10)
    } else {
        format!("{}.{} 万", tenths / 10, tenths % 10)
    }
}

/// 段の下の端を、「22.5 万〜」の形にする。
fn from_man(tokens: i64) -> String {
    format!("{}〜", man(tokens))
}

/// いちばん長い文脈。本体の回が無ければ（0 なら）無い。
fn longest(tokens: i64) -> Option<String> {
    (tokens > 0).then(|| man(tokens))
}

// --- 画面に出す形 ---

/// 全体の数。
struct TotalView {
    seats: String,
    turns: String,
    cost: String,
    helper_turns: String,
    helper_cost: String,
    /// 手伝いの額の、全体の額に対する比。
    helper_share: Option<String>,
}

/// 役割ごとの表の行。無い値（本体の回が無い時の、文脈の長さの数）は「—」。
struct RoleView {
    name: Option<String>,
    seats: String,
    turns: String,
    cost: String,
    /// 本体の額の、全体の額に対する比。
    share: Option<String>,
    median: Option<String>,
    p90: Option<String>,
    max_context: Option<String>,
    over_turns: Option<String>,
    over_cost: Option<String>,
    helper_cost: String,
}

/// 課題ごとの表の行。
struct SubjectView {
    name: Option<String>,
    seats: String,
    turns: String,
    cost: String,
    max_context: Option<String>,
}

struct Tables {
    total: TotalView,
    roles: Vec<RoleView>,
    subjects: Vec<SubjectView>,
}

fn tables(summary: Summary) -> Tables {
    let whole = summary.total.cost;
    let seats = |n: usize| count(i64::try_from(n).unwrap_or(i64::MAX));
    Tables {
        total: TotalView {
            seats: seats(summary.total.seats),
            turns: count(summary.total.turns),
            cost: dollars(whole),
            helper_turns: count(summary.total.helper_turns),
            helper_cost: dollars(summary.total.helper_cost),
            helper_share: percent(summary.total.helper_cost, whole),
        },
        roles: summary
            .roles
            .into_iter()
            .map(|r| RoleView {
                seats: seats(r.seats),
                turns: count(r.turns),
                cost: dollars(r.cost),
                share: percent(r.cost, whole),
                median: r.median.map(from_man),
                p90: r.p90.map(from_man),
                max_context: longest(r.max_context),
                over_turns: percent(r.over_turns, r.bucket_turns),
                over_cost: percent(r.over_cost, r.bucket_cost),
                helper_cost: dollars(r.helper_cost),
                name: r.name,
            })
            .collect(),
        subjects: summary
            .subjects
            .into_iter()
            .map(|s| SubjectView {
                seats: seats(s.seats),
                turns: count(s.turns),
                cost: dollars(s.cost),
                max_context: longest(s.max_context),
                name: s.name,
            })
            .collect(),
    }
}

/// castlist の答えを、まとめる計算に渡す形にする。
fn summarize(answer: UsageAnswer) -> Summary {
    let rows: Vec<summary::Row> = answer
        .rows
        .into_iter()
        .map(|u| summary::Row {
            actor_id: u.actor_id,
            helper: u.agent.is_some(),
            turns: u.turns,
            cost: u.cost_micro_usd,
            max_context: u.max_context,
            buckets: u
                .buckets
                .into_iter()
                .map(|b| summary::Bucket {
                    bucket: b.bucket,
                    turns: b.turns,
                    cost: b.cost_micro_usd,
                })
                .collect(),
        })
        .collect();
    let actors: Vec<summary::Actor> = answer
        .actors
        .into_iter()
        .map(|a| summary::Actor {
            id: a.id,
            labels: a.labels,
        })
        .collect();
    summary::summarize(&rows, &actors)
}

/// 絞りの form に、今の値を入れて出す。
#[derive(Default)]
struct Form {
    labels: Vec<String>,
    from: String,
    to: String,
}

impl Form {
    fn new(pairs: &Pairs) -> Self {
        let mut form = Self::default();
        for (name, value) in pairs {
            match name.as_str() {
                "label" => form.labels.push(value.clone()),
                "from" => form.from = params::to_local(value),
                "to" => form.to = params::to_local(value),
                _ => {}
            }
        }
        form
    }
}

#[derive(Template)]
#[template(path = "usage.html")]
struct UsagePage<'a> {
    css: &'static str,
    nav: &'a Nav,
    form: Form,
    /// 期間をすぐ選ぶ link（名と URL）。
    quick: Vec<(&'static str, String)>,
    /// castlist に送った期間（時の境に揃えた後）。
    from: Time,
    to: Time,
    /// 期間が無かったので、直近の日数で出したか。
    defaulted: bool,
    days: i64,
    /// 途中で止めたか（行が多すぎた・時間内に読み切れなかった）。その時に読んだ行の数。
    too_many: bool,
    too_slow: bool,
    read: String,
    /// 行が 1 つも無ければ無い。
    tables: Option<Tables>,
    /// 段の幅（「2.5 万」）。
    width: String,
    as_of: Time,
}

fn time_of(t: OffsetDateTime) -> Time {
    Time {
        iso: stamp(t),
        local: local(t),
    }
}

/// `GET /usage/?label=…&from=…&to=…`: 費え。空の値は付けなかったのと同じ（form は空の欄も送る）。
pub async fn usage(
    State(app): State<Arc<App>>,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Response {
    match usage_page(app.clone(), q).await {
        Ok(response) => response,
        Err(PageError(status, message)) => error_page(&app.nav, status, &message),
    }
}

async fn usage_page(
    app: Arc<App>,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Result<Response, PageError> {
    let mut pairs = params::pairs(q)?;
    pairs.retain(|(_, v)| !v.is_empty());
    params::period_named(&mut pairs, PERIOD_FIELDS)?;
    let p = Params::new(pairs.clone(), USAGE_PARAMS, &["label"])?;
    let labels = p
        .all("label")
        .map(|v| params::pair("label", v, "鍵:値").map(|()| v.to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    // 期間の欄の値は、上で RFC 3339 にそろえてある。
    let at = |name: &str| {
        p.one(name)
            .and_then(|v| OffsetDateTime::parse(v, &Rfc3339).ok())
    };
    let now = OffsetDateTime::now_utc();
    let defaulted = PERIOD.iter().all(|name| p.one(name).is_none());
    let (from, to) = window(at("from"), at("to"), now)?;
    let form = Form::new(&pairs);
    let quick = params::quick_links(PATH, &pairs, PERIOD, now);
    let search = UsageSearch {
        from: DateTime::from_secs(from.unix_timestamp()),
        to: DateTime::from_secs(to.unix_timestamp()),
        labels,
    };
    let answer = app.castlist.usage(&search).await.map_err(|e| match e {
        // castlist が引数を断った（callboard の確かめを通った物なので、ふつうは起きない）。castlist の訳を、そのまま書く。
        Failure::Rejected(message) => {
            bad_request(format!("castlist が引数を断りました: {message}"))
        }
        // この操作に、名指す物は無い。
        Failure::NotFound => bad_request("castlist が引数を断りました"),
        Failure::Unavailable(why) => PageError(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("castlist に聞けません（{why}）。少し待ってから開き直してください。"),
        ),
    })?;
    tokio::task::spawn_blocking(move || {
        let as_of = time(&answer.as_of);
        let too_many = answer.truncated == Some(Cut::Pages);
        let too_slow = answer.truncated == Some(Cut::Time);
        let read = count(i64::try_from(answer.rows.len()).unwrap_or(i64::MAX));
        let tables = (!answer.rows.is_empty()).then(|| tables(summarize(answer)));
        let page = UsagePage {
            css: &CSS_HREF,
            nav: &app.nav,
            form,
            quick,
            from: time_of(from),
            to: time_of(to),
            defaulted,
            days: DEFAULT_DAYS,
            too_many,
            too_slow,
            read,
            tables,
            width: man(BUCKET_WIDTH),
            as_of,
        };
        render(StatusCode::OK, &page)
    })
    .await
    .map_err(|e| {
        tracing::error!("費えを表示できません: {e}");
        PageError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "費えを表示できません".to_owned(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(iso: &str) -> OffsetDateTime {
        OffsetDateTime::parse(iso, &Rfc3339).unwrap()
    }

    #[test]
    fn 期間は_始めを時の初めに切り下げ_終わりを時の境に切り上げる() {
        let now = at("2026-11-28T03:20:15Z");
        let got = window(
            Some(at("2026-11-22T10:30:00+09:00")),
            Some(at("2026-11-22T12:10:00.250+09:00")),
            now,
        );
        assert_eq!(
            got.ok(),
            Some((at("2026-11-22T01:00:00Z"), at("2026-11-22T04:00:00Z")))
        );
        // ちょうどの時は、そのまま。
        let got = window(
            Some(at("2026-11-22T01:00:00Z")),
            Some(at("2026-11-22T03:00:00Z")),
            now,
        );
        assert_eq!(
            got.ok(),
            Some((at("2026-11-22T01:00:00Z"), at("2026-11-22T03:00:00Z")))
        );
        // 期間が無ければ、今を切り上げた時刻までの 7 日。
        assert_eq!(
            window(None, None, now).ok(),
            Some((at("2026-11-21T04:00:00Z"), at("2026-11-28T04:00:00Z")))
        );
        // 始めだけなら、今まで。
        assert_eq!(
            window(Some(at("2026-11-27T00:10:00Z")), None, now).ok(),
            Some((at("2026-11-27T00:00:00Z"), at("2026-11-28T04:00:00Z")))
        );
    }

    #[test]
    fn 期間は_揃えた後で_31_日まで_終わりは始めより後() {
        let now = at("2026-11-28T03:20:15Z");
        let ok = |from: &str, to: &str| window(Some(at(from)), Some(at(to)), now).is_ok();
        assert!(ok("2026-10-01T00:00:00Z", "2026-11-01T00:00:00Z"));
        assert!(!ok("2026-10-01T00:00:00Z", "2026-11-01T00:00:01Z"));
        assert!(!ok("2026-09-30T23:59:59Z", "2026-11-01T00:00:00Z"));
        assert!(!ok("2026-11-22T03:00:00Z", "2026-11-22T03:00:00Z"));
        // 同じ時の内でも、終わりが前なら断る（揃えると 1 時間の期間に見えてしまう）。
        assert!(!ok("2026-11-22T03:30:00Z", "2026-11-22T03:10:00Z"));
        assert!(ok("2026-11-22T03:10:00Z", "2026-11-22T03:30:00Z"));
        // 始めだけで、31 日より前。
        assert!(window(Some(at("2026-10-01T00:00:00Z")), None, now).is_err());
    }

    #[test]
    fn 額はドルの小数_2_桁_比は小数_1_桁_トークンは万の単位() {
        assert_eq!(dollars(0), "$0.00");
        assert_eq!(dollars(4_999), "$0.00");
        assert_eq!(dollars(5_000), "$0.01");
        assert_eq!(dollars(7_260_000), "$7.26");
        assert_eq!(dollars(1_234_567_890_000), "$1,234,567.89");
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1_000), "1,000");
        assert_eq!(count(50_000), "50,000");
        assert_eq!(percent(1, 8).as_deref(), Some("12.5%"));
        assert_eq!(percent(250_000, 7_260_000).as_deref(), Some("3.4%"));
        assert_eq!(percent(1, 2_000).as_deref(), Some("0.1%"));
        assert_eq!(percent(0, 5).as_deref(), Some("0.0%"));
        assert_eq!(percent(5, 5).as_deref(), Some("100.0%"));
        assert_eq!(percent(0, 0), None);
        assert_eq!(man(225_000), "22.5 万");
        assert_eq!(man(970_000), "97 万");
        assert_eq!(man(1_000_000), "100 万");
        assert_eq!(man(61_000), "6.1 万");
        assert_eq!(man(0), "0 万");
        assert_eq!(from_man(975_000), "97.5 万〜");
        assert_eq!(longest(0), None);
        assert_eq!(longest(230_000).as_deref(), Some("23 万"));
    }
}
