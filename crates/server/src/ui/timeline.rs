//! タイムライン（`GET /graph/timeline`）。castlist の画面から移した物。
//!
//! castlist の API の `GetTimeline`（点と線と、点の出来事と、model と effort の行の全部）の答えから、並べる計算
//! （[`super::timeline_layout`]。castlist の `timeline.rs` をそのまま移した、外に触らない純な計算）で帯・矢印・印・目盛りの
//! 置き場を出し、SVG を組んでページに埋め込む（script は使わない）。並べる計算と HTML を組むのは、`spawn_blocking` の糸で行う。
use std::collections::BTreeMap;
use std::sync::Arc;

use askama::Template;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use castlist_client::operation::get_timeline::GetTimelineOutput;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::i18n::filters;
use super::params::{self, GRAPH_PERIOD, GraphForm, PageError, Pairs};
use super::profile::{event_name, source_name};
use super::time::{OFFSET, Time, stamp, time};
use super::timeline_layout::{self as timeline, END_ROOM, MARGIN, Presence, ROW_H, text_width};
use super::{CSS_HREF, Nav, error_page, kind_name, render};
use crate::App;
use crate::upstream::Failure;

/// castlist の答えを、並べる計算の前の形にした物（時刻は、castlist の API の字と同じ UTC・ミリ秒までの字）。
/// castlist の画面が DB から読んでいた行と同じ欄の名にしてある（移した code を変えないため）。
mod data {
    pub struct Actor {
        pub id: String,
        pub name: String,
        pub kind: String,
        pub received_at: String,
    }
    pub struct State {
        pub entered_at: Option<String>,
    }
    pub struct Node {
        pub actor: Actor,
        pub state: State,
    }
    pub struct Link {
        pub kind: String,
        pub from_actor: String,
        pub to_actor: String,
        pub received_at: String,
    }
    pub struct Graph {
        pub nodes: Vec<Node>,
        pub edges: Vec<Link>,
    }
    pub struct Moment {
        pub actor_id: String,
        pub kind: String,
        pub received_at: String,
        pub inferred: bool,
        pub reason: Option<String>,
    }
    pub struct Setting {
        pub actor_id: String,
        pub model: String,
        pub effort: String,
        pub source: String,
        pub received_at: String,
    }
    pub struct Timeline {
        pub graph: Graph,
        pub events: Vec<Moment>,
        pub settings: Vec<Setting>,
    }
}
use data as db;
use data as graph;

/// castlist の答え（GetTimeline）を、並べる計算の前の形にする。
fn from_answer(answer: GetTimelineOutput) -> graph::Timeline {
    let iso = |t: &aws_smithy_types::DateTime| time(t).iso;
    graph::Timeline {
        graph: graph::Graph {
            nodes: answer
                .nodes
                .into_iter()
                .map(|n| graph::Node {
                    actor: db::Actor {
                        received_at: iso(&n.received_at),
                        id: n.id,
                        name: n.name,
                        kind: n.kind,
                    },
                    state: data::State {
                        entered_at: n.state.entered_at.as_ref().map(iso),
                    },
                })
                .collect(),
            edges: answer
                .edges
                .into_iter()
                .map(|l| db::Link {
                    received_at: iso(&l.received_at),
                    kind: l.r#type,
                    from_actor: l.from,
                    to_actor: l.to,
                })
                .collect(),
        },
        events: answer
            .events
            .into_iter()
            .map(|e| graph::Moment {
                received_at: iso(&e.received_at),
                actor_id: e.actor_id,
                kind: e.r#type,
                inferred: e.inferred,
                reason: e.reason,
            })
            .collect(),
        settings: answer
            .settings
            .into_iter()
            .map(|s| db::Setting {
                received_at: iso(&s.received_at),
                actor_id: s.actor_id,
                model: s.model,
                effort: s.effort,
                source: s.source,
            })
            .collect(),
    }
}

/// 図の線 1 本（SVG の path の class と `d`）。
struct ArrowView {
    class: &'static str,
    d: String,
}

/// 名の字の大きさ（太字）と、種類の字の大きさ（px。site.css の `.timeline` と同じ）。
const NAME_PX: u32 = 13;
const SUB_PX: u32 = 11;

/// model の名を短くする（`claude-opus-5-5` → `opus-5-5`）。
fn short_model(model: &str) -> &str {
    model.strip_prefix("claude-").unwrap_or(model)
}

/// `GET /graph/timeline?label=…&from=…&to=…`: タイムライン。引数は castlist の GetTimeline と同じ（空の値は付けなかったのと同じ）。
/// `from`・`to` を付ければ、横の時刻もその窓にする（無ければ記録の初めから終わりまで）。
pub async fn timeline(
    State(app): State<Arc<App>>,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Response {
    match timeline_page(app.clone(), q).await {
        Ok(response) => response,
        Err(PageError(status, message)) => error_page(&app.nav, status, &message),
    }
}

async fn timeline_page(
    app: Arc<App>,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Result<Response, PageError> {
    let mut pairs = params::pairs(q)?;
    pairs.retain(|(_, v)| !v.is_empty());
    params::period(&mut pairs, GRAPH_PERIOD)?;
    let form = GraphForm::new(&pairs);
    let now = OffsetDateTime::now_utc();
    let quick = params::quick_links("/graph/timeline", &pairs, GRAPH_PERIOD, now);
    // 確かめた引数を、castlist の API に渡す形（期間は UTC の字）にする。
    let api = params::graph_query(&pairs)?;
    let one = |name: &str| api.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
    let (label, from, to) = (one("label"), one("from"), one("to"));
    let answer = app
        .castlist
        .timeline(label.as_deref(), from.as_deref(), to.as_deref())
        .await
        .map_err(|e| match e {
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
    // 表示した時刻は、castlist が読み始めた時刻（図の「今」の線も、この時刻）。
    let as_of = time(&answer.as_of).iso;
    tokio::task::spawn_blocking(move || {
        let data = from_answer(answer);
        let window = timeline::Window {
            from: from.as_deref().and_then(millis),
            to: to.as_deref().and_then(millis),
            now: millis(&as_of).unwrap_or_default(),
            offset_s: i64::from(OFFSET.whole_seconds()),
        };
        let page = TimelinePage {
            css: &CSS_HREF,
            nav: &app.nav,
            count: data.graph.nodes.len(),
            legend: legend(&data.settings),
            svg: timeline_svg(&data, window),
            form,
            quick,
            as_of: time_of(&as_of),
        };
        render(StatusCode::OK, &page)
    })
    .await
    .map_err(|e| {
        tracing::error!("タイムラインを表示できません: {e}");
        PageError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "タイムラインを表示できません".to_owned(),
        )
    })
}

/// ISO の字（UTC・ミリ秒まで）を、画面に出す時刻にする。
fn time_of(iso: &str) -> Time {
    match OffsetDateTime::parse(iso, &Rfc3339) {
        Ok(t) => Time {
            iso: stamp(t),
            local: super::time::local(t),
        },
        Err(_) => Time {
            iso: iso.to_owned(),
            local: iso.to_owned(),
        },
    }
}

#[derive(Template)]
#[template(path = "timeline.html")]
struct TimelinePage<'a> {
    css: &'static str,
    nav: &'a Nav,
    form: GraphForm,
    quick: Vec<(&'static str, String)>,
    count: usize,
    /// 凡例の model（短い名と色の class）。
    legend: Vec<(String, &'static str)>,
    /// 点が無ければ無い。
    svg: Option<TimelineSvg>,
    as_of: Time,
}

struct TimelineSvg {
    width: u32,
    height: u32,
    /// 目盛りの字の下の線と、目盛りの縦の線の上と下。
    tick_y: u32,
    grid_y0: u32,
    grid_y1: u32,
    row_h: u32,
    ticks: Vec<timeline::Tick>,
    /// 今の時刻の縦の線（窓の中なら）。
    now: Option<u32>,
    /// 上からの順。
    lanes: Vec<LaneView>,
    arrows: Vec<ArrowView>,
}

/// タイムラインの行 1 つ。
struct LaneView {
    href: String,
    name: String,
    kind: &'static str,
    human: bool,
    /// 行の上と、名の字の下の線と、種類の字の左。
    y: u32,
    text_y: u32,
    kind_x: u32,
    segs: Vec<SegView>,
    marks: Vec<MarkView>,
}

struct SegView {
    class: String,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    title: String,
    /// 帯に書く字と、その置き場（収まらなければ無い）。
    label: Option<(u32, u32, String)>,
    /// 開いた端の印の path（居るまま・窓の外へ続く）。
    open_end: Option<String>,
    open_start: Option<String>,
}

struct MarkView {
    kind: String,
    x: u32,
    y0: u32,
    y1: u32,
    title: String,
}

/// 推しの幕引きの訳（`detail.reason`）の字。知らない訳と、訳の無い物は、推した者だけを書く。
fn lost_reason(reason: Option<&str>) -> &'static str {
    match reason {
        Some("workspace_gone") => "居場所が消えた",
        Some("superseded") => "居場所に別の session が入った",
        _ => "roster が落ちたと推した",
    }
}

/// 印を付ける出来事。
const MARK_EVENTS: [&str; 3] = ["compacted", "reported_in", "model_unreadable"];

/// model の名から、帯の色の class（同じ名はいつも同じ class）。知らない種類はみな `m-other`。
fn tone(model: &str) -> &'static str {
    ["opus", "sonnet", "haiku"]
        .into_iter()
        .zip(["m-opus", "m-sonnet", "m-haiku"])
        .find(|(family, _)| model.contains(family))
        .map_or("m-other", |(_, class)| class)
}

/// 凡例の model: 出てくる model の短い名（名の順）と色の class。
fn legend(settings: &[db::Setting]) -> Vec<(String, &'static str)> {
    let names: std::collections::BTreeSet<&str> =
        settings.iter().map(|s| short_model(&s.model)).collect();
    names.into_iter().map(|m| (m.to_owned(), tone(m))).collect()
}

/// RFC 3339 の時刻を、UNIX の時からのミリ秒に。
fn millis(iso: &str) -> Option<i64> {
    let t = OffsetDateTime::parse(iso, &Rfc3339).ok()?;
    i64::try_from(t.unix_timestamp_nanos() / 1_000_000).ok()
}

/// タイムラインの SVG に置く物。点が無ければ None。
fn timeline_svg(data: &graph::Timeline, window: timeline::Window) -> Option<TimelineSvg> {
    let nodes = &data.graph.nodes;
    if nodes.is_empty() {
        return None;
    }
    let index: BTreeMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.actor.id.as_str(), i))
        .collect();
    let mut lanes: Vec<timeline::Lane> = nodes
        .iter()
        .map(|n| {
            let kind = kind_name(&n.actor.kind);
            let order = n
                .state
                .entered_at
                .as_deref()
                .unwrap_or(&n.actor.received_at);
            timeline::Lane {
                human: n.actor.kind == "human",
                order: millis(order).unwrap_or(i64::MAX),
                label_width: name_width(&n.actor.name)
                    + if kind.is_empty() {
                        0
                    } else {
                        6 + text_width(kind, SUB_PX)
                    },
                ..timeline::Lane::default()
            }
        })
        .collect();
    // 出来事の印（行ごとに、印の順の種類と時刻）と、model と effort の替わり目（行ごとに、元の行）。
    let mut marks: Vec<Vec<(&str, &str)>> = vec![Vec::new(); nodes.len()];
    // 推しの幕引き（行ごとに、受けた時刻と訳）。推しの切れの説明に使う。
    let mut lost: Vec<Vec<(i64, Option<&str>)>> = vec![Vec::new(); nodes.len()];
    for e in &data.events {
        let (Some(&i), Some(t)) = (index.get(e.actor_id.as_str()), millis(&e.received_at)) else {
            continue;
        };
        match e.kind.as_str() {
            "entered" | "reentered" | "forked" => lanes[i].presence.push((t, Presence::In)),
            "closed" if e.inferred => {
                lanes[i].presence.push((t, Presence::Lost));
                lost[i].push((t, e.reason.as_deref()));
            }
            "cleared" | "exited" | "closed" => lanes[i].presence.push((t, Presence::Out)),
            kind if MARK_EVENTS.contains(&kind) => {
                lanes[i].marks.push(t);
                marks[i].push((kind, &e.received_at));
            }
            _ => {}
        }
    }
    let mut changes: Vec<Vec<(&db::Setting, bool)>> = vec![Vec::new(); nodes.len()];
    let mut requested: BTreeMap<usize, (&str, &str)> = BTreeMap::new();
    for s in &data.settings {
        let (Some(&i), Some(t)) = (index.get(s.actor_id.as_str()), millis(&s.received_at)) else {
            continue;
        };
        // 見た行が、その前の最新の頼んだ行と違うか（プロフィールの「食い違い」と同じ決め方）。
        let pair = (s.model.as_str(), s.effort.as_str());
        let mismatch = s.source == "observed" && requested.get(&i).is_some_and(|r| *r != pair);
        if s.source == "requested" {
            requested.insert(i, pair);
        }
        lanes[i].changes.push(timeline::Change {
            at: t,
            label: format!("{} · {}", short_model(&s.model), s.effort),
        });
        changes[i].push((s, mismatch));
    }
    let links: Vec<timeline::Link> = data
        .graph
        .edges
        .iter()
        .filter_map(|e| {
            let kind = match e.kind.as_str() {
                "launched" => timeline::Kind::Launched,
                "continued" => timeline::Kind::Continued,
                _ => return None,
            };
            Some(timeline::Link {
                from: *index.get(e.from_actor.as_str())?,
                to: *index.get(e.to_actor.as_str())?,
                kind,
                at: millis(&e.received_at)?,
            })
        })
        .collect();
    let layout = timeline::layout(&lanes, &links, window);
    let clock = |t: i64| {
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(t) * 1_000_000)
            .ok()
            .and_then(|t| t.format(&Rfc3339).ok())
            .map(|iso| time_of(&iso).local)
            .unwrap_or_default()
    };
    let mut views: Vec<LaneView> = layout
        .order
        .iter()
        .map(|&i| {
            let n = &nodes[i];
            let y = layout.rows[i];
            LaneView {
                href: format!("/actors/{}", n.actor.id),
                name: n.actor.name.clone(),
                kind: kind_name(&n.actor.kind),
                human: lanes[i].human,
                y,
                text_y: y + ROW_H / 2 + 4,
                kind_x: MARGIN + name_width(&n.actor.name) + 6,
                segs: Vec::new(),
                marks: Vec::new(),
            }
        })
        .collect();
    let rank: BTreeMap<usize, usize> = layout
        .order
        .iter()
        .enumerate()
        .map(|(r, &i)| (i, r))
        .collect();
    for s in &layout.segments {
        let name = &nodes[s.lane].actor.name;
        let row = s
            .change
            .map(|c| (&lanes[s.lane].changes[c], changes[s.lane][c]));
        let (class, what) = match row {
            Some((change, (setting, mismatch))) => (
                format!("seg {}", tone(&setting.model)),
                format!(
                    "{}（{}{}）",
                    change.label,
                    source_name(&setting.source),
                    if mismatch { "・指定と違う" } else { "" }
                ),
            ),
            None => (
                "seg m-none".to_owned(),
                "model と effort の記録なし".to_owned(),
            ),
        };
        // 推しの切れの訳: その切れの終わりか後の、初めの推しの幕引き。
        let why = || {
            let reason = lost[s.lane]
                .iter()
                .find(|(t, _)| *t >= s.end)
                .and_then(|(_, r)| *r);
            lost_reason(reason)
        };
        let until = if s.inferred && s.open_end {
            format!("続く（表示する期間の後に推しの幕引き: {}）", why())
        } else if s.open_end {
            "続く".to_owned()
        } else if s.inferred {
            format!("{}（推し: {}）", clock(s.end), why())
        } else {
            clock(s.end)
        };
        let class = if s.inferred {
            format!("{class} inferred")
        } else {
            class
        };
        let mid = s.y + s.h / 2;
        views[rank[&s.lane]].segs.push(SegView {
            class,
            x: s.x,
            y: s.y,
            w: s.w,
            h: s.h,
            title: format!("{name} · {what} · {} から {until}", clock(s.start)),
            label: s
                .label_x
                .and_then(|x| row.map(|(c, _)| (x, mid + 4, c.label.clone()))),
            open_end: s.open_end.then(|| {
                let x = s.x + s.w;
                format!("M{x},{} L{},{mid} L{x},{}z", s.y, x + END_ROOM, s.y + s.h)
            }),
            open_start: s.open_start.then(|| {
                format!(
                    "M{},{} L{},{mid} L{},{}z",
                    s.x,
                    s.y,
                    s.x.saturating_sub(6),
                    s.x,
                    s.y + s.h
                )
            }),
        });
    }
    for m in &layout.marks {
        let (kind, at) = marks[m.lane][m.mark];
        views[rank[&m.lane]].marks.push(MarkView {
            kind: kind.to_owned(),
            x: m.x,
            y0: m.y0,
            y1: m.y1,
            title: format!("{} · {}", event_name(kind), time_of(at).local),
        });
    }
    let now = (layout.start..=layout.end)
        .contains(&window.now)
        .then(|| layout.x(window.now));
    let arrows = layout
        .arrows
        .into_iter()
        .map(|a| ArrowView {
            class: match a.kind {
                timeline::Kind::Launched => "link launched",
                timeline::Kind::Continued => "link continued",
            },
            d: a.d,
        })
        .collect();
    let grid_y0 = timeline::AXIS_H - 6;
    Some(TimelineSvg {
        width: layout.width,
        height: layout.height,
        tick_y: timeline::AXIS_H - 10,
        grid_y0,
        grid_y1: layout.height,
        row_h: ROW_H,
        now,
        ticks: layout.ticks,
        lanes: views,
        arrows,
    })
}

/// 行の名の字の幅（太字は 1 px 大きく見積もる）。
fn name_width(name: &str) -> u32 {
    text_width(name, NAME_PX + 1)
}

#[cfg(test)]
mod tests {
    use super::lost_reason;

    #[test]
    fn 推しの訳が無い物と知らない訳は_roster_が推したと書く() {
        assert_eq!(lost_reason(Some("workspace_gone")), "居場所が消えた");
        assert_eq!(
            lost_reason(Some("superseded")),
            "居場所に別の session が入った"
        );
        assert_eq!(lost_reason(None), "roster が落ちたと推した");
        assert_eq!(lost_reason(Some("other")), "roster が落ちたと推した");
    }
}
