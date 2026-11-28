//! 使用量（`GET /usage/`）: 席（アクター）ごとのトークンの数と額を、全体・役割ごと・課題ごとの表と、2 つの図（文脈の長さの段ごと・
//! 日ごとか時ごと。座標は [`super::usage_chart`]）で出す。一覧の額の列とアクターの頁の節「使用量」も、ここの決め方（期間・数の字）を使う。
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

use super::i18n::{Text, filters};
use super::params::{self, LABEL_SHAPE, PageError, Pairs, Params, bad_request};
use super::time::{OFFSET, Time, local, stamp, time};
use super::timeline_layout;
use super::usage_chart::{self as chart, Axis};
use super::usage_labels::{self as labels, Choice, Choices, Picked};
use super::usage_summary::{
    self as summary, BUCKET_WIDTH, HEAT, LAST_BUCKET, Matrix, OVER_BUCKET, Summary,
};
use super::{CSS_HREF, Nav, error_page, render};
use crate::App;
use crate::upstream::Failure;
use crate::upstream::castlist::{ActorSearch, Cut, UsageAnswer, UsageSearch};

/// 使用量の道。
pub const PATH: &str = "/usage/";

/// 引数の名。`label` は何度でも。
const USAGE_PARAMS: &[&str] = &["label", "from", "to"];

/// 期間の欄の引数の名と、画面の見出しの字の鍵（読めない値の 400 の画面に書く）。
const PERIOD: [&str; 2] = ["from", "to"];
const PERIOD_FIELDS: [(&str, &str); 2] =
    [("from", "usage.filters.from"), ("to", "usage.filters.to")];

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

/// 使用量の画面の期間（[`align`]）。送れない期間は 400。
fn window(
    from: Option<OffsetDateTime>,
    to: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> Result<(OffsetDateTime, OffsetDateTime), PageError> {
    align(from, to, now).map_err(|misfit| match misfit {
        Misfit::NotAfter => bad_request(Text::new("usage.errors.notAfter")),
        Misfit::TooLong => {
            bad_request(Text::new("usage.errors.tooLong").with_num("days", MAX_DAYS))
        }
        // 打っていない「期間（まで）」の名を出さない。
        Misfit::StartsLater => bad_request(Text::new("usage.errors.startsLater")),
    })
}

/// 直近 [`MAX_DAYS`] 日（今を時の境に切り上げた時刻まで。アクターの頁の節「使用量」）。
pub(super) fn recent(now: OffsetDateTime) -> (OffsetDateTime, OffsetDateTime) {
    let to = ceil_hour(now);
    (to - time::Duration::days(MAX_DAYS), to)
}

/// 絞りの候補を作る席の条件: 期間に稼働した席を、新しく登場した順に 1 頁（[`CANDIDATE_LIMIT`] まで）。ラベルの絞りは付けない。
fn candidates((from, to): (OffsetDateTime, OffsetDateTime)) -> ActorSearch {
    ActorSearch {
        active_from: Some(DateTime::from_secs(from.unix_timestamp())),
        active_to: Some(DateTime::from_secs(to.unix_timestamp())),
        sort: "entered".to_owned(),
        order: "desc".to_owned(),
        limit: Some(CANDIDATE_LIMIT),
        ..ActorSearch::default()
    }
}

/// 絞りの候補を作る席の数の上限（castlist の 1 頁の上限）。越えた分の、古く登場した席のラベルは候補に出ない（log に 1 行）。
const CANDIDATE_LIMIT: i32 = 1000;

/// 絞りの候補を待つ長さ。候補は飾りで、使用量の数は ListUsage だけで決まるので、一覧の額の列と同じく短く切る。
const CANDIDATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

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
pub(super) fn unreadable(failure: Failure) -> Text {
    match failure {
        Failure::Rejected(message) => Text::new("errors.rejected")
            .with("service", "castlist")
            .with("detail", message),
        // この操作に名指す物は無いので、古い castlist の 404 と同じ。
        Failure::NotFound => Text::new("upstream.returned").with_num("status", 404),
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

/// 1 回あたりの額を、ドルの小数 3 桁にする（`$0.093`。四捨五入）。回が無ければ無い。
fn per_turn(micro: i64, turns: i64) -> Option<String> {
    if turns <= 0 {
        return None;
    }
    let turns = i128::from(turns);
    let mills = (i128::from(micro.max(0)) + turns * 500) / (turns * 1_000);
    let mills = i64::try_from(mills).unwrap_or(i64::MAX);
    Some(format!("${}.{:03}", count(mills / 1_000), mills % 1_000))
}

/// 比を % の小数 1 桁にする（`12.5%`。四捨五入）。元が 0 なら無い。
fn percent(part: i64, whole: i64) -> Option<String> {
    if whole <= 0 {
        return None;
    }
    let tenths = (i128::from(part.max(0)) * 2_000 + i128::from(whole)) / (i128::from(whole) * 2);
    Some(format!("{}.{}%", tenths / 10, tenths % 10))
}

/// トークンの数を「万」の単位にする（`22.5 万`・`97 万`。小数 1 桁に四捨五入し、`.0` は書かない）。値は言語で替わらない数の
/// 部分だけ（`man` は万の数、`k` は千の数。同じ桁まで）で、形（日本語「{{man}} 万」・英語「{{k}}k」）は訳の file の `common.tokens`。
pub(super) fn man(tokens: i64) -> Text {
    let tenths = tokens.max(0).saturating_add(500) / 1_000;
    let man = if tenths % 10 == 0 {
        format!("{}", tenths / 10)
    } else {
        format!("{}.{}", tenths / 10, tenths % 10)
    };
    Text::new("common.tokens")
        .with("man", man)
        .with("k", count(tenths))
}

/// 段の下の端を、「22.5 万〜」の形にする。
fn from_man(tokens: i64) -> Text {
    Text::new("common.tokensFrom").with_text("low", &man(tokens))
}

/// いちばん長い文脈。本体の回が無ければ（0 なら）無い。
pub(super) fn longest(tokens: i64) -> Option<Text> {
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
    /// 名にマウスを重ねた時の説明（名が無ければ無い）。
    about: Option<Text>,
    /// 説明を書いた名か（点線の下線を引く）。
    described: bool,
    name: Option<String>,
    seats: String,
    turns: String,
    cost: String,
    /// 本体の額の、全体の額に対する比。
    share: Option<String>,
    median: Option<Text>,
    p90: Option<Text>,
    max_context: Option<Text>,
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
    max_context: Option<Text>,
}

/// 行列の升 1 つ。
struct CellView {
    cost: String,
    /// 「5 回」。
    turns: Text,
    /// 地の色の class（`heat-1`〜`heat-3`。額が 0 なら空）。
    heat: &'static str,
    /// マウスを重ねた時の 1 回あたりの額。
    per_turn: Text,
}

/// 役割の名（castlist の席のラベル `tpm.role`）と、その説明の鍵（t-3jkq）。役割を足したら、ここと訳の file（ja・en）に足す。
const ROLE_ABOUT: &[(&str, &str)] = &[
    ("tpm", "usage.about.role.tpm"),
    ("software-engineer", "usage.about.role.softwareEngineer"),
    ("staff-engineer", "usage.about.role.staffEngineer"),
    ("tech-lead", "usage.about.role.techLead"),
    ("devops-engineer", "usage.about.role.devopsEngineer"),
    ("sustaining-engineer", "usage.about.role.sustainingEngineer"),
    ("auditor", "usage.about.role.auditor"),
];

/// 手伝い（subagent）の種類の名（castlist の回の `agent`）と、その説明の鍵（t-3jkq）。
const HELPER_ABOUT: &[(&str, &str)] = &[
    ("general-purpose", "usage.about.helper.generalPurpose"),
    ("Explore", "usage.about.helper.explore"),
    ("Plan", "usage.about.helper.plan"),
    ("web-fetch", "usage.about.helper.webFetch"),
    ("claude-code-guide", "usage.about.helper.claudeCodeGuide"),
    ("statusline-setup", "usage.about.helper.statuslineSetup"),
    ("audit-reader", "usage.about.helper.auditReader"),
];

/// 役割（`helper` が false）か手伝いの種類（true）の名の説明。名にマウスを重ねると出す（`title`）。
/// 書いていない名は「説明はまだありません」。
fn about(name: &str, helper: bool) -> Text {
    let table = if helper { HELPER_ABOUT } else { ROLE_ABOUT };
    let key = table
        .iter()
        .find(|(n, _)| *n == name)
        .map_or("usage.about.none", |(_, key)| key);
    Text::new(key)
}

/// 説明を書いた名か（[`about`] が「説明はまだありません」でない）。
fn described(name: &str, helper: bool) -> bool {
    about(name, helper).key() != "usage.about.none"
}

/// 行列の行。上の組は役割（`role`。ラベルが無ければ無い）、下の組は手伝い（`helper` に「手伝い: 種類」）。
struct MatrixRowView {
    /// 名にマウスを重ねた時の説明（名が無ければ無い）。
    about: Option<Text>,
    /// 説明を書いた名か（点線の下線を引く。書いていない名は「説明はまだありません」だけ）。
    described: bool,
    helper: Option<Text>,
    role: Option<String>,
    /// 席の最新の effort（1 種類なら名だけ、2 種類以上なら「medium（76 席）」の形を、席の多い順に）。
    efforts: Vec<Text>,
    cells: Vec<Option<CellView>>,
    total: String,
}

/// 行列の下の行の、列ごとの値（最後は計の列）。
struct FootView {
    costs: Vec<String>,
    shares: Vec<Option<String>>,
    per_turns: Vec<Option<String>>,
}

/// model と役割ごとの行列。
struct MatrixView {
    models: Vec<String>,
    rows: Vec<MatrixRowView>,
    foot: FootView,
    /// 地の色の凡例（段の境の数を値で）と、段ごとの色見本の字（濃い順）。
    legend: Text,
    keys: Vec<(&'static str, Text)>,
    /// effort を引けなかった・一部だけ引けた時の注。
    effort_note: Option<Text>,
}

/// 席の最新の effort を引けたか（絞りの候補と同じ `ListActors` の答えから）。
pub(super) enum Efforts {
    /// 引けた。`partial` は、席が 1 頁の上限を越え、新しく登場した席の分だけの時。
    Read {
        efforts: std::collections::HashMap<String, String>,
        partial: bool,
    },
    /// 引けなかった（断られた・時間内に来なかった）。
    Unread,
}

fn matrix(matrix: Matrix, efforts: &Efforts) -> MatrixView {
    let whole = matrix.total.cost;
    let cell = |c: summary::Cell| CellView {
        cost: dollars(c.cost),
        turns: Text::new("usage.models.turns")
            .with("turns", count(c.turns))
            .with_num("count", c.turns),
        heat: match summary::heat(c.cost, whole) {
            Some(3) => "heat-3",
            Some(2) => "heat-2",
            Some(_) => "heat-1",
            None => "",
        },
        per_turn: Text::new("usage.models.perTurn").with(
            "cost",
            per_turn(c.cost, c.turns).unwrap_or_else(|| "—".to_owned()),
        ),
    };
    let row = |r: summary::MatrixRow, helper: bool| MatrixRowView {
        // 1 種類で、行の席がみな数えられた時だけ名だけにする（effort の分からない席があれば、数を添える）。
        efforts: match r.efforts.as_slice() {
            [(effort, n)] if *n == r.seats => vec![Text::plain(effort.clone())],
            many => many
                .iter()
                .map(|(effort, n)| {
                    Text::new("usage.models.effortSeats")
                        .with("effort", effort)
                        .with_num("count", *n)
                })
                .collect(),
        },
        about: r.name.as_deref().map(|name| about(name, helper)),
        described: r
            .name
            .as_deref()
            .is_some_and(|name| described(name, helper)),
        helper: helper.then(|| {
            Text::new("usage.models.helper").with("name", r.name.as_deref().unwrap_or_default())
        }),
        role: if helper { None } else { r.name },
        cells: r.cells.into_iter().map(|c| c.map(cell)).collect(),
        total: dollars(r.total.cost),
    };
    let columns: Vec<summary::Cell> = matrix
        .models
        .iter()
        .map(|(_, c)| *c)
        .chain([matrix.total])
        .collect();
    MatrixView {
        foot: FootView {
            costs: columns.iter().map(|c| dollars(c.cost)).collect(),
            shares: columns.iter().map(|c| percent(c.cost, whole)).collect(),
            per_turns: columns.iter().map(|c| per_turn(c.cost, c.turns)).collect(),
        },
        models: matrix.models.into_iter().map(|(m, _)| m).collect(),
        rows: matrix
            .roles
            .into_iter()
            .map(|r| row(r, false))
            .chain(matrix.helpers.into_iter().map(|r| row(r, true)))
            .collect(),
        legend: Text::new("usage.models.legend")
            .with_num("high", HEAT[0])
            .with_num("low", HEAT[1]),
        keys: vec![
            (
                "heat-3",
                Text::new("usage.models.key.high").with_num("high", HEAT[0]),
            ),
            (
                "heat-2",
                Text::new("usage.models.key.mid")
                    .with_num("high", HEAT[0])
                    .with_num("low", HEAT[1]),
            ),
            (
                "heat-1",
                Text::new("usage.models.key.low").with_num("low", HEAT[1]),
            ),
        ],
        effort_note: match efforts {
            Efforts::Read { partial: false, .. } => None,
            Efforts::Read { partial: true, .. } => Some(
                Text::new("usage.models.effortPartial")
                    .with("seats", count(i64::from(CANDIDATE_LIMIT))),
            ),
            Efforts::Unread => Some(Text::new("usage.models.effortUnread")),
        },
    }
}

struct Tables {
    total: TotalView,
    matrix: MatrixView,
    roles: Vec<RoleView>,
    subjects: Vec<SubjectView>,
    charts: Charts,
}

fn tables(summary: Summary, matrix: MatrixView, charts: Charts) -> Tables {
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
        matrix,
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
                about: r.name.as_deref().map(|name| about(name, false)),
                described: r.name.as_deref().is_some_and(|name| described(name, false)),
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
            model: u.model,
            agent: u.agent,
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
fn actors_of(answer_actors: Vec<castlist_client::types::UsageActor>) -> Vec<summary::Actor> {
    answer_actors
        .into_iter()
        .map(|a| summary::Actor {
            id: a.id,
            labels: a.labels,
        })
        .collect()
}

// --- 図（SVG。座標は [`chart`] が出す） ---

/// 目盛り 1 つ（置き場と字）。
pub(super) struct TickView {
    pub at: u32,
    pub label: Text,
}

/// 棒 1 本（と、段々の線の時ごとの、マウスを重ねる所）。
pub(super) struct BarView {
    pub class: &'static str,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub title: Text,
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
    pub title: Text,
}

/// 図 1 つ（`templates/macros.html` の `chart`）。
pub(super) struct ChartView {
    /// 図の名（`aria-label`）。
    pub label: Text,
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
fn frame(label: Text, axis: Axis, y_label: impl Fn(i64) -> Text) -> ChartView {
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
fn dollar_tick(value: i64, step: i64) -> Text {
    Text::plain(if step % 1_000_000 == 0 {
        format!("${}", count(value / 1_000_000))
    } else if step % 100_000 == 0 {
        let dimes = value / 100_000;
        format!("${}.{}", count(dimes / 10), dimes % 10)
    } else {
        dollars(value)
    })
}

/// 縦の目盛りの文脈の長さの字（0 は単位を書かない）。
fn token_tick(value: i64) -> Text {
    if value == 0 {
        Text::plain("0")
    } else {
        man(value)
    }
}

/// 段の範囲の字（`22.5 万〜25 万`。段 0 は `0〜2.5 万`、段 40 は `100 万〜`）。
fn bucket_range(bucket: i64) -> Text {
    let low = bucket * BUCKET_WIDTH;
    if bucket >= i64::from(LAST_BUCKET) {
        return from_man(low);
    }
    let low = if bucket == 0 {
        Text::plain("0")
    } else {
        man(low)
    };
    Text::new("common.tokenRange")
        .with_text("low", &low)
        .with_text("high", &man((bucket + 1) * BUCKET_WIDTH))
}

/// 文脈の長さの段ごとの額（本体の回）。本体の回が無ければ無い。
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
        Text::new("usage.byContext.chartLabel")
            .with_text("width", &man(BUCKET_WIDTH))
            .with("total", dollars(total)),
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
                0 => Text::plain("0"),
                b if b >= i64::from(LAST_BUCKET) => from_man(b * BUCKET_WIDTH),
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
                title: Text::new("usage.byContext.bar")
                    .with_text("range", &bucket_range(i64::try_from(i).unwrap_or(0)))
                    .with("cost", dollars(*cost))
                    .with("turns", count(*turns))
                    .with_num("count", *turns),
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

/// 日ごと（期間が 1 日より長い時）か、時ごと（1 日まで）の額。本体と手伝いを積む。図と、時ごとか（区切りの名の鍵を選ぶ）。
fn slot_chart(
    rows: &[summary::Row],
    from: OffsetDateTime,
    to: OffsetDateTime,
) -> (ChartView, bool, Vec<SlotRow>) {
    let (start, end) = (from.unix_timestamp(), to.unix_timestamp());
    let mut table = Vec::new();
    let hourly = end - start <= chart::DAY;
    let starts = if hourly {
        chart::hour_starts(start, end)
    } else {
        chart::day_starts(start, end, i64::from(OFFSET.whole_seconds()))
    };
    let sums = summary::by_slot(rows, &starts, end);
    let total: i64 = sums.iter().map(|(m, h)| m + h).sum();
    let axis = chart::axis(sums.iter().map(|(m, h)| m + h).max().unwrap_or(0), 10_000);
    let label = if hourly {
        "usage.bySlot.chartLabel.hour"
    } else {
        "usage.bySlot.chartLabel.day"
    };
    let mut view = frame(Text::new(label).with("total", dollars(total)), axis, |v| {
        dollar_tick(v, axis.step)
    });
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
                label: Text::plain(if hourly { clock } else { date }),
            });
        }
        let sum = dollars(main + helper);
        // 図の外の字の表（読み上げのため。図の棒の <title> は読み上げに出ない）。額の在る区切りだけ。
        if main + helper > 0 {
            table.push(SlotRow {
                name: name.clone(),
                main: dollars(*main),
                helper: dollars(*helper),
                total: sum.clone(),
            });
        }
        // 手伝いの棒は、本体の棒の上の端から積む（本体の棒が 1 px に満たず 1 px で描いた時に、それを覆わない）。
        let main_bar = chart::bar(axis, slot, 0, *main);
        let helper_bar = chart::stacked(axis, slot, main_bar, *main, *helper);
        for (class, value, key, bar) in [
            ("bar main", *main, "usage.bySlot.bar.main", main_bar),
            ("bar helper", *helper, "usage.bySlot.bar.helper", helper_bar),
        ] {
            if let Some(b) = bar {
                view.bars.push(BarView {
                    class,
                    x: b.x,
                    y: b.y,
                    w: b.w,
                    h: b.h,
                    title: Text::new(key)
                        .with("name", &name)
                        .with("cost", dollars(value))
                        .with("sum", &sum),
                });
            }
        }
    }
    (view, hourly, table)
}

/// 区切り（日・時）ごとの額の、字の表の 1 行。
pub(super) struct SlotRow {
    name: String,
    main: String,
    helper: String,
    total: String,
}

/// 文脈の長さの段々の線（1 時間ごとの、本体の回のいちばん長い文脈）と、縦の印（時刻・字）。本体の回が無ければ無い。
/// 横は、記録の在る初めの時から終わりの時まで（印がその外なら、印まで延ばす）。
pub(super) fn line_chart(rows: &[summary::Row], marks: &[(i64, Text)]) -> Option<ChartView> {
    let points = summary::context_by_hour(rows);
    let (first, last) = (points.first()?.0, points.last()?.0 + chart::HOUR);
    // 横は、数の記録の在る時の内だけ（その外の印は描かない。取り込みの始まる前の印で、線が端に縮まないように）。
    let (start, end) = (first, last);
    let max = points.iter().map(|p| p.1).max().unwrap_or(0);
    let axis = chart::axis(max.max(LIMIT), 10_000);
    let mut view = frame(
        Text::new("usage.line.chartLabel").with_text("max", &man(max)),
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
        label: Text::plain(t.label),
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
            title: Text::new("usage.line.hit")
                .with("from", from.get(..16).unwrap_or(&from))
                .with("to", to.get(11..16).unwrap_or(&to))
                .with_text("value", &man(value)),
        });
    }
    view.line = Some(chart::steps(&points, start, end, axis));
    view.marks = marks
        .iter()
        .filter(|(t, _)| (start..=end).contains(t))
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

/// 使用量の画面の図。
struct Charts {
    by_context: Option<ChartView>,
    by_slot: ChartView,
    /// 日ごと・時ごとの額の字の表（畳んで置く）。
    slot_rows: Vec<SlotRow>,
    /// 区切りの見出し・注・表の名・表の列の名（時ごとか日ごとかで鍵が替わる）。
    slot_heading: Text,
    slot_legend: Text,
    slot_table: Text,
    slot_col: Text,
}

/// 絞りの form に、今の値を入れて出す。ラベルは、役割と課題の一覧と「ほかのラベル」の欄に振り分ける（[`labels::pick`]）。
#[derive(Default)]
struct Form {
    picked: Picked,
    from: String,
    to: String,
}

impl Form {
    fn new(pairs: &Pairs) -> Self {
        let mut form = Self::default();
        let mut given = Vec::new();
        for (name, value) in pairs {
            match name.as_str() {
                "label" => given.push(value.clone()),
                "from" => form.from = params::to_local(value),
                "to" => form.to = params::to_local(value),
                _ => {}
            }
        }
        form.picked = labels::pick(&given);
        form
    }
}

/// 一覧と欄の候補 1 つ（値・字・選んであるか）。
struct ChoiceView {
    value: String,
    /// 一覧では「名（N 席）」、欄の候補では「鍵:値（N 席）」。席の数が無ければ（候補に無かった今の値）名だけ。
    text: Text,
    selected: bool,
}

/// 絞りの候補（役割と課題の一覧・「ほかのラベル」の欄の候補）。
struct ChoicesView {
    roles: Vec<ChoiceView>,
    subjects: Vec<ChoiceView>,
    others: Vec<ChoiceView>,
}

impl ChoicesView {
    fn new(mut choices: Choices, picked: &Picked) -> Self {
        choices.keep(picked);
        let listed = |list: Vec<Choice>, chosen: &Option<String>| -> Vec<ChoiceView> {
            list.into_iter()
                .map(|c| ChoiceView {
                    selected: chosen.as_deref() == Some(c.value.as_str()),
                    text: match c.seats {
                        Some(n) => Text::new("usage.filters.choice")
                            .with("name", &c.name)
                            .with_num("count", n),
                        None => Text::plain(c.name),
                    },
                    value: c.value,
                })
                .collect()
        };
        Self {
            roles: listed(choices.roles, &picked.role),
            subjects: listed(choices.subjects, &picked.subject),
            others: choices
                .others
                .into_iter()
                // 字にも値を書く（Firefox は、候補に値の代わりに字を出す）。
                .map(|c| ChoiceView {
                    text: Text::new("usage.filters.choice")
                        .with("name", &c.value)
                        .with_num("count", c.seats.unwrap_or(0)),
                    value: c.value,
                    selected: false,
                })
                .collect(),
        }
    }
}

#[derive(Template)]
#[template(path = "usage.html")]
struct UsagePage<'a> {
    css: &'static str,
    nav: &'a Nav,
    form: Form,
    /// 絞りの候補（表示と同じ期間に稼働した席のラベル）。
    choices: ChoicesView,
    /// 期間をすぐ選ぶ link（名と URL）。
    quick: Vec<(Text, String)>,
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
    /// 段の幅（「2.5 万」）を入れた、文脈の長さの図の注と、表の注。
    context_legend: Text,
    bands: Text,
    as_of: Time,
}

fn time_of(t: OffsetDateTime) -> Time {
    Time {
        iso: stamp(t),
        local: local(t),
    }
}

/// `GET /usage/?label=…&from=…&to=…`: 使用量。空の値は付けなかったのと同じ（form は空の欄も送る）。
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
        .map(|v| params::pair("label", v, LABEL_SHAPE).map(|()| v.to_owned()))
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
    // 絞りの候補は、同じ期間に稼働した席から（ラベルの絞りは付けない。絞っても候補が減らないように）。使用量の行と並べて聞く。
    let active = candidates((from, to));
    let (answer, active) = tokio::join!(
        app.castlist.usage(&search),
        tokio::time::timeout(CANDIDATE_TIMEOUT, app.castlist.actors(&active))
    );
    // castlist が引数を断った時（callboard の確かめを通った物なので、ふつうは起きない）は、castlist の訳を、そのまま書く。
    let answer = answer.map_err(|e| params::failed("castlist", e))?;
    // 候補を引けなくても（断られた・期限を過ぎた）、使用量の数は出す（一覧は今の値だけ、欄は候補無し）。
    // 同じ答えから、行列の役割の行の席の最新の effort も引く（castlist への問いは増やさない）。
    let (choices, efforts) = match active {
        Ok(Ok(output)) => {
            let partial = output.next_token.is_some();
            if partial {
                tracing::warn!(
                    "使用量の絞りの候補と effort は、新しく登場した {CANDIDATE_LIMIT} 席の分だけです（期間に稼働した席が、それより多い）"
                );
            }
            let efforts = output
                .actors
                .iter()
                .filter_map(|a| a.effort.clone().map(|e| (a.id.clone(), e)))
                .collect();
            (
                labels::choices(output.actors.iter().map(|a| &a.labels)),
                Efforts::Read { efforts, partial },
            )
        }
        Ok(Err(e)) => {
            tracing::warn!("使用量の絞りの候補を引けません: {e:?}");
            (Choices::default(), Efforts::Unread)
        }
        Err(_) => {
            tracing::warn!(
                "使用量の絞りの候補を、{} ms の内に引けませんでした",
                CANDIDATE_TIMEOUT.as_millis()
            );
            (Choices::default(), Efforts::Unread)
        }
    };
    let choices = ChoicesView::new(choices, &form.picked);
    tokio::task::spawn_blocking(move || {
        let no_efforts = std::collections::HashMap::new();
        let as_of = time(&answer.as_of);
        let too_many = answer.truncated == Some(Cut::Pages);
        let too_slow = answer.truncated == Some(Cut::Time);
        let read = count(i64::try_from(answer.rows.len()).unwrap_or(i64::MAX));
        let rows = rows_of(answer.rows);
        let tables = (!rows.is_empty()).then(|| {
            let (by_slot, hourly, slot_rows) = slot_chart(&rows, from, to);
            let pick = |hour, day| Text::new(if hourly { hour } else { day });
            let charts = Charts {
                by_context: context_chart(&rows),
                by_slot,
                slot_rows,
                slot_heading: pick("usage.bySlot.heading.hour", "usage.bySlot.heading.day"),
                slot_legend: pick("usage.bySlot.legend.hour", "usage.bySlot.legend.day"),
                slot_table: pick("usage.bySlot.table.hour", "usage.bySlot.table.day"),
                slot_col: pick("usage.bySlot.col.hour", "usage.bySlot.col.day"),
            };
            let actors = actors_of(answer.actors);
            let by_model = summary::by_model(
                &rows,
                &actors,
                match &efforts {
                    Efforts::Read { efforts, .. } => efforts,
                    Efforts::Unread => &no_efforts,
                },
            );
            tables(
                summary::summarize(&rows, &actors),
                matrix(by_model, &efforts),
                charts,
            )
        });
        let page = UsagePage {
            css: &CSS_HREF,
            nav: &app.nav,
            form,
            choices,
            quick,
            from: time_of(from),
            to: time_of(to),
            defaulted,
            days: DEFAULT_DAYS,
            too_many,
            too_slow,
            read,
            tables,
            context_legend: Text::new("usage.byContext.legend")
                .with_text("width", &man(BUCKET_WIDTH)),
            bands: Text::new("usage.notes.bands").with_text("width", &man(BUCKET_WIDTH)),
            as_of,
        };
        render(StatusCode::OK, &page)
    })
    .await
    .map_err(|e| {
        tracing::error!("使用量を表示できません: {e}");
        PageError(
            StatusCode::INTERNAL_SERVER_ERROR,
            Text::new("usage.errors.render"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(iso: &str) -> OffsetDateTime {
        OffsetDateTime::parse(iso, &Rfc3339).unwrap()
    }

    /// 説明を書く名（t-3jkq の confirm-1 の 14 名）。役割 7 つと手伝い 7 つ。
    const ROLES_WITH_ABOUT: [&str; 7] = [
        "tpm",
        "software-engineer",
        "staff-engineer",
        "tech-lead",
        "devops-engineer",
        "sustaining-engineer",
        "auditor",
    ];
    const HELPERS_WITH_ABOUT: [&str; 7] = [
        "general-purpose",
        "Explore",
        "Plan",
        "web-fetch",
        "claude-code-guide",
        "statusline-setup",
        "audit-reader",
    ];

    #[test]
    fn 役割と手伝いの説明は_名から引き_書いていない名は_説明はまだありません() {
        assert_eq!(
            about("software-engineer", false).key(),
            "usage.about.role.softwareEngineer"
        );
        assert_eq!(
            about("software-engineer", false),
            "課題 1 つを受け持ち、調べ・提案・計画・実装・試験をする。席がいちばん多く、長い文脈で実装と試験を回すので、額が大きくなりやすい。"
        );
        assert_eq!(about("Explore", true).key(), "usage.about.helper.explore");
        // 書いていない名・大文字と小文字の違う名・役割の名を手伝いとして引いた時は、説明が無い。
        assert_eq!(
            about("technical-program-manager", false).key(),
            "usage.about.none"
        );
        assert_eq!(about("explore", true).key(), "usage.about.none");
        assert_eq!(about("tpm", true).key(), "usage.about.none");
        assert_eq!(about("Explore", false).key(), "usage.about.none");
        assert_eq!(about("tpm", true), "説明はまだありません");
        // 点線の下線は、説明を書いた名だけ。
        assert!(described("software-engineer", false));
        assert!(described("Explore", true));
        assert!(!described("technical-program-manager", false));
        assert!(!described("tpm", true));
    }

    #[test]
    fn 説明を書く_14_名には_日本語と英語の説明がみなある() {
        let ja = super::super::i18n::table(super::super::i18n::JA_JSON);
        let en = super::super::i18n::table(super::super::i18n::EN_JSON);
        let names = ROLES_WITH_ABOUT
            .iter()
            .map(|n| (n, false))
            .chain(HELPERS_WITH_ABOUT.iter().map(|n| (n, true)));
        for (name, helper) in names {
            let key = about(name, helper).key();
            assert_ne!(key, "usage.about.none", "{name} の説明が無い");
            assert!(ja.contains_key(key), "{name} の日本語の説明 {key} が無い");
            assert!(en.contains_key(key), "{name} の英語の説明 {key} が無い");
        }
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
        assert_eq!(
            longest(230_000).map(|t| t.to_string()).as_deref(),
            Some("23 万")
        );
        // 値は数の部分だけ（英語の形は訳の file）。千の数は同じ桁まで。
        assert_eq!(
            man(225_000).values(),
            serde_json::json!({"man": "22.5", "k": "225"})
        );
        assert_eq!(man(1_000_000).values()["k"], "1,000");
        assert_eq!(
            from_man(975_000).values()["low"],
            serde_json::json!({"$t": "common.tokens", "values": {"man": "97.5", "k": "975"}})
        );
    }
}
