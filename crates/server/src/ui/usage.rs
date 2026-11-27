//! 費え（`GET /usage/`）: 席（アクター）ごとのトークンの数と額を、全体・役割ごと・課題ごとの表と、2 つの図（文脈の長さの段ごと・
//! 日ごとか時ごと。座標は [`super::usage_chart`]）で出す。一覧の額の列とアクターの頁の節「費え」も、ここの決め方（期間・数の字）を使う。
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

use castlist_client::types::Usage;

use super::params::{self, PageError, Pairs, Params, bad_request};
use super::time::{OFFSET, Time, local, stamp, time};
use super::timeline_layout;
use super::usage_chart::{self as chart, Axis};
use super::usage_summary::{self as summary, BUCKET_WIDTH, LAST_BUCKET, OVER_BUCKET, Summary};
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
pub(super) const MAX_DAYS: i64 = 31;

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

/// 期間を castlist に送れない訳。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Misfit {
    /// 終わりが始めより後でない。
    NotAfter,
    /// 揃えた後で [`MAX_DAYS`] 日を越える。
    TooLong,
    /// 終わりを打たずに、始めだけを先の時刻にした。
    StartsLater,
}

/// castlist に送る期間（時の境に揃えた始めと終わり）。終わりが無ければ今まで、始めが無ければ、揃えた終わりの
/// [`DEFAULT_DAYS`] 日前から（どちらも無ければ、直近 [`DEFAULT_DAYS`] 日）。一覧の額の列も、この決め方で一覧の期間を揃える。
pub(super) fn align(
    from: Option<OffsetDateTime>,
    to: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> Result<(OffsetDateTime, OffsetDateTime), Misfit> {
    let end = to.unwrap_or(now);
    // 終わりを打たずに、始めだけを先の時刻にした時は、打っていない「期間（まで）」の名を出さない。
    if to.is_none() && from.is_some_and(|from| from >= now) {
        return Err(Misfit::StartsLater);
    }
    let to = ceil_hour(end);
    // 始めが無ければ、揃えた終わりから、ちょうど DEFAULT_DAYS 日前。
    let start = from.unwrap_or(to - time::Duration::days(DEFAULT_DAYS));
    if end <= start {
        return Err(Misfit::NotAfter);
    }
    let from = floor_hour(start);
    if to - from > time::Duration::days(MAX_DAYS) {
        return Err(Misfit::TooLong);
    }
    Ok((from, to))
}

/// 費えの画面の期間（[`align`]）。送れない期間は 400。
fn window(
    from: Option<OffsetDateTime>,
    to: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> Result<(OffsetDateTime, OffsetDateTime), PageError> {
    align(from, to, now).map_err(|misfit| match misfit {
        Misfit::NotAfter => {
            bad_request("「期間（まで）」は、「期間（から）」より後にしてください。")
        }
        Misfit::TooLong => bad_request(format!(
            "期間は {MAX_DAYS} 日までです（時の境に揃えた後の長さ）。期間を狭めてください。"
        )),
        // 打っていない「期間（まで）」の名を出さない。
        Misfit::StartsLater => bad_request("「期間（から）」が、先の時刻です。"),
    })
}

/// 直近 [`MAX_DAYS`] 日（今を時の境に切り上げた時刻まで。アクターの頁の節「費え」）。
pub(super) fn recent(now: OffsetDateTime) -> (OffsetDateTime, OffsetDateTime) {
    let to = ceil_hour(now);
    (to - time::Duration::days(MAX_DAYS), to)
}

/// 揃えた期間の、castlist への条件。
pub(super) fn search(
    (from, to): (OffsetDateTime, OffsetDateTime),
    labels: Vec<String>,
    actor_id: Option<String>,
) -> UsageSearch {
    UsageSearch {
        from: DateTime::from_secs(from.unix_timestamp()),
        to: DateTime::from_secs(to.unix_timestamp()),
        labels,
        actor_id,
    }
}

/// 途中で止めたか（行が多すぎた・時間内に読み切れなかった）。
pub(super) fn cut(answer: &UsageAnswer) -> bool {
    answer.truncated.is_some()
}

/// 数を読めなかった訳（一覧の列とアクターの頁の節に、頁を落とさずに書く）。
pub(super) fn unreadable(failure: Failure) -> String {
    match failure {
        Failure::Rejected(message) => format!("castlist が引数を断りました: {message}"),
        // この操作に名指す物は無いので、古い castlist の 404 と同じ。
        Failure::NotFound => "404 が返りました".to_owned(),
        Failure::Unavailable(why) => why,
    }
}

// --- 数の字 ---

/// 数を 3 桁ごとに区切る（`12,345`）。
pub(super) fn count(n: i64) -> String {
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
pub(super) fn dollars(micro: i64) -> String {
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
pub(super) fn man(tokens: i64) -> String {
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
pub(super) fn longest(tokens: i64) -> Option<String> {
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
    charts: Charts,
}

fn tables(summary: Summary, charts: Charts) -> Tables {
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
        charts,
    }
}

/// castlist の行を、まとめる計算に渡す形にする。
pub(super) fn rows_of(usage: Vec<Usage>) -> Vec<summary::Row> {
    usage
        .into_iter()
        .map(|u| summary::Row {
            to_at: u.to_at.secs(),
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
        .collect()
}

/// castlist の答えを、まとめる。
fn summarize(
    rows: &[summary::Row],
    answer_actors: Vec<castlist_client::types::UsageActor>,
) -> Summary {
    let actors: Vec<summary::Actor> = answer_actors
        .into_iter()
        .map(|a| summary::Actor {
            id: a.id,
            labels: a.labels,
        })
        .collect();
    summary::summarize(rows, &actors)
}

// --- 図（SVG。座標は [`chart`] が出す） ---

/// 目盛り 1 つ（置き場と字）。
pub(super) struct TickView {
    pub at: u32,
    pub label: String,
}

/// 棒 1 本（と、段々の線の時ごとの、マウスを重ねる所）。
pub(super) struct BarView {
    pub class: &'static str,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub title: String,
}

/// 20 万の点線と、その字の置き場。
pub(super) struct LimitView {
    pub x1: u32,
    pub y1: u32,
    pub x2: u32,
    pub y2: u32,
    pub text_x: u32,
    pub text_y: u32,
    pub anchor: &'static str,
}

/// 縦の印（圧縮）。
pub(super) struct MarkView {
    pub x: u32,
    pub title: String,
}

/// 図 1 つ（`templates/macros.html` の `chart`）。
pub(super) struct ChartView {
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub base: u32,
    /// 縦の目盛りの字の右の端と、横の目盛りの字の下の線。
    pub tick_x: u32,
    pub label_y: u32,
    pub y_ticks: Vec<TickView>,
    pub x_ticks: Vec<TickView>,
    pub bars: Vec<BarView>,
    pub limit: Option<LimitView>,
    /// 段々の線の path。
    pub line: Option<String>,
    pub marks: Vec<MarkView>,
}

/// 20 万（段 8 の下の端）。
const LIMIT: i64 = OVER_BUCKET as i64 * BUCKET_WIDTH;
/// 横の目盛りの字の間の狭さの下限（px）。
const LABEL_PX: u32 = 48;

/// 目盛りと縁だけの図。
fn frame(label: String, axis: Axis, y_label: impl Fn(i64) -> String) -> ChartView {
    ChartView {
        label,
        width: chart::WIDTH,
        height: chart::HEIGHT,
        left: chart::LEFT,
        right: chart::LEFT + chart::PLOT_W,
        top: chart::TOP,
        base: chart::BASE,
        tick_x: chart::LEFT - 6,
        label_y: chart::BASE + 16,
        y_ticks: axis
            .ticks()
            .into_iter()
            .map(|(value, at)| TickView {
                at,
                label: y_label(value),
            })
            .collect(),
        x_ticks: Vec::new(),
        bars: Vec::new(),
        limit: None,
        line: None,
        marks: Vec::new(),
    }
}

/// 縦の目盛りの額の字（刻みに合わせて、`$2`・`$0.5`・`$0.05`）。
fn dollar_tick(value: i64, step: i64) -> String {
    if step % 1_000_000 == 0 {
        format!("${}", count(value / 1_000_000))
    } else if step % 100_000 == 0 {
        let dimes = value / 100_000;
        format!("${}.{}", count(dimes / 10), dimes % 10)
    } else {
        dollars(value)
    }
}

/// 縦の目盛りの文脈の長さの字（0 は単位を書かない）。
fn token_tick(value: i64) -> String {
    if value == 0 {
        "0".to_owned()
    } else {
        man(value)
    }
}

/// 段の範囲の字（`22.5 万〜25 万`。段 0 は `0〜2.5 万`、段 40 は `100 万〜`）。
fn bucket_range(bucket: i64) -> String {
    let low = bucket * BUCKET_WIDTH;
    if bucket >= i64::from(LAST_BUCKET) {
        return format!("{}〜", man(low));
    }
    let low = if bucket == 0 {
        "0".to_owned()
    } else {
        man(low)
    };
    format!("{low}〜{}", man((bucket + 1) * BUCKET_WIDTH))
}

/// 文脈の長さの段ごとの費え（本体の回）。本体の回が無ければ無い。
fn context_chart(rows: &[summary::Row]) -> Option<ChartView> {
    let buckets = summary::by_bucket(rows);
    if buckets
        .iter()
        .all(|(turns, cost)| *turns == 0 && *cost == 0)
    {
        return None;
    }
    let total: i64 = buckets.iter().map(|b| b.1).sum();
    let axis = chart::axis(buckets.iter().map(|b| b.1).max().unwrap_or(0), 10_000);
    let mut view = frame(
        format!(
            "文脈の長さの段ごとの費え（本体の回）。横は文脈の長さ（{}刻みの段）、縦は額。20 万に点線。本体の額の計 {}。",
            man(BUCKET_WIDTH),
            dollars(total)
        ),
        axis,
        |v| dollar_tick(v, axis.step),
    );
    let slots = chart::slots(buckets.len());
    // 横の目盛りは、段 4 つ（10 万）ごとに、段の下の端に。
    for (i, slot) in slots.iter().enumerate().step_by(4) {
        let bucket = i64::try_from(i).unwrap_or(0);
        view.x_ticks.push(TickView {
            at: slot.left,
            label: match bucket {
                0 => "0".to_owned(),
                b if b >= i64::from(LAST_BUCKET) => format!("{}〜", man(b * BUCKET_WIDTH)),
                b => man(b * BUCKET_WIDTH),
            },
        });
    }
    for (i, (slot, (turns, cost))) in slots.iter().zip(&buckets).enumerate() {
        if let Some(b) = chart::bar(axis, slot, 0, *cost) {
            view.bars.push(BarView {
                class: "bar main",
                x: b.x,
                y: b.y,
                w: b.w,
                h: b.h,
                title: format!(
                    "{}: {}（{} 回）",
                    bucket_range(i64::try_from(i).unwrap_or(0)),
                    dollars(*cost),
                    count(*turns)
                ),
            });
        }
    }
    let at = slots
        .get(usize::try_from(OVER_BUCKET).unwrap_or(0))
        .map_or(chart::LEFT, |s| s.left);
    view.limit = Some(LimitView {
        x1: at,
        y1: chart::TOP,
        x2: at,
        y2: chart::BASE,
        text_x: at + 4,
        text_y: chart::TOP + 10,
        anchor: "start",
    });
    Some(view)
}

/// 日ごと（期間が 1 日より長い時）か、時ごと（1 日まで）の費え。本体と手伝いを積む。図と、区切りの名（日・時）。
fn slot_chart(
    rows: &[summary::Row],
    from: OffsetDateTime,
    to: OffsetDateTime,
) -> (ChartView, &'static str) {
    let (start, end) = (from.unix_timestamp(), to.unix_timestamp());
    let hourly = end - start <= chart::DAY;
    let starts = if hourly {
        chart::hour_starts(start, end)
    } else {
        chart::day_starts(start, end, i64::from(OFFSET.whole_seconds()))
    };
    let sums = summary::by_slot(rows, &starts, end);
    let total: i64 = sums.iter().map(|(m, h)| m + h).sum();
    let unit = if hourly { "時" } else { "日" };
    let axis = chart::axis(sums.iter().map(|(m, h)| m + h).max().unwrap_or(0), 10_000);
    let mut view = frame(
        format!(
            "{unit}ごとの費え（日本時間）。本体と手伝いを積んだ棒。額の計 {}。",
            dollars(total)
        ),
        axis,
        |v| dollar_tick(v, axis.step),
    );
    let slots = chart::slots(starts.len());
    let every = chart::label_every(starts.len(), LABEL_PX);
    for (i, ((slot, at), (main, helper))) in slots.iter().zip(&starts).zip(&sums).enumerate() {
        let t = OffsetDateTime::from_unix_timestamp(*at)
            .unwrap_or(from)
            .to_offset(OFFSET);
        let date = format!("{:02}-{:02}", u8::from(t.month()), t.day());
        let clock = format!("{:02}:00", t.hour());
        let name = if hourly {
            format!("{date} {clock}")
        } else {
            date.clone()
        };
        if i % every == 0 {
            view.x_ticks.push(TickView {
                at: slot.mid(),
                label: if hourly { clock } else { date },
            });
        }
        let sum = dollars(main + helper);
        for (class, base, value, what) in [
            ("bar main", 0, *main, "本体"),
            ("bar helper", *main, *helper, "手伝い"),
        ] {
            if let Some(b) = chart::bar(axis, slot, base, value) {
                view.bars.push(BarView {
                    class,
                    x: b.x,
                    y: b.y,
                    w: b.w,
                    h: b.h,
                    title: format!(
                        "{name}: {what} {}（本体と手伝いの計 {sum}）",
                        dollars(value)
                    ),
                });
            }
        }
    }
    (view, unit)
}

/// 文脈の長さの段々の線（1 時間ごとの、本体の回のいちばん長い文脈）と、縦の印（時刻・字）。本体の回が無ければ無い。
/// 横は、記録の在る初めの時から終わりの時まで（印がその外なら、印まで延ばす）。
pub(super) fn line_chart(rows: &[summary::Row], marks: &[(i64, String)]) -> Option<ChartView> {
    let points = summary::context_by_hour(rows);
    let (first, last) = (points.first()?.0, points.last()?.0 + chart::HOUR);
    let start = marks.iter().map(|m| m.0).fold(first, i64::min);
    let end = marks.iter().map(|m| m.0).fold(last, i64::max);
    let max = points.iter().map(|p| p.1).max().unwrap_or(0);
    let axis = chart::axis(max.max(LIMIT), 10_000);
    let mut view = frame(
        format!(
            "文脈の長さの段々の線（1 時間ごとの、本体の回のいちばん長い文脈）。横は時刻（日本時間）、縦は文脈の長さ。20 万に点線。いちばん長い文脈 {}。",
            man(max)
        ),
        axis,
        token_tick,
    );
    view.x_ticks = timeline_layout::ticks(
        start * 1000,
        end * 1000,
        chart::LEFT,
        chart::PLOT_W,
        i64::from(OFFSET.whole_seconds()),
    )
    .into_iter()
    .map(|t| TickView {
        at: t.x,
        label: t.label,
    })
    .collect();
    let clock = |t: i64| {
        OffsetDateTime::from_unix_timestamp(t)
            .map(local)
            .unwrap_or_default()
    };
    for &(t, value) in &points {
        let (x0, x1) = (
            chart::x_at(start, end, t),
            chart::x_at(start, end, t + chart::HOUR),
        );
        let (from, to) = (clock(t), clock(t + chart::HOUR));
        view.bars.push(BarView {
            class: "hit",
            x: x0,
            y: chart::TOP,
            w: (x1 - x0).max(1),
            h: chart::PLOT_H,
            title: format!(
                "{}〜{}: {}",
                from.get(..16).unwrap_or(&from),
                to.get(11..16).unwrap_or(&to),
                man(value)
            ),
        });
    }
    view.line = Some(chart::steps(&points, start, end, axis));
    view.marks = marks
        .iter()
        .map(|(t, title)| MarkView {
            x: chart::x_at(start, end, *t),
            title: title.clone(),
        })
        .collect();
    let y = axis.y(LIMIT);
    view.limit = Some(LimitView {
        x1: chart::LEFT,
        y1: y,
        x2: chart::LEFT + chart::PLOT_W,
        y2: y,
        text_x: chart::LEFT + chart::PLOT_W - 4,
        text_y: y.saturating_sub(4),
        anchor: "end",
    });
    Some(view)
}

/// 費えの画面の図。
struct Charts {
    by_context: Option<ChartView>,
    by_slot: ChartView,
    /// 区切りの名（日・時）。
    unit: &'static str,
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
    let search = search((from, to), labels, None);
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
        let rows = rows_of(answer.rows);
        let tables = (!rows.is_empty()).then(|| {
            let (by_slot, unit) = slot_chart(&rows, from, to);
            let charts = Charts {
                by_context: context_chart(&rows),
                by_slot,
                unit,
            };
            tables(summarize(&rows, answer.actors), charts)
        });
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
