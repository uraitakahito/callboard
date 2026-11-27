//! アクターの一覧（`GET /actors/`）。castlist の画面（castlist の `/`）から移した物。
//!
//! 絞り込みは、castlist の API の `ListActors`（監査の検索）と同じ条件を、画面の引数の名（snake_case。`activeFrom` は
//! `active_from`、`launchedBy` は `launched_by`、`maxResults` は `limit`、`nextToken` は `after`）で URL に置く。
//! 引数は画面が確かめ（誤りは、引数の名を書いた 400 の画面。castlist の画面と同じ字）、確かめた物を API に渡す。
use std::collections::BTreeMap;
use std::sync::Arc;

use askama::Template;
use aws_smithy_types::DateTime;
use aws_smithy_types::date_time::Format;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use castlist_client::types::ActorRow;
use time::OffsetDateTime;

use super::i18n::filters;
use super::params::{self, LIST_PERIOD, PageError, Pairs, Params, bad_request};
use super::profile::event_name;
use super::time::{Time, local, stamp, time};
use super::usage::{self, MAX_DAYS, Misfit};
use super::usage_summary::{self as summary, Seat};
use super::{CSS_HREF, Nav, error_page, kind_name, render};
use crate::App;
use crate::upstream::Failure;
use crate::upstream::castlist::ActorSearch;

/// 一覧の道。
pub const PATH: &str = "/actors/";

/// 条件が無い時の一覧: この日数の間に稼働した者（`active_from` を今からこの日数前にする）。
const DEFAULT_DAYS: i64 = 7;

/// 一覧の 1 頁の数（引数の `limit` が無い時）と、上限。
const PAGE_SIZE: &str = "100";
const MAX_LIMIT: i32 = 1000;

/// 一覧の引数の名（ListActors と同じ条件の、画面の名。snake_case）。
const ACTOR_PARAMS: &[&str] = &[
    "label",
    "binding",
    "at",
    "active_from",
    "active_to",
    "kind",
    "model",
    "effort",
    "active",
    "launched_by",
    "skewed",
    "sort",
    "order",
    "limit",
    "after",
];

/// 一覧の頁の印（`after`）の頭（castlist の ListActors の nextToken の形: 種類・sort・order・並べる値・seq を改行で繋いだ 16 進）。
const TOKEN_KIND: &str = "actors";

/// 16 進を戻し、改行で分ける。崩れていれば（空の物も）None（castlist の `token::decode` と同じ）。
fn decode(token: &str) -> Option<Vec<String>> {
    if token.is_empty()
        || !token.len().is_multiple_of(2)
        || !token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    let bytes = (0..token.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&token[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;
    Some(text.split('\n').map(str::to_owned).collect())
}

fn broken_after() -> PageError {
    bad_request("after が読めない（前の答えの next を、そのまま渡す）")
}

/// 一覧の引数を確かめ、castlist の API（ListActors）に渡す条件にする。誤りの字は、castlist の画面と同じ。
fn actor_search(p: &Params) -> Result<ActorSearch, PageError> {
    let api_time = |name: &str| -> Result<Option<DateTime>, PageError> {
        Ok(p.time(name)?
            .and_then(|t| DateTime::from_str(&t, Format::DateTime).ok()))
    };
    let labels = p
        .all("label")
        .map(|v| params::pair("label", v, "鍵:値").map(|()| v.to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    let at = api_time("at")?;
    let binding = match p.one("binding") {
        Some(v) => {
            params::pair("binding", v, "種類:値")?;
            Some(v.to_owned())
        }
        None if at.is_some() => {
            return Err(bad_request(
                "at は binding と一緒に付ける（居た間で探すなら active_from・active_to）",
            ));
        }
        None => None,
    };
    let sort = p
        .choice("sort", &["entered", "exited"])?
        .unwrap_or("entered");
    let order = p.choice("order", &["asc", "desc"])?.unwrap_or("asc");
    let after = match p.one("after") {
        None => None,
        Some(v) => {
            let parts = decode(v).ok_or_else(broken_after)?;
            let [kind, token_sort, token_order, _value, seq] = &parts[..] else {
                return Err(bad_request(
                    "after が読めない（actors の検索の next ではない）",
                ));
            };
            if kind != TOKEN_KIND {
                return Err(bad_request(
                    "after が読めない（actors の検索の next ではない）",
                ));
            }
            if token_sort != sort || token_order != order {
                return Err(bad_request(
                    "after は、同じ sort と order の答えの next を渡す",
                ));
            }
            seq.parse::<i64>().map_err(|_| broken_after())?;
            Some(v.to_owned())
        }
    };
    let limit = match p.one("limit") {
        None => None,
        Some(v) => Some(
            v.parse()
                .ok()
                .filter(|n| (1..=MAX_LIMIT).contains(n))
                .ok_or_else(|| bad_request(format!("limit は 1 から {MAX_LIMIT} までの数")))?,
        ),
    };
    Ok(ActorSearch {
        labels,
        binding,
        at,
        active_from: api_time("active_from")?,
        active_to: api_time("active_to")?,
        kind: p.choice("kind", &["ai", "human"])?.map(str::to_owned),
        model: p.name("model")?,
        effort: p.name("effort")?,
        active: p.flag("active")?,
        launched_by: p.value("launched_by")?,
        skewed: p.only_true("skewed")?,
        sort: sort.to_owned(),
        order: order.to_owned(),
        limit,
        after,
    })
}

/// 索引で引ける条件があるか（castlist の検索が、条件の無い問いを断るのと同じ決め方。`kind` と `active=false` は数えない）。
fn narrowed(search: &ActorSearch) -> bool {
    !search.labels.is_empty()
        || search.binding.is_some()
        || search.active_from.is_some()
        || search.active_to.is_some()
        || search.active == Some(true)
        || search.model.is_some()
        || search.effort.is_some()
        || search.launched_by.is_some()
        || search.skewed
}

/// 一覧の行。
struct Row {
    href: String,
    name: String,
    kind: &'static str,
    present: bool,
    /// 今の状態を決めた出来事が、推しの幕引き（「停止（推し）」と書く）。
    inferred: bool,
    /// 稼働を替える出来事の最新（日本語の名・id）。
    presence: Option<(String, String)>,
    key: Option<String>,
    role: Option<String>,
    /// 最新の model と effort。
    setting: Option<(String, String)>,
    first_entered: Option<Time>,
    last_exited: Option<Time>,
    skewed: bool,
    /// 一覧の期間の、席の額（本体と手伝いの計）と、いちばん長い文脈（本体）。数が無ければ無い（「—」）。
    cost: Option<String>,
    context: Option<String>,
}

fn row(item: &ActorRow, seats: &BTreeMap<String, Seat>) -> Row {
    let seat = seats.get(&item.id);
    Row {
        href: format!("/actors/{}", item.id),
        name: item.name.clone(),
        kind: kind_name(&item.kind),
        present: item.present,
        inferred: item.inferred,
        presence: item
            .presence
            .as_deref()
            .map(|p| (event_name(p).to_owned(), p.to_owned())),
        key: item.labels.get("tpm.key").cloned(),
        role: item.labels.get("tpm.role").cloned(),
        setting: item.model.clone().zip(item.effort.clone()),
        first_entered: item.first_entered_at.as_ref().map(time),
        last_exited: item.last_exited_at.as_ref().map(time),
        skewed: item.skewed.unwrap_or(false),
        cost: seat.map(|s| usage::dollars(s.cost)),
        context: seat.and_then(|s| usage::longest(s.max_context)),
    }
}

/// 一覧の額の列: 数を聞いた期間（時の境に揃えた物）と、席ごとの数か、出さない訳。
struct Costs {
    window: Option<(Time, Time)>,
    seats: BTreeMap<String, Seat>,
    /// 列を空けた訳（表の下の注）。
    note: Option<String>,
}

fn time_of(t: OffsetDateTime) -> Time {
    Time {
        iso: stamp(t),
        local: local(t),
    }
}

/// 一覧の期間（castlist に渡す `active_from`・`active_to`）を時の境に揃え、席ごとの額を聞く（ListUsage を 1 度。頁はみな引く）。
/// 聞けない・期間が長すぎる時は、列を空ける（一覧は出す）。
/// 一覧の、使用量を待つ長さ（使用量の画面とアクターの頁は、castlist の口の期限のまま）。
const LIST_COST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

async fn costs(app: &App, search: &ActorSearch, now: OffsetDateTime) -> Costs {
    let at = |t: Option<DateTime>| {
        t.and_then(|t| OffsetDateTime::from_unix_timestamp_nanos(t.as_nanos()).ok())
    };
    let empty = |note: Option<String>| Costs {
        window: None,
        seats: BTreeMap::new(),
        note,
    };
    let window = match usage::align(at(search.active_from), at(search.active_to), now) {
        Ok(window) => window,
        // 終わりが始めより後でない・始めだけが先の時刻の期間（一覧にも、だれも出ない）。
        Err(Misfit::NotAfter | Misfit::StartsLater) => return empty(None),
        Err(Misfit::TooLong) => {
            return empty(Some(format!(
                "一覧の期間が {MAX_DAYS} 日を越えるので、額といちばん長い文脈は出しません（数は {MAX_DAYS} 日ずつ読みます）。"
            )));
        }
    };
    // 一覧は人がいちばん開く画面なので、使用量は短い期限（LIST_COST_TIMEOUT）で待ち、過ぎたら列を空ける（一覧を遅らせない）。
    let found = tokio::time::timeout(
        LIST_COST_TIMEOUT,
        app.castlist
            .usage(&usage::search(window, search.labels.clone(), None)),
    )
    .await;
    let shown = Some((time_of(window.0), time_of(window.1)));
    let Ok(found) = found else {
        return Costs {
            window: shown,
            seats: BTreeMap::new(),
            note: Some(
                "額といちばん長い文脈を、時間内に読めませんでした。一覧は、そのまま出しています。"
                    .to_owned(),
            ),
        };
    };
    match found {
        Ok(answer) if usage::cut(&answer) => Costs {
            window: shown,
            seats: BTreeMap::new(),
            note: Some(
                "記録が多すぎて、額といちばん長い文脈を出せません。期間を狭めてください。"
                    .to_owned(),
            ),
        },
        Ok(answer) => Costs {
            window: shown,
            seats: summary::by_seat(&usage::rows_of(answer.rows)),
            note: None,
        },
        Err(e) => Costs {
            window: shown,
            seats: BTreeMap::new(),
            note: Some(format!(
                "額といちばん長い文脈を読めません（{}）。一覧は、そのまま出しています。",
                usage::unreadable(e)
            )),
        },
    }
}

#[derive(Template)]
#[template(path = "list.html")]
struct ListPage<'a> {
    css: &'static str,
    nav: &'a Nav,
    rows: Vec<Row>,
    /// 条件が無かったので、既定の窓（この 7 日）で出したか。
    defaulted: bool,
    days: i64,
    form: Form,
    /// 期間をすぐ選ぶ link（名と URL）。
    quick: Vec<(&'static str, String)>,
    next: Option<String>,
    /// 額の列を出した期間（時の境に揃えた物）と、列を空けた訳。
    cost_window: Option<(Time, Time)>,
    cost_note: Option<String>,
    as_of: Time,
}

/// 絞りの form に、今の値を入れて出す。
#[derive(Default)]
struct Form {
    labels: Vec<String>,
    active: String,
    kind: String,
    model: String,
    effort: String,
    active_from: String,
    active_to: String,
    skewed: bool,
    /// form に欄の無い引数（binding・at・launched_by・sort・order・limit）。そのまま送り直す。
    hidden: Vec<(String, String)>,
}

impl Form {
    fn new(pairs: &Pairs) -> Self {
        let mut form = Self::default();
        for (name, value) in pairs {
            let value = value.clone();
            match name.as_str() {
                "label" => form.labels.push(value),
                "active" => form.active = value,
                "kind" => form.kind = value,
                "model" => form.model = value,
                "effort" => form.effort = value,
                "active_from" => form.active_from = params::to_local(&value),
                "active_to" => form.active_to = params::to_local(&value),
                "skewed" => form.skewed = value == "true",
                "after" => {}
                _ => form.hidden.push((name.clone(), value)),
            }
        }
        form
    }
}

/// `GET /actors/`: アクターの一覧。空の値は付けなかったのと同じ（form は空の欄も送る）。
/// 既定は新しい順（`order=desc`）。絞る条件が無ければ、この 7 日に稼働した者を出す。
pub async fn list(
    State(app): State<Arc<App>>,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Response {
    match list_page(&app, q).await {
        Ok(page) => render(StatusCode::OK, &page),
        Err(PageError(status, message)) => error_page(&app.nav, status, &message),
    }
}

async fn list_page(
    app: &App,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Result<ListPage<'_>, PageError> {
    let mut pairs = params::pairs(q)?;
    pairs.retain(|(_, v)| !v.is_empty());
    params::period(&mut pairs, LIST_PERIOD)?;
    let form = Form::new(&pairs);
    let now = OffsetDateTime::now_utc();
    let quick = params::quick_links(PATH, &pairs, LIST_PERIOD, now);
    let mut effective = pairs.clone();
    if !effective.iter().any(|(n, _)| n == "order") {
        effective.push(("order".to_owned(), "desc".to_owned()));
    }
    if !effective.iter().any(|(n, _)| n == "limit") {
        effective.push(("limit".to_owned(), PAGE_SIZE.to_owned()));
    }
    let p = Params::new(effective.clone(), ACTOR_PARAMS, &["label"])?;
    let mut search = actor_search(&p)?;
    let defaulted = !narrowed(&search);
    if defaulted {
        let from = stamp(now - time::Duration::days(DEFAULT_DAYS));
        search.active_from = DateTime::from_str(&from, Format::DateTime).ok();
        // 次の頁も、同じ窓で続ける。
        effective.push(("active_from".to_owned(), from));
    }
    // 一覧と、席ごとの額を、一緒に聞く。
    let (answer, costs) = tokio::join!(app.castlist.actors(&search), costs(app, &search, now));
    let answer = answer.map_err(|e| match e {
        // castlist が引数を断った（callboard の確かめを通った物なので、ふつうは起きない）。castlist の訳を、そのまま書く。
        Failure::Rejected(message) => {
            params::bad_request(format!("castlist が引数を断りました: {message}"))
        }
        // この操作に、名指す物は無い。
        Failure::NotFound => params::bad_request("castlist が引数を断りました"),
        Failure::Unavailable(why) => PageError(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("castlist に聞けません（{why}）。少し待ってから開き直してください。"),
        ),
    })?;
    let next = answer.next_token.as_ref().map(|after| {
        let mut next: Pairs = effective
            .iter()
            .filter(|(n, _)| n != "after")
            .cloned()
            .collect();
        next.push(("after".to_owned(), after.clone()));
        format!("{PATH}?{}", params::query(&next))
    });
    Ok(ListPage {
        css: &CSS_HREF,
        nav: &app.nav,
        rows: answer.actors.iter().map(|a| row(a, &costs.seats)).collect(),
        defaulted,
        days: DEFAULT_DAYS,
        form,
        quick,
        next,
        cost_window: costs.window,
        cost_note: costs.note,
        as_of: time(&answer.as_of),
    })
}
