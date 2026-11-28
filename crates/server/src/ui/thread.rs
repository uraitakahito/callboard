//! 課題の筋（`GET /messages/tasks/{key}`）: 課題 1 つのメッセージを、頼みから返事までの往復の棒で、時間の上に並べる。
//!
//! - 課題は、Key の点の前の字でまとめる（`<Key>.staff`・`<Key>.<repo の席>` は、親の課題へ）。記録は pagetray の API で読む:
//!   絞り込みの候補（GetMessageFacets の keys）から `<Key>` と `<Key>.` で始まる Key を選び、Key の完全一致（ListMessages の `key`）で読む。
//!   候補（新しい 100 個）に無い古い課題は、`<Key>` と `<Key>.staff` だけを読む。
//! - 往復の作り方は [`super::exchanges`]。ここは、字と位置にして出す。
//! - 棒は、行ごとの SVG に、属性（`x`・`width`）で描く（CSP が inline の style を断る。色は CSS の class）。SVG は幅に合わせて
//!   横にだけ伸び縮みするので、字は SVG に入れず、行の左の列に HTML で書く。
//! - 引数は `project`（その project の記録だけ）と `at`（往復の頼みの id。時間の軸を、その往復の始めから終わりまでに絞る）。
//!   知らない引数は無視する。
//! - 字はみな HTML として逃がす（本文も）。本文の中の URL は、置き場のページ（`…/<種類>-<番号>.html`）と PR（`…/pull/<番号>`）の
//!   形だけを、短い名の link にする（`http://`・`https://` で始まる物だけ）。
use std::sync::Arc;

use askama::Template;
use aws_smithy_types::DateTime;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use time::OffsetDateTime;

use super::exchanges::{self, Close, Exchange, Holder, Note};
use super::i18n::{Text, filters};
use super::params::{self, PageError, Pairs, bad_request, strict};
use super::time::{Time, short};
use super::{CSS_HREF, Nav, TASKS_JS_HREF, error_page, messages, render};
use crate::App;
use crate::upstream::pagetray::{Message, MessageSearch};

/// 画面の道（axum の形）。
pub const ROUTE: &str = "/messages/tasks/{key}";

/// 1 度に読む数（ListMessages の上限）と、Key 1 つで読む頁の数の上限。
const PAGE_SIZE: i32 = 1000;
const MAX_PAGES: usize = 5;

/// 棒の幅の単位（SVG の viewBox の幅）と、棒の幅の下限。
const TRACK: i64 = 1000;
const MIN_BAR: i64 = 5;
/// 片道の知らせの印の幅。
const MARK: i64 = 6;
/// 時間の軸の目盛りの数（等しい幅）。
const TICKS: i64 = 6;

/// 本文の頭として見せる字の数。
const HEAD_CHARS: usize = 60;
/// 行に出す短い link の数の上限。
const MAX_CHIPS: usize = 3;

/// その課題の筋の URL。
pub fn href(key: &str, project: Option<&str>) -> String {
    let base = format!("/messages/tasks/{}", strict(key));
    match project {
        Some(project) => format!("{base}?project={}", strict(project)),
        None => base,
    }
}

/// Key を、課題の Key（点の前の字）にする。
pub fn task_of(key: &str) -> &str {
    key.split('.').next().unwrap_or(key)
}

/// 課題の Key の形か（英数字と `-`・`_`。点は、席の Key を分ける字なので入らない）。
pub fn is_task_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// 本文の中の、短い名にした link。
#[derive(Clone, Debug, PartialEq, Eq)]
struct Chip {
    label: String,
    href: String,
}

/// 本文の中の URL のうち、置き場のページ（`…/<種類>-<番号>.html` → `<種類>-<番号>`）と PR（`…/<repo>/pull/<番号>` → `<repo>#<番号>`）を、
/// 出た順に [`MAX_CHIPS`] つまで拾う。同じ名は 1 度だけ。
fn chips(text: &str) -> Vec<Chip> {
    let mut out: Vec<Chip> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("http") {
        let tail = &rest[at..];
        let end = tail
            .find(|c: char| c.is_whitespace() || "（）「」、。／・".contains(c))
            .unwrap_or(tail.len());
        let url = &tail[..end];
        rest = &tail[end.max(4)..];
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            continue;
        }
        let Some(label) = chip_label(url) else {
            continue;
        };
        if out.len() < MAX_CHIPS && out.iter().all(|c| c.label != label) {
            out.push(Chip {
                label,
                href: url.to_owned(),
            });
        }
    }
    out
}

fn chip_label(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let parts: Vec<&str> = path.split('/').collect();
    let last = *parts.last()?;
    if let Some(name) = last.strip_suffix(".html")
        && let Some((kind, n)) = name.rsplit_once('-')
        && !kind.is_empty()
        && kind.bytes().all(|b| b.is_ascii_lowercase())
        && !n.is_empty()
        && n.bytes().all(|b| b.is_ascii_digit())
    {
        return Some(name.to_owned());
    }
    if parts.len() >= 3
        && parts[parts.len() - 2] == "pull"
        && !last.is_empty()
        && last.bytes().all(|b| b.is_ascii_digit())
    {
        return Some(format!("{}#{last}", parts[parts.len() - 3]));
    }
    None
}

/// 頼みの見出しの、画面に出す種類の名。決めていない見出しは、書かれた字のまま。
fn kind_label(heading: &str) -> Text {
    let key = match heading {
        "brief" => "thread.kind.brief",
        "dismiss" => "thread.kind.dismiss",
        "ask-pm" => "thread.kind.askPm",
        "ask-review" => "thread.kind.askReview",
        "ask-tpm" => "thread.kind.askTpm",
        "ask-devops" => "thread.kind.askDevops",
        "ask-lead" => "thread.kind.askLead",
        _ => return Text::plain(heading),
    };
    Text::new(key)
}

/// 長さ（ミリ秒）を、画面の字にする（90 秒までは秒、90 分までは分、その先は時間（小数 1 桁））。
fn duration(ms: i64) -> Text {
    let seconds = (ms.max(0) + 500) / 1000;
    if seconds < 90 {
        Text::new("thread.duration.seconds").with_num("n", seconds)
    } else if seconds < 90 * 60 {
        Text::new("thread.duration.minutes").with_num("n", (seconds + 30) / 60)
    } else {
        let tenths = (seconds + 180) / 360;
        Text::new("thread.duration.hours").with("n", format!("{}.{}", tenths / 10, tenths % 10))
    }
}

/// 時間の軸（始めと終わり。ミリ秒）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Axis {
    from: i64,
    to: i64,
}

impl Axis {
    /// 時刻の、軸の上の位置（0〜[`TRACK`]。外は端に寄せる）。
    fn x(self, at: i64) -> i64 {
        let span = (self.to - self.from).max(1);
        ((at - self.from).saturating_mul(TRACK) / span).clamp(0, TRACK)
    }

    /// 棒の位置と幅（幅は [`MIN_BAR`] より細くしない。右の端からはみ出さない）。
    fn bar(self, from: i64, to: i64) -> (i64, i64) {
        let x = self.x(from);
        let w = (self.x(to) - x).max(MIN_BAR);
        (x.min(TRACK - w), w)
    }

    /// その間が、軸と重なるか。
    fn overlaps(self, from: i64, to: i64) -> bool {
        to >= self.from && from <= self.to
    }
}

/// 行の棒（往復）か、印（片道）。
struct Shape {
    x: i64,
    w: i64,
    class: String,
}

/// 開いた時に出す、記録 1 件。
struct Part {
    heading: String,
    from: String,
    to: String,
    sent: Time,
    delivered: Option<Time>,
    text: String,
}

/// 行 1 つ（往復か、片道の知らせ）。
struct Row {
    id: String,
    sent: Time,
    kind: Text,
    chips: Vec<Chip>,
    /// 往復の結果（返事の見出し・取り消した・返事なしで終わった・待ち）。片道には無い。
    outcome: Option<Text>,
    outcome_chips: Vec<Chip>,
    /// 往復の、球の持ち手と長さ。片道には無い。
    holder: Option<Text>,
    took: Option<Text>,
    waiting: bool,
    /// 片道の、本文の頭。
    head: Option<String>,
    shape: Shape,
    /// 時間の軸を、この往復に絞る link（依頼書の往復で、今絞っていない時だけ）。
    zoom: Option<String>,
    parts: Vec<Part>,
}

/// 球の持ち手ごとの合計。
struct Total {
    class: &'static str,
    label: Text,
    took: Text,
}

/// 課題の今の様子。
struct Status {
    class: &'static str,
    label: Text,
}

#[derive(Template)]
#[template(path = "thread.html")]
struct ThreadPage<'a> {
    css: &'static str,
    nav: &'a Nav,
    script: &'static str,
    events: &'static str,
    key: String,
    messages_href: &'static str,
    reload_href: String,
    /// 絞っている時の、課題の全体へ戻る link。
    whole_href: Option<String>,
    status: Status,
    count: usize,
    exchanges: usize,
    singles: usize,
    first: Time,
    last: Time,
    /// 返事の先でつながらなかった、古い記録の数。
    unlinked: usize,
    /// 候補に無い古い課題で、席の記録を読み切れていないかもしれない。
    partial: bool,
    latest: Part,
    holders: Vec<Total>,
    /// 誰も球を持っていなかった長さ（0 なら出さない）。
    idle: Option<Text>,
    ball: Vec<Shape>,
    ticks: Vec<Time>,
    rows: Vec<Row>,
}

/// `GET /messages/tasks/{key}`。
pub async fn thread(
    State(app): State<Arc<App>>,
    Path(key): Path<String>,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Response {
    match page(&app, &key, q, now_ms()).await {
        Ok(page) => render(StatusCode::OK, &page),
        Err(PageError(status, message)) => error_page(&app.nav, status, &message),
    }
}

fn now_ms() -> i64 {
    i64::try_from(OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
}

fn millis(t: &DateTime) -> i64 {
    t.to_millis().unwrap_or(0)
}

fn failed(e: crate::upstream::Failure) -> PageError {
    params::failed("pagetray", e)
}

/// 課題の記録を、送った時刻の順（同じ時刻は id の順）に読む。返すのは、記録と、読み切れていないかもしれないかの印。
pub(super) async fn read(
    app: &App,
    task: &str,
    project: Option<&str>,
) -> Result<(Vec<Message>, bool), PageError> {
    let facets = app
        .pagetray
        .message_facets(None, None, Some(task))
        .await
        .map_err(failed)?;
    let prefix = format!("{task}.");
    let mut keys: Vec<String> = facets
        .keys
        .iter()
        .map(|c| c.value.clone())
        .filter(|k| k == task || k.starts_with(&prefix))
        .collect();
    // 候補は新しい 100 個まで。満ちている時は、古い課題の席の Key が漏れているかもしれない（Staff の席の分は、名が決まっているので読む）。
    let partial = facets.keys.len() >= 100;
    for known in [task.to_owned(), format!("{task}.staff")] {
        if !keys.contains(&known) {
            keys.push(known);
        }
    }
    let mut found: Vec<Message> = Vec::new();
    for key in keys {
        let mut after = None;
        for _ in 0..MAX_PAGES {
            let page = app
                .pagetray
                .messages(&MessageSearch {
                    project: project.map(str::to_owned),
                    key: Some(key.clone()),
                    after,
                    limit: PAGE_SIZE,
                    ..MessageSearch::default()
                })
                .await
                .map_err(failed)?;
            found.extend(page.messages);
            after = page.next_token;
            if after.is_none() {
                break;
            }
        }
    }
    found.sort_by(|a, b| {
        a.sent_at
            .as_nanos()
            .cmp(&b.sent_at.as_nanos())
            .then_with(|| a.id.cmp(&b.id))
    });
    found.dedup_by(|a, b| a.project == b.project && a.id == b.id);
    Ok((found, partial))
}

/// 往復を作るのに読む欄にする。
pub(super) fn notes(messages: &[Message]) -> Vec<Note> {
    messages
        .iter()
        .map(|m| Note {
            id: m.id.clone(),
            heading: m.heading.clone(),
            from: m.from.clone(),
            to: m.to.clone(),
            sent: millis(&m.sent_at),
            reply_to: m.reply_to.clone(),
        })
        .collect()
}

fn part(m: &Message) -> Part {
    Part {
        heading: m.heading.clone(),
        from: m.from.clone(),
        to: m.to.clone(),
        sent: short(&m.sent_at),
        delivered: m.delivered_at.as_ref().map(short),
        text: m.text.clone(),
    }
}

/// 本文の頭（見出しの【…】と URL を外し、空白をまとめた、初めの [`HEAD_CHARS`] 字）。URL は長く、要る物は短い名の link で出してある。
fn head(m: &Message) -> String {
    let body = m
        .text
        .strip_prefix(&format!("【{}】", m.heading))
        .unwrap_or(&m.text);
    let words: Vec<&str> = body
        .split_whitespace()
        .filter(|w| !(w.starts_with("http://") || w.starts_with("https://")))
        .collect();
    let body = words.join(" ");
    let mut out: String = body.chars().take(HEAD_CHARS).collect();
    if body.chars().count() > HEAD_CHARS {
        out.push('…');
    }
    out
}

/// 引数を読み、pagetray に聞き、画面を組む。
async fn page<'a>(
    app: &'a App,
    key: &str,
    q: Result<Query<Pairs>, QueryRejection>,
    now: i64,
) -> Result<ThreadPage<'a>, PageError> {
    let not_found = || PageError(StatusCode::NOT_FOUND, Text::new("thread.notFound"));
    if !is_task_key(key) {
        return Err(not_found());
    }
    let pairs = params::pairs(q).map_err(|_| bad_request(Text::new("errors.badArguments")))?;
    let one = |name: &str| {
        pairs
            .iter()
            .find(|(n, v)| n == name && !v.trim().is_empty())
            .map(|(_, v)| v.clone())
    };
    let (project, at) = (one("project"), one("at"));
    let (found, partial) = read(app, key, project.as_deref()).await?;
    let (Some(first), Some(last)) = (found.first(), found.last()) else {
        return Err(not_found());
    };
    let notes = notes(&found);
    let built = exchanges::build(&notes, app.exchanges_since);
    let waiting = built.exchanges.iter().any(|e| e.close == Close::Waiting);
    let ends = |e: &Exchange| e.closed_by().map_or(now, |i| notes[i].sent);
    // 時間の軸: 課題の始めから、返事待ちがあれば今まで、無ければ最後の記録まで。絞っていれば、その往復の始めから終わりまで。
    let zoomed = at.as_deref().and_then(|id| {
        built
            .exchanges
            .iter()
            .find(|e| notes[e.request].id == id)
            .copied()
    });
    let whole = Axis {
        from: notes[0].sent,
        to: if waiting {
            now
        } else {
            notes[notes.len() - 1].sent
        },
    };
    let axis = zoomed.map_or(whole, |e| Axis {
        from: notes[e.request].sent,
        to: ends(&e),
    });
    // 始めと終わりが同じ（記録が 1 件だけ）なら、1 分の幅にする。
    let axis = Axis {
        to: axis.to.max(axis.from + 60_000),
        ..axis
    };
    let here = |at: Option<&str>| {
        let base = href(key, project.as_deref());
        match at {
            Some(at) => format!(
                "{base}{}at={}",
                if project.is_some() { '&' } else { '?' },
                strict(at)
            ),
            None => base,
        }
    };

    let mut rows: Vec<(i64, Row)> = Vec::new();
    for e in &built.exchanges {
        let request = &found[e.request];
        let (from, to) = (notes[e.request].sent, ends(e));
        if !axis.overlaps(from, to) {
            continue;
        }
        let closer = e.closed_by().map(|i| &found[i]);
        let (outcome, outcome_chips) = match (e.close, closer) {
            (Close::Replied(_), Some(reply)) => (Text::plain(&reply.heading), chips(&reply.text)),
            (Close::Cancelled(_), _) => (Text::new("thread.outcome.cancelled"), Vec::new()),
            (Close::Ended(_), _) => (Text::new("thread.outcome.ended"), Vec::new()),
            _ => (Text::new("thread.outcome.waiting"), Vec::new()),
        };
        let (x, w) = axis.bar(from, to);
        let open = e.close == Close::Waiting;
        // 開いた時の中身: 頼みと、返事（取り消し・終わりで閉じた時は、その記録は自分の行に出るので、頼みだけ）。
        let mut parts = vec![part(request)];
        if let (Close::Replied(_), Some(reply)) = (e.close, closer) {
            parts.push(part(reply));
        }
        rows.push((
            from,
            Row {
                id: request.id.clone(),
                sent: short(&request.sent_at),
                kind: kind_label(&request.heading),
                chips: chips(&request.text),
                outcome: Some(outcome),
                outcome_chips,
                holder: Some(e.holder.label()),
                took: Some(duration(to - from)),
                waiting: open,
                head: None,
                shape: Shape {
                    x,
                    w,
                    class: format!(
                        "bar {}{}",
                        e.holder.class(),
                        if open { " open" } else { "" }
                    ),
                },
                zoom: (request.heading == "brief" && zoomed.is_none())
                    .then(|| here(Some(&request.id))),
                parts,
            },
        ));
    }
    for &i in &built.singles {
        let m = &found[i];
        let sent = notes[i].sent;
        if !axis.overlaps(sent, sent) {
            continue;
        }
        let tone = match m.heading.as_str() {
            "incident" => " bad",
            "merged" | "doc-fix" | "done" | "end" => " ok",
            _ => "",
        };
        rows.push((
            sent,
            Row {
                id: m.id.clone(),
                sent: short(&m.sent_at),
                kind: Text::plain(&m.heading),
                chips: chips(&m.text),
                outcome: None,
                outcome_chips: Vec::new(),
                holder: None,
                took: None,
                waiting: false,
                head: Some(head(m)),
                shape: Shape {
                    x: (axis.x(sent) - MARK / 2).clamp(0, TRACK - MARK),
                    w: MARK,
                    class: format!("mark{tone}"),
                },
                zoom: None,
                parts: vec![part(m)],
            },
        ));
    }
    rows.sort_by_key(|(at, _)| *at);

    let spans = exchanges::ball(&notes, &built.exchanges, now);
    let held = |holder: Holder| -> i64 {
        spans
            .iter()
            .filter(|s| s.holder == holder)
            .map(|s| s.to - s.from)
            .sum()
    };
    let total = (whole.to - whole.from).max(0);
    let idle = total - spans.iter().map(|s| s.to - s.from).sum::<i64>();
    let status = if waiting {
        // 返事待ちがいくつもあれば、球の持ち手の帯と同じ順（PM・Staff・TPM・担当）で、前の者を出す。
        let first_waiting = built
            .exchanges
            .iter()
            .filter(|e| e.close == Close::Waiting)
            .map(|e| e.holder)
            .min();
        Status {
            class: first_waiting.map_or("", Holder::class),
            label: Text::new("thread.status.waiting").with_text(
                "who",
                &first_waiting.map_or_else(|| Text::plain(""), Holder::label),
            ),
        }
    } else if found.iter().any(|m| m.heading == "end") {
        Status {
            class: "done",
            label: Text::new("thread.status.done"),
        }
    } else {
        Status {
            class: "idle",
            label: Text::new("thread.status.idle"),
        }
    };
    Ok(ThreadPage {
        css: &CSS_HREF,
        nav: &app.nav,
        script: &TASKS_JS_HREF,
        events: messages::EVENTS_PATH,
        key: key.to_owned(),
        messages_href: messages::PATH,
        reload_href: here(at.as_deref()),
        whole_href: zoomed.map(|_| here(None)),
        status,
        count: found.len(),
        exchanges: built.exchanges.len(),
        singles: built.singles.len(),
        first: short(&first.sent_at),
        last: short(&last.sent_at),
        unlinked: built.unlinked,
        partial,
        latest: part(last),
        holders: Holder::ALL
            .into_iter()
            .filter(|&h| held(h) > 0)
            .map(|h| Total {
                class: h.class(),
                label: h.label(),
                took: duration(held(h)),
            })
            .collect(),
        idle: (idle >= 30_000).then(|| duration(idle)),
        ball: spans
            .iter()
            .filter(|s| axis.overlaps(s.from, s.to))
            .map(|s| {
                let (x, w) = axis.bar(s.from, s.to);
                Shape {
                    x,
                    w,
                    class: format!("bar {}", s.holder.class()),
                }
            })
            .collect(),
        ticks: (0..TICKS)
            .map(|i| {
                short(&DateTime::from_millis(
                    axis.from + (axis.to - axis.from) * i / TICKS,
                ))
            })
            .collect(),
        rows: rows.into_iter().map(|(_, row)| row).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 本文の_url_は_置き場のページと_pr_だけを_短い名にする() {
        let text = "【done】http://127.0.0.1:8801/p/t-1/result-2.html 要点: https://github.com/o/roster/pull/291 を入れた／\
                    https://example.com/x と http://127.0.0.1:8801/p/t-1/result-2.html（同じ物）";
        assert_eq!(
            chips(text),
            [
                Chip {
                    label: "result-2".to_owned(),
                    href: "http://127.0.0.1:8801/p/t-1/result-2.html".to_owned()
                },
                Chip {
                    label: "roster#291".to_owned(),
                    href: "https://github.com/o/roster/pull/291".to_owned()
                },
            ]
        );
        // http で始まらない字・番号の無い形は拾わない。
        assert_eq!(
            chips("javascript:alert(1) httpx://a/b-1.html http://a/b.html"),
            []
        );
        // 多くても 3 つまで。
        let many = (1..=5)
            .map(|n| format!("http://a/p/brief-{n}.html"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(chips(&many).len(), MAX_CHIPS);
    }

    #[test]
    fn 片道の行の本文の頭は_見出しと_url_を外し_長ければ切る() {
        let m = |text: &str| {
            Message::builder()
                .project("p")
                .id("m")
                .heading("merged")
                .from("K")
                .to("tpm")
                .text(text)
                .sent_at(DateTime::from_secs(0))
                .state("delivered")
                .created_at(DateTime::from_secs(0))
                .updated_at(DateTime::from_secs(0))
                .build()
                .unwrap()
        };
        assert_eq!(
            head(&m(
                "【merged】https://github.com/o/r/pull/7 段 1:\n入れた  http://a/b"
            )),
            "段 1: 入れた"
        );
        let long = head(&m(&format!("【merged】{}", "あ".repeat(HEAD_CHARS + 1))));
        assert_eq!(long.chars().count(), HEAD_CHARS + 1);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn 長さは_秒_分_時間で出す() {
        assert_eq!(duration(17_000).to_string(), "17 秒");
        assert_eq!(duration(89_400).to_string(), "89 秒");
        assert_eq!(duration(90_000).to_string(), "2 分");
        assert_eq!(duration(27 * 60_000).to_string(), "27 分");
        assert_eq!(duration(90 * 60_000).to_string(), "1.5 時間");
        assert_eq!(duration(33 * 3_600_000).to_string(), "33.0 時間");
        assert_eq!(duration(-5).to_string(), "0 秒");
    }

    #[test]
    fn 棒の位置は_軸の中に収まり_幅は下限より細くしない() {
        let axis = Axis {
            from: 0,
            to: 100_000,
        };
        assert_eq!(axis.bar(0, 50_000), (0, 500));
        assert_eq!(axis.bar(25_000, 100_000), (250, 750));
        // 短い往復は、下限の幅。右の端では、はみ出さないように左へ寄せる。
        assert_eq!(axis.bar(50_000, 50_010), (500, MIN_BAR));
        assert_eq!(axis.bar(100_000, 100_000), (TRACK - MIN_BAR, MIN_BAR));
        // 軸の外の時刻は、端に寄せる。
        assert_eq!(axis.bar(-10_000, 200_000), (0, TRACK));
        assert!(
            axis.overlaps(-5, 0)
                && axis.overlaps(100_000, 100_005)
                && !axis.overlaps(100_001, 100_002)
        );
    }

    #[test]
    fn 課題の_key_は_点の前の字_形の違う_key_は受けない() {
        assert_eq!(task_of("t-6bc6.staff"), "t-6bc6");
        assert_eq!(task_of("AI-36.pagetray.staff"), "AI-36");
        assert_eq!(task_of("devops"), "devops");
        assert!(is_task_key("AI-36") && is_task_key("t_1"));
        for bad in ["", "a.b", "a/b", "a b", "あ", &"x".repeat(129)] {
            assert!(!is_task_key(bad), "{bad}");
        }
    }
}
