//! アクターの一覧（`GET /actors/`）。castlist の画面（castlist の `/`）から移した物。
//!
//! 絞り込みは、castlist の API の `ListActors`（監査の検索）と同じ条件を、画面の引数の名（snake_case。`activeFrom` は
//! `active_from`、`launchedBy` は `launched_by`、`maxResults` は `limit`、`nextToken` は `after`）で URL に置く。
//! 引数は画面が確かめ（誤りは、引数の名を書いた 400 の画面。castlist の画面と同じ字）、確かめた物を API に渡す。
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

use super::params::{self, LIST_PERIOD, PageError, Pairs, Params, bad_request};
use super::profile::event_name;
use super::time::{Time, stamp, time};
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
}

fn row(item: &ActorRow) -> Row {
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
    let answer = app.castlist.actors(&search).await.map_err(|e| match e {
        // castlist が引数を断った（callboard の確かめを通った物なので、起きない見込み）。
        Failure::NotFound => bad_request("castlist が引数を断りました"),
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
        rows: answer.actors.iter().map(row).collect(),
        defaulted,
        days: DEFAULT_DAYS,
        form,
        quick,
        next,
        as_of: time(&answer.as_of),
    })
}
