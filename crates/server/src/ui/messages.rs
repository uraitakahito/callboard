//! メッセージの一覧（`GET /messages`）と、新着の知らせの道（`GET /api/pagetray/events`）。pagetray の画面（pagetray の
//! `/messages`）から移した物。
//!
//! - pagetray は、ほかの道具（roster）が配ったメッセージの記録を見せるだけ。callboard は、その記録を pagetray の API
//!   （ListMessages と GetMessageFacets）で読んで出す。状態は、pagetray の答えの字を、画面の字にして出す。
//! - 絞り込みは link で動き、条件は URL に出る。引数は `project`・`key`・`to`・`from`・`state`・`at`（続きの印。ListMessages の
//!   `nextToken`）。空は書かないのと同じ。知らない `state` と、知らない引数は無視する（pagetray の画面と同じ）。
//!   値の形は pagetray が確かめる（断られたら、pagetray の訳を書いた 400 の画面）。
//! - 「まだ届いていない n 件」と絞り込みの候補は、絞りに関わらず置き場の全部（GetMessageFacets の答えのまま）。
//! - 字はみな HTML として逃がす（本文も。HTML として出さない）。
//! - script（`static/messages.js`）が無くても一覧は動く。script は、新着の知らせで一覧の列と絞り込みの列を差し替える。
use std::sync::Arc;

use askama::Template;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;

use super::i18n::filters;
use super::params::{self, PageError, Pairs, bad_request, strict};
use super::time::{Time, short};
use super::{CSS_HREF, MESSAGES_JS_HREF, Nav, error_page, render};
use crate::App;
use crate::upstream::Failure;
use crate::upstream::pagetray::{FacetCount, Message, MessageSearch};

/// 一覧の道。
pub const PATH: &str = "/messages";

/// script が新着の知らせを聞く道。pagetray の `GET /api/events` を、そのまま流す。
pub const EVENTS_PATH: &str = "/api/pagetray/events";

/// 1 度に出す数（pagetray の画面と同じ）。
const PAGE_SIZE: i32 = 200;

/// 状態の字（pagetray の定義の `MessageState`）と、画面に出す字。
const STATES: [(&str, &str); 5] = [
    ("waiting", "まだ届いていない"),
    ("delivered", "届いた"),
    ("undeliverable", "渡せない"),
    ("expired", "期限切れ"),
    ("carried", "ほかの道具で運んだ"),
];

/// 渡した時の名（呼ぶ側が `via` に書く字）と、画面に出す字。ここに無い名は、書かれた字のまま出す。
const VIAS: [(&str, &str); 6] = [
    ("gate", "検査が止めた後"),
    ("wake", "起こした"),
    ("prompt", "人の入力の時"),
    ("max-deliveries", "渡し直しの上限を超えた"),
    ("ttl", "期限で消えた"),
    ("closed", "席を閉じた"),
];

/// 本文をそのまま出す長さ（字の数と行の数）。超えれば、頭だけ見せて、開くと全部。
const TEXT_CHARS: usize = 200;
const TEXT_LINES: usize = 5;
/// 長い本文の、頭として見せる字の数。
const HEAD_CHARS: usize = 120;

/// 画面の引数（空でない値だけ）。
#[derive(Clone, Default)]
struct View {
    project: Option<String>,
    key: Option<String>,
    /// 受け手の名。
    to: Option<String>,
    /// 送り手の名。
    from: Option<String>,
    /// 状態（[`STATES`] の字）。
    state: Option<String>,
    /// 並びの続きの印（前の頁の答えの `nextToken`）。
    at: Option<String>,
}

impl View {
    /// 引数の名と、その値の置き場（URL に出す順）。
    fn fields(&mut self) -> [(&'static str, &mut Option<String>); 6] {
        [
            ("project", &mut self.project),
            ("key", &mut self.key),
            ("to", &mut self.to),
            ("from", &mut self.from),
            ("state", &mut self.state),
            ("at", &mut self.at),
        ]
    }

    /// 名と値の並びから読む。同じ名を 2 度書いた物は 400（pagetray の画面と同じ）。空の値と、知らない状態は、書かないのと同じ。
    fn read(pairs: Pairs) -> Result<Self, PageError> {
        let mut view = Self::default();
        let mut seen = Vec::new();
        for (name, value) in pairs {
            let Some((_, field)) = view.fields().into_iter().find(|(n, _)| *n == name) else {
                continue;
            };
            if seen.contains(&name) {
                return Err(bad_request(format!(
                    "引数を読めません（{name} を 2 度書いています）"
                )));
            }
            seen.push(name);
            *field = Some(value).filter(|v| !v.trim().is_empty());
        }
        view.state = view.state.filter(|s| state_label(s).is_some());
        Ok(view)
    }

    /// この条件の一覧の URL。空の条件は出さない。
    fn href(&self) -> String {
        let mut view = self.clone();
        let pairs: Pairs = view
            .fields()
            .into_iter()
            .filter_map(|(name, value)| Some((name.to_owned(), value.clone()?)))
            .collect();
        if pairs.is_empty() {
            PATH.to_owned()
        } else {
            let query: Vec<String> = pairs
                .iter()
                .map(|(name, value)| format!("{name}={}", strict(value)))
                .collect();
            format!("{PATH}?{}", query.join("&"))
        }
    }

    /// 条件を 1 つ変えた一覧の URL。並びの続きの印は、条件が変わると意味が無いので外す。
    fn with(&self, change: impl FnOnce(&mut View)) -> String {
        let mut next = self.clone();
        next.at = None;
        change(&mut next);
        next.href()
    }
}

/// 絞り込みの列の、候補 1 つ。
struct Choice {
    label: String,
    count: i64,
    href: String,
    active: bool,
}

/// 絞り込みの列の、見出し 1 つと、その候補。
struct FilterGroup {
    title: &'static str,
    choices: Vec<Choice>,
}

/// 一覧の列の、メッセージ 1 件分。
struct Row {
    project: String,
    id: String,
    /// Key を書かずに置いた記録は空。
    key: String,
    heading: String,
    from: String,
    to: String,
    /// 本文（字のまま。template が逃がす）。
    text: String,
    /// 長い本文の頭（改行を空白にした、初めの [`HEAD_CHARS`] 字。切った時は後ろに「…」）。短い本文には無い。
    head: Option<String>,
    sent: Time,
    /// 状態（[`STATES`] の字。知らない字は空）と、画面に出す字。
    state: &'static str,
    state_label: &'static str,
    delivered: Option<Time>,
    /// どの時に渡したかの、画面に出す字。書いていなければ空。
    via: String,
    /// 渡し直した数（渡した回数 − 1）。渡した回数が 2 以上の時だけ出す。
    again: i32,
    /// 最後に書き足された時刻（script が、行が替わったかを見る）。
    updated_at: String,
}

impl Row {
    /// 届いた時刻・渡した時・渡し直しの数の、どれかを出すか。
    fn has_notes(&self) -> bool {
        self.delivered.is_some() || !self.via.is_empty() || self.again > 0
    }
}

#[derive(Template)]
#[template(path = "messages.html")]
struct MessagesPage<'a> {
    css: &'static str,
    nav: &'a Nav,
    script: &'static str,
    events: &'static str,
    /// まだ届いていないメッセージの数（絞り込みに関わらず、置き場の全部）。tab の題と、操作の帯の釦に出す。
    waiting: i64,
    /// 「まだ届いていない n 件」の釦の先（ほかの絞り込みを外して、まだ届いていない物だけに絞る）と、今そう絞っているか。
    waiting_href: String,
    waiting_active: bool,
    reload_href: String,
    reset_href: String,
    filtered: bool,
    filters: Vec<FilterGroup>,
    rows: Vec<Row>,
    more_href: Option<String>,
}

/// 状態の名（'static にする）と、画面に出す字。知らない字なら None。
fn state_label(state: &str) -> Option<(&'static str, &'static str)> {
    STATES.into_iter().find(|(name, _)| *name == state)
}

/// 渡した時の名の、画面に出す字。決めていない名は、書かれた字のまま（隠さない）。
fn via_label(via: &str) -> &str {
    VIAS.into_iter()
        .find(|(name, _)| *name == via)
        .map_or(via, |(_, label)| label)
}

/// `GET /messages`。
pub async fn messages(
    State(app): State<Arc<App>>,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Response {
    match page(&app, q).await {
        Ok(page) => render(StatusCode::OK, &page),
        Err(PageError(status, message)) => error_page(&app.nav, status, &message),
    }
}

/// `GET /api/pagetray/events`: pagetray の新着の知らせを、そのまま流す。
pub async fn events(State(app): State<Arc<App>>) -> Response {
    app.pagetray
        .events(app.events_wait, app.closing.clone())
        .await
}

/// pagetray に聞けなかった訳を、画面の誤りにする。
fn failed(e: Failure) -> PageError {
    match e {
        // pagetray が引数を断った（値の形が違う・続きの印が読めない）。pagetray の訳を、そのまま書く。
        Failure::Rejected(message) => {
            bad_request(format!("pagetray が引数を断りました: {message}"))
        }
        // この操作に、名指す物は無い。
        Failure::NotFound => bad_request("pagetray が引数を断りました"),
        Failure::Unavailable(why) => PageError(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("pagetray に聞けません（{why}）。少し待ってから開き直してください。"),
        ),
    }
}

/// 条件を読み、pagetray に聞き、画面を組む。
async fn page(
    app: &App,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Result<MessagesPage<'_>, PageError> {
    let view = View::read(params::pairs(q).map_err(|_| bad_request("引数を読めません"))?)?;
    let search = MessageSearch {
        project: view.project.clone(),
        key: view.key.clone(),
        to: view.to.clone(),
        from: view.from.clone(),
        state: view.state.clone(),
        after: view.at.clone(),
        limit: PAGE_SIZE,
    };
    let (found, facets) = tokio::join!(
        app.pagetray.messages(&search),
        app.pagetray.message_facets(
            view.to.as_deref(),
            view.from.as_deref(),
            view.key.as_deref()
        ),
    );
    let (found, facets) = (found.map_err(failed)?, facets.map_err(failed)?);

    let more_href = found.next_token.map(|at| view.with(|v| v.at = Some(at)));
    let waiting = facets
        .states
        .iter()
        .find(|c| c.value == "waiting")
        .map_or(0, |c| c.count);
    // 候補の並び。押すと今の条件に重ね、選んである候補をもう 1 度押すと外す。`show` が、候補の字を決める（値そのものか、状態の字）。
    let group = |title,
                 counts: Vec<FacetCount>,
                 field: fn(&mut View) -> &mut Option<String>,
                 show: fn(&str) -> String| FilterGroup {
        title,
        choices: counts
            .into_iter()
            .map(|c| {
                let mut probe = view.clone();
                let active = field(&mut probe).as_deref() == Some(c.value.as_str());
                let label = show(&c.value);
                let next = (!active).then_some(c.value);
                Choice {
                    label,
                    count: c.count,
                    href: view.with(|v| *field(v) = next),
                    active,
                }
            })
            .collect(),
    };
    Ok(MessagesPage {
        css: &CSS_HREF,
        nav: &app.nav,
        script: &MESSAGES_JS_HREF,
        events: EVENTS_PATH,
        waiting,
        waiting_href: View {
            state: Some("waiting".to_owned()),
            ..View::default()
        }
        .href(),
        waiting_active: view.state.as_deref() == Some("waiting"),
        reload_href: view.with(|_| {}),
        reset_href: View::default().href(),
        filtered: [&view.project, &view.key, &view.to, &view.from, &view.state]
            .iter()
            .any(|v| v.is_some()),
        filters: vec![
            // 状態の候補は、値でなく画面の字で出す（並びは pagetray の答えの順: 決まった順で、件数が 0 の物は無い）。
            group(
                "状態",
                facets.states,
                |v| &mut v.state,
                |s| state_label(s).map_or(s, |(_, label)| label).to_owned(),
            ),
            group("受け手", facets.to, |v| &mut v.to, str::to_owned),
            group("送り手", facets.from, |v| &mut v.from, str::to_owned),
            group("Key", facets.keys, |v| &mut v.key, str::to_owned),
            group(
                "project",
                facets.projects,
                |v| &mut v.project,
                str::to_owned,
            ),
        ],
        rows: found.messages.into_iter().map(row).collect(),
        more_href,
    })
}

/// 長い本文（字か行が多い）なら、頭として見せる字。改行（`\n`・`\r`）は空白にし、切った時だけ後ろに「…」を付ける。
fn head(text: &str) -> Option<String> {
    let chars = text.chars().count();
    let long = chars > TEXT_CHARS || text.lines().count() > TEXT_LINES;
    long.then(|| {
        let mut head: String = text
            .chars()
            .take(HEAD_CHARS)
            .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
            .collect();
        if chars > HEAD_CHARS {
            head.push('…');
        }
        head
    })
}

fn row(m: Message) -> Row {
    let (state, state_label) = state_label(&m.state).unwrap_or(("", ""));
    Row {
        head: head(&m.text),
        sent: short(&m.sent_at),
        delivered: m.delivered_at.as_ref().map(short),
        again: m.deliveries.map_or(0, |n| n - 1).max(0),
        state,
        state_label,
        via: m
            .via
            .as_deref()
            .map(via_label)
            .unwrap_or_default()
            .to_owned(),
        updated_at: short(&m.updated_at).iso,
        project: m.project,
        id: m.id,
        key: m.key.unwrap_or_default(),
        heading: m.heading,
        from: m.from,
        to: m.to,
        text: m.text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs<const N: usize>(pairs: [(&str, &str); N]) -> Pairs {
        pairs
            .into_iter()
            .map(|(n, v)| (n.to_owned(), v.to_owned()))
            .collect()
    }

    #[test]
    fn link_は条件を運び_空の条件は出さない() {
        let view = View::read(pairs([
            ("to", "a b?c&d#e/f"),
            ("state", "waiting"),
            ("at", "6d65"),
        ]))
        .ok()
        .unwrap();
        assert_eq!(
            view.href(),
            "/messages?to=a%20b%3Fc%26d%23e%2Ff&state=waiting&at=6d65"
        );
        assert_eq!(view.with(|v| v.to = None), "/messages?state=waiting");
        assert_eq!(View::default().href(), "/messages");
    }

    #[test]
    fn 空の値と知らない状態と知らない引数は_書かないのと同じ() {
        let view = View::read(pairs([
            ("to", " "),
            ("state", "done"),
            ("nope", "1"),
            ("key", "K"),
        ]))
        .ok()
        .unwrap();
        assert_eq!(view.href(), "/messages?key=K");
        // 同じ名を 2 度書いた物は断る（空の値でも）。
        assert!(View::read(pairs([("to", "a"), ("to", "")])).is_err());
    }

    #[test]
    fn 長い本文だけに_頭が付く() {
        assert_eq!(head("短い\n2 行目"), None);
        assert_eq!(head(&"あ".repeat(200)), None);
        // 切った時だけ「…」が付く（120 字 + 1）。
        assert_eq!(
            head(&"あ".repeat(201)).map(|h| h.chars().count()),
            Some(121)
        );
        assert_eq!(head("1\r\n2\n3\n4\n5\n6"), Some("1  2 3 4 5 6".to_owned()));
        assert_eq!(head("1\n2\n3\n4\n5"), None);
        assert_eq!(head("1\n2\n3\n4\n5\n6"), Some("1 2 3 4 5 6".to_owned()));
    }
}
