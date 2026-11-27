//! プロフィール（`GET /actors/{id}`）。castlist の画面から移した物。
//!
//! castlist の API の 2 つの答えから組む: アクター 1 つ（GetActor）と、起こした・継いだ線を辿った先（GetLineage。
//! 上は人の根まで、下は 1 段）。2 つは別々の読みなので、間に記録が足されると、片方にだけ出ることがある（名が引けない
//! アクターは、id のまま出す）。
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use askama::Template;
use aws_smithy_types::{Document, Number};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use castlist_client::operation::get_actor::GetActorOutput;
use castlist_client::operation::get_lineage::GetLineageOutput;
use castlist_client::types::{ActorRow, Link};
use serde_json::Value;
use time::OffsetDateTime;

use super::i18n::{Text, filters};
use super::params;
use super::time::{Time, local, stamp, time};
use super::usage::{self, ChartView};
use super::usage_summary as summary;
use super::{CSS_HREF, Nav, error_page, kind_name, render};
use crate::App;
use crate::upstream::Failure;
use crate::upstream::castlist::UsageAnswer;

/// 線を上る段の上限（castlist の GetLineage の上限と同じ）。
const MAX_DEPTH: usize = 20;

/// 出来事の種類と、画面に出す名の鍵（id は英語のまま。名は画面だけで、字は `static/locales/` の `events`）。
pub(crate) const EVENT_NAMES: [(&str, &str); 10] = [
    ("entered", "events.entered"),
    ("reentered", "events.reentered"),
    ("forked", "events.forked"),
    ("compacted", "events.compacted"),
    ("cleared", "events.cleared"),
    ("exited", "events.exited"),
    ("closed", "events.closed"),
    ("reported_in", "events.reportedIn"),
    ("model_unreadable", "events.modelUnreadable"),
    ("retracted", "events.retracted"),
];

/// 出来事の種類の、画面に出す名。知らない種類は id のまま（鍵は付けない）。
pub fn event_name(kind: &str) -> Text {
    EVENT_NAMES
        .iter()
        .find(|(id, _)| *id == kind)
        .map_or_else(|| Text::plain(kind), |(_, key)| Text::new(key))
}

/// アクターを指す link（名と種類）。
struct ActorLink {
    href: String,
    name: String,
    kind: Option<Text>,
}

fn actor_link(id: &str, nodes: &BTreeMap<&str, &ActorRow>) -> ActorLink {
    let node = nodes.get(id);
    ActorLink {
        href: format!("/actors/{id}"),
        name: node.map_or_else(|| id.to_owned(), |n| n.name.clone()),
        kind: node.and_then(|n| kind_name(&n.kind)),
    }
}

/// プロフィールの頭と「今の状態」に出す物。
struct Me {
    name: String,
    kind: Option<Text>,
    present: bool,
    /// 今の状態を決めた出来事が、推しの幕引き（「停止（推し）」と書く）。
    inferred: bool,
    /// 稼働を替える出来事の最新（名・id）。
    presence: Option<(Text, String)>,
    /// ラベルの全部（鍵・値。鍵の順）。
    labels: Vec<(String, String)>,
    /// 最新の model と effort。
    setting: Option<(String, String)>,
    first_entered: Option<Time>,
    last_exited: Option<Time>,
    /// 時刻ずれの行（登録・出来事・model と effort）を持つか。
    skewed: bool,
}

#[derive(Template)]
#[template(path = "profile.html")]
struct ProfilePage<'a> {
    css: &'static str,
    nav: &'a Nav,
    id: String,
    issuer: String,
    registered: Time,
    me: Me,
    current: Option<CurrentSetting>,
    bindings: Vec<BindingRow>,
    /// 起こした人（根の人から、このアクターの手前まで）。線の種類の名も。
    chain: Vec<(ActorLink, Text)>,
    children: Vec<ActorLink>,
    before: Vec<ActorLink>,
    after: Vec<ActorLink>,
    settings: Vec<SettingRow>,
    events: Vec<EventRow>,
    usage: UsageSection,
    as_of: Time,
}

/// 節「使用量」（直近 31 日の、このアクターの数）。数を読めなければ `error`、行が無ければ `view` が無い。
struct UsageSection {
    from: Time,
    to: Time,
    error: Option<Text>,
    view: Option<UsageView>,
}

struct UsageView {
    cost: String,
    main_cost: String,
    helper_cost: String,
    turns: String,
    helper_turns: String,
    max_context: Option<Text>,
    /// 行が多すぎて、途中で止めた。
    truncated: bool,
    /// 本体の回が無ければ無い。
    chart: Option<ChartView>,
}

fn time_of(t: OffsetDateTime) -> Time {
    Time {
        iso: stamp(t),
        local: local(t),
    }
}

/// 節「使用量」を組む。圧縮の印は、出来事のうち、期間の内の要約（`compacted`）。
fn usage_section(
    window: (OffsetDateTime, OffsetDateTime),
    found: Result<UsageAnswer, Failure>,
    actor: &GetActorOutput,
) -> UsageSection {
    let mut section = UsageSection {
        from: time_of(window.0),
        to: time_of(window.1),
        error: None,
        view: None,
    };
    let answer = match found {
        Ok(answer) => answer,
        Err(e) => {
            section.error =
                Some(Text::new("profile.usage.error").with_text("why", &usage::unreadable(e)));
            return section;
        }
    };
    if answer.rows.is_empty() {
        return section;
    }
    let truncated = usage::cut(&answer);
    let rows = usage::rows_of(answer.rows);
    let total = summary::summarize(&rows, &[]).total;
    let max_context = summary::by_seat(&rows)
        .values()
        .map(|s| s.max_context)
        .max()
        .unwrap_or(0);
    let (from, to) = (window.0.unix_timestamp(), window.1.unix_timestamp());
    let marks: Vec<(i64, Text)> = actor
        .events
        .iter()
        .filter(|e| e.r#type == "compacted")
        .filter_map(|e| {
            let at = e.received_at.secs();
            (from..to).contains(&at).then(|| (at, compaction_title(e)))
        })
        .collect();
    section.view = Some(UsageView {
        cost: usage::dollars(total.cost),
        main_cost: usage::dollars(total.cost - total.helper_cost),
        helper_cost: usage::dollars(total.helper_cost),
        turns: usage::count(total.turns),
        helper_turns: usage::count(total.helper_turns),
        max_context: usage::longest(max_context),
        truncated,
        chart: usage::line_chart(&rows, &marks),
    });
    section
}

/// 圧縮の印の字: 出来事の名と時刻と、detail に前と後の文脈の長さ（`preTokens`・`postTokens`）が在れば、それも。
fn compaction_title(event: &castlist_client::types::Event) -> Text {
    let detail = json(&event.detail);
    let tokens = |key: &str| detail.get(key).and_then(Value::as_i64);
    let name = event_name(&event.r#type);
    let at = time(&event.received_at).local;
    match (tokens("preTokens"), tokens("postTokens")) {
        (Some(pre), Some(post)) => Text::new("profile.compaction.tokens")
            .with_text("name", &name)
            .with("at", at)
            .with_text("pre", &usage::man(pre))
            .with_text("post", &usage::man(post)),
        _ => Text::new("common.eventAt")
            .with_text("name", &name)
            .with("at", at),
    }
}

struct CurrentSetting {
    source: Text,
    received: Time,
}

struct BindingRow {
    kind: String,
    value: String,
    is_key: bool,
    current: bool,
    received: Time,
}

struct SettingRow {
    received: Time,
    model: String,
    effort: String,
    source: Text,
    /// 実際の行（observed）が、その前に最後に指定した物（requested）と違う（画面の「食い違い」）。
    mismatch: bool,
    skewed: bool,
}

struct EventRow {
    received: Time,
    name: Text,
    kind: String,
    reason: Option<String>,
    source: Option<String>,
    /// reason・source の外の detail（JSON の字。無ければ空）。
    rest: String,
    sent: Time,
    skewed: bool,
}

pub fn source_name(source: &str) -> Text {
    Text::new(if source == "requested" {
        "profile.source.requested"
    } else {
        "profile.source.observed"
    })
}

/// castlist のアクターの id の形（`ai.common#ActorId`: 小文字の 16 進の UUID の字）。違う形の id は、castlist に聞かずに 404 にする
/// （castlist の API は形の違う id を 400 で断るが、画面では「無いアクター」と同じに見せる。前の画面と同じ）。
fn is_actor_id(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(g, len)| {
            g.len() == len && g.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
}

/// `GET /actors/{id}`: 1 つのアクターのプロフィール。無いアクターは 404 の画面。castlist に聞けなければ 503 の画面。
pub async fn profile(State(app): State<Arc<App>>, Path(id): Path<String>) -> Response {
    let nav = &app.nav;
    let not_found = || error_page(nav, StatusCode::NOT_FOUND, &Text::new("profile.notFound"));
    if !is_actor_id(&id) {
        return not_found();
    }
    // 節「使用量」の数は 3 つ目の問い。これが落ちても、頁は出す（節にだけ訳を書く）。
    let window = usage::recent(OffsetDateTime::now_utc());
    let search = usage::search(window, Vec::new(), Some(id.clone()));
    let (actor, lineage, found) = tokio::join!(
        app.castlist.actor(&id),
        app.castlist.lineage(&id),
        app.castlist.usage(&search)
    );
    match (actor, lineage) {
        (Ok(actor), Ok(lineage)) => match profile_page(
            nav,
            &id,
            &actor,
            &lineage,
            usage_section(window, found, &actor),
        ) {
            Some(page) => render(StatusCode::OK, &page),
            None => not_found(),
        },
        // castlist が id を断った時（形は先に見てあるので、ふつうは起きない）も、無いアクターと同じに見せる。
        (Err(Failure::NotFound | Failure::Rejected(_)), _)
        | (_, Err(Failure::NotFound | Failure::Rejected(_))) => not_found(),
        (Err(e @ Failure::Unavailable(_)), _) | (_, Err(e @ Failure::Unavailable(_))) => {
            let params::PageError(status, message) = params::failed("castlist", e);
            error_page(nav, status, &message)
        }
    }
}

fn profile_page<'a>(
    nav: &'a Nav,
    id: &str,
    actor: &GetActorOutput,
    lineage: &GetLineageOutput,
    usage: UsageSection,
) -> Option<ProfilePage<'a>> {
    let nodes: BTreeMap<&str, &ActorRow> =
        lineage.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    // 辿った先に自分がいなければ（2 つの読みの間に替わった、など）、無いアクターとする。
    let me = nodes.get(id)?;
    let ends = |links: &[Link], kind: &str, outward: bool| -> Vec<ActorLink> {
        links
            .iter()
            .filter(|l| l.r#type == kind)
            .map(|l| actor_link(if outward { &l.to } else { &l.from }, &nodes))
            .collect()
    };
    let current: HashSet<(&str, &str, i128)> = actor
        .current_bindings
        .iter()
        .map(|b| {
            (
                b.r#type.as_str(),
                b.value.as_str(),
                b.received_at.as_nanos(),
            )
        })
        .collect();
    let bindings = actor
        .bindings
        .iter()
        .map(|b| BindingRow {
            kind: b.r#type.clone(),
            value: b.value.clone(),
            is_key: b.is_key,
            current: current.contains(&(
                b.r#type.as_str(),
                b.value.as_str(),
                b.received_at.as_nanos(),
            )),
            received: time(&b.received_at),
        })
        .collect();
    // 実際の行を、その前の最新の指定した行と比べる（castlist の監査の検索の mismatch と同じ決め方）。
    let mut requested: Option<(&str, &str)> = None;
    let settings = actor
        .settings
        .iter()
        .map(|s| {
            let pair = (s.model.as_str(), s.effort.as_str());
            let mismatch = s.source == "observed" && requested.is_some_and(|r| r != pair);
            if s.source == "requested" {
                requested = Some(pair);
            }
            SettingRow {
                received: time(&s.received_at),
                model: s.model.clone(),
                effort: s.effort.clone(),
                source: source_name(&s.source),
                mismatch,
                skewed: s.clock_skew,
            }
        })
        .collect();
    let events = actor
        .events
        .iter()
        .map(|e| {
            let mut detail = json(&e.detail);
            let mut take = |key: &str| {
                detail
                    .as_object_mut()
                    .and_then(|o| o.remove(key))
                    .map(|v| match v {
                        Value::String(s) => s,
                        other => other.to_string(),
                    })
            };
            let reason = take("reason");
            let source = take("source");
            let rest = match &detail {
                Value::Object(o) if o.is_empty() => String::new(),
                other => other.to_string(),
            };
            EventRow {
                received: time(&e.received_at),
                name: event_name(&e.r#type),
                kind: e.r#type.clone(),
                reason,
                source,
                rest,
                sent: time(&e.sent_at),
                skewed: e.clock_skew,
            }
        })
        .collect();
    let mut labels: Vec<(String, String)> = me
        .labels
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    labels.sort();
    Some(ProfilePage {
        css: &CSS_HREF,
        nav,
        id: actor.id.clone(),
        issuer: actor.issuer.clone(),
        registered: time(&actor.received_at),
        me: Me {
            name: me.name.clone(),
            kind: kind_name(&me.kind),
            present: me.present,
            inferred: me.inferred,
            // 稼働を替える出来事の最新と、時刻ずれの印は、辿った先の行（ActorRow。castlist 1.1.0 から）から読む（一覧の行と同じ値）。
            presence: me
                .presence
                .as_deref()
                .map(|p| (event_name(p), p.to_owned())),
            labels,
            setting: me.model.clone().zip(me.effort.clone()),
            first_entered: me.first_entered_at.as_ref().map(time),
            last_exited: me.last_exited_at.as_ref().map(time),
            skewed: me.skewed.unwrap_or(false),
        },
        current: actor.current_settings.as_ref().map(|c| CurrentSetting {
            source: source_name(&c.source),
            received: time(&c.received_at),
        }),
        bindings,
        children: ends(&actor.links_out, "launched", true),
        before: ends(&actor.links_in, "continued", false),
        after: ends(&actor.links_out, "continued", true),
        chain: chain(id, &lineage.edges, &nodes),
        settings,
        events,
        usage,
        as_of: time(&actor.as_of),
    })
}

/// 出来事の detail（Smithy の Document）を、JSON の値にする（物の鍵は、名の順に並べる）。
fn json(document: &Document) -> Value {
    match document {
        Document::Object(map) => {
            let sorted: BTreeMap<&String, &Document> = map.iter().collect();
            Value::Object(
                sorted
                    .into_iter()
                    .map(|(k, v)| (k.clone(), json(v)))
                    .collect(),
            )
        }
        Document::Array(items) => Value::Array(items.iter().map(json).collect()),
        Document::Number(Number::PosInt(n)) => Value::from(*n),
        Document::Number(Number::NegInt(n)) => Value::from(*n),
        Document::Number(Number::Float(n)) => {
            serde_json::Number::from_f64(*n).map_or(Value::Null, Value::Number)
        }
        Document::String(s) => Value::String(s.clone()),
        Document::Bool(b) => Value::Bool(*b),
        Document::Null => Value::Null,
    }
}

/// 起こした人の鎖: このアクターから、起こした・継がせた線を上り、人で止める。根から順に返す。
/// 両方の線がある時は、起こした（launched）方を辿る。
fn chain(id: &str, edges: &[Link], nodes: &BTreeMap<&str, &ActorRow>) -> Vec<(ActorLink, Text)> {
    let mut out = Vec::new();
    let mut seen: HashSet<&str> = HashSet::from([id]);
    let mut current = id;
    for _ in 0..MAX_DEPTH {
        let up = |kind: &str| edges.iter().find(|l| l.to == current && l.r#type == kind);
        let Some(edge) = up("launched").or_else(|| up("continued")) else {
            break;
        };
        let from = edge.from.as_str();
        if !seen.insert(from) {
            break;
        }
        let verb = Text::new(if edge.r#type == "launched" {
            "profile.chain.launched"
        } else {
            "profile.chain.continued"
        });
        out.push((actor_link(from, nodes), verb));
        if nodes.get(from).is_some_and(|n| n.kind == "human") {
            break;
        }
        current = from;
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn アクターの_id_は_小文字の_uuid_の字() {
        assert!(is_actor_id("019a0000-0000-7000-8000-00000000000a"));
        for ng in [
            "",
            "019A0000-0000-7000-8000-00000000000a",
            "019a0000-0000-7000-8000-00000000000",
            "019a0000-0000-7000-8000-00000000000a-",
            "../0000-0000-7000-8000-00000000000a",
            "019a00000000-7000-8000-00000000000a",
        ] {
            assert!(!is_actor_id(ng), "{ng}");
        }
    }

    /// 出来事の種類（castlist の定義の EventType の 10 種）。
    const EVENT_TYPES: [&str; 10] = [
        "entered",
        "reentered",
        "forked",
        "compacted",
        "cleared",
        "exited",
        "closed",
        "reported_in",
        "model_unreadable",
        "retracted",
    ];

    #[test]
    fn 出来事の種類はみな日本語の名を持つ() {
        let names: Vec<String> = EVENT_TYPES
            .iter()
            .map(|t| event_name(t).to_string())
            .collect();
        assert_eq!(
            names,
            [
                "登場",
                "再登場",
                "分身",
                "要約",
                "交代",
                "退場",
                "幕引き",
                "名乗り",
                "model 不明",
                "取り消し"
            ]
        );
        // 鍵は static/locales/ の events の物。
        let keys: Vec<&str> = EVENT_TYPES.iter().map(|t| event_name(t).key()).collect();
        assert!(keys.iter().all(|k| k.starts_with("events.")), "{keys:?}");
        // 知らない種類は、id のまま出す（鍵は付けない）。
        assert_eq!(event_name("unknown"), "unknown");
        assert_eq!(event_name("unknown").attrs(), "");
    }

    #[test]
    fn detail_は_鍵を名の順に並べた_json_にする() {
        let document = Document::Object(
            [
                ("b".to_owned(), Document::Number(Number::PosInt(2))),
                ("a".to_owned(), Document::Bool(true)),
                (
                    "c".to_owned(),
                    Document::Array(vec![Document::Null, Document::String("x".to_owned())]),
                ),
            ]
            .into(),
        );
        assert_eq!(
            json(&document).to_string(),
            r#"{"a":true,"b":2,"c":[null,"x"]}"#
        );
    }
}
