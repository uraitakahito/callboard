//! 課題の一覧（`GET /messages/tasks`）: メッセージの記録を課題ごとにまとめ、最新の順に並べる。課題の筋（`/messages/tasks/{key}`）の入口。
//!
//! - 記録は、pagetray の API（ListMessages）で、新しい順に 1 度（[`PAGE_SIZE`] 通）読む。それより古い課題は、続きの link（`at`）で読む。
//!   課題のまとめは、読んだ記録の中だけで作る。新しい頁では、頼みの返事は頼みより後に送られるので、範囲の中の頼みの返事はみな範囲に入る
//!   （範囲より前に送った頼みの返事待ちは、ここには出ない。課題の筋を開けば出る）。続きの頁は逆で、頼みの返事が、読んでいない新しい側に
//!   在りうる。だから、続きの頁では、今の様子と返事待ちを出さない。
//! - 課題は、project と、Key の点の前の字（[`super::thread::task_of`]）の組でまとめる。Key の無い記録・課題の Key の形でない記録は、数えない。
//! - 課題の今の様子は、課題の筋と同じ決め（往復の作り方は [`super::exchanges`]）: 返事待ちがあれば「<誰> の返事待ち」（帯と同じ順で前の者）、
//!   無くて `end` があれば「済み」、ほかは「次の依頼待ち」。
//! - 引数は `project`・`waiting`（`1` なら、返事待ちの課題だけ）・`at`（続きの印。ListMessages の `nextToken`）。知らない引数は無視する。
//!   値の形は pagetray が確かめる（断られたら、pagetray の訳を書いた 400 の画面）。
use std::sync::Arc;

use askama::Template;
use aws_smithy_types::DateTime;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;

use super::exchanges::{self, Close, Holder, Note};
use super::i18n::{Text, filters};
use super::params::{self, PageError, Pairs, bad_request, strict};
use super::thread::{self, Status};
use super::time::{Time, short};
use super::{CSS_HREF, Nav, TASKS_JS_HREF, error_page, messages, render};
use crate::App;
use crate::upstream::pagetray::MessageSearch;

/// 一覧の道。
pub const PATH: &str = "/messages/tasks";

/// 1 度に読む数（ListMessages の上限）。
const PAGE_SIZE: i32 = 1000;

/// 課題の、返事待ち（球の持ち手は、帯と同じ順で前の者。`since` は、その者への、いちばん古い返事待ちの頼みの時刻）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Waiting {
    pub holder: Holder,
    pub since: i64,
    /// 返事待ちの頼みの数（どの持ち手の分も）。
    pub count: usize,
}

/// 課題 1 つのまとめ。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    /// 記録の project。
    pub project: String,
    /// 課題の Key（記録の Key の、点の前の字）。
    pub key: String,
    pub count: usize,
    pub first: i64,
    pub last: i64,
    /// 最新の 1 通の、渡した並びの中の位置。
    pub latest: usize,
    pub waiting: Option<Waiting>,
    /// 片付けの終わり（`end`）があり、返事待ちが無い。
    pub done: bool,
}

/// 記録（どの順でもよい）を、課題ごとにまとめる（最新の 1 通が新しい順）。`keys` は記録ごとの Key（無い記録・課題の Key の形でない記録は、数えない）。
pub fn summarize(
    projects: &[String],
    keys: &[Option<String>],
    notes: &[Note],
    since: Option<i64>,
) -> Vec<Task> {
    // project と課題の Key の組ごとの、記録の位置（出てきた順）。同じ Key でも、project が違えば別の課題（返事の先の id は、project の中の物）。
    let mut groups: Vec<((&str, &str), Vec<usize>)> = Vec::new();
    for (i, (project, key)) in projects.iter().zip(keys).enumerate().take(notes.len()) {
        let Some(task) = key
            .as_deref()
            .map(thread::task_of)
            .filter(|task| thread::is_task_key(task))
        else {
            continue;
        };
        let name = (project.as_str(), task);
        match groups.iter_mut().find(|(group, _)| *group == name) {
            Some((_, members)) => members.push(i),
            None => groups.push((name, vec![i])),
        }
    }
    let mut tasks: Vec<Task> = groups
        .into_iter()
        .map(|((project, key), mut members)| {
            // 往復は、送った時刻の順（同じ時刻は id の順）で作る。
            members.sort_by(|&a, &b| {
                notes[a]
                    .sent
                    .cmp(&notes[b].sent)
                    .then_with(|| notes[a].id.cmp(&notes[b].id))
            });
            let ordered: Vec<Note> = members.iter().map(|&i| notes[i].clone()).collect();
            let built = exchanges::build(&ordered, since);
            let open: Vec<_> = built
                .exchanges
                .iter()
                .filter(|e| e.close == Close::Waiting)
                .collect();
            let waiting = open.iter().map(|e| e.holder).min().map(|holder| Waiting {
                holder,
                since: open
                    .iter()
                    .filter(|e| e.holder == holder)
                    .map(|e| ordered[e.request].sent)
                    .min()
                    .unwrap_or_default(),
                count: open.len(),
            });
            let latest = members[members.len() - 1];
            Task {
                project: project.to_owned(),
                key: key.to_owned(),
                count: members.len(),
                first: notes[members[0]].sent,
                last: notes[latest].sent,
                latest,
                done: waiting.is_none() && ordered.iter().any(|n| n.heading == "end"),
                waiting,
            }
        })
        .collect();
    tasks.sort_by(|a, b| {
        b.last
            .cmp(&a.last)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.project.cmp(&b.project))
    });
    tasks
}

/// 課題の一覧の URL（`project` があれば、その project だけ）。
pub fn href(project: Option<&str>) -> String {
    View {
        project: project.map(str::to_owned),
        ..View::default()
    }
    .href()
}

/// 画面の引数（空でない値だけ）。
#[derive(Clone, Default)]
struct View {
    project: Option<String>,
    waiting: bool,
    /// 並びの続きの印（前の頁の答えの `nextToken`）。
    at: Option<String>,
}

impl View {
    fn read(pairs: &Pairs) -> Self {
        let one = |name: &str| {
            pairs
                .iter()
                .find(|(n, v)| n == name && !v.trim().is_empty())
                .map(|(_, v)| v.clone())
        };
        Self {
            project: one("project"),
            waiting: one("waiting").as_deref() == Some("1"),
            at: one("at"),
        }
    }

    /// この条件の一覧の URL。空の条件は出さない。
    fn href(&self) -> String {
        let mut query: Vec<String> = Vec::new();
        if let Some(project) = &self.project {
            query.push(format!("project={}", strict(project)));
        }
        if self.waiting {
            query.push("waiting=1".to_owned());
        }
        if let Some(at) = &self.at {
            query.push(format!("at={}", strict(at)));
        }
        if query.is_empty() {
            PATH.to_owned()
        } else {
            format!("{PATH}?{}", query.join("&"))
        }
    }
}

/// 一覧の、課題 1 つの行。
struct Row {
    key: String,
    project: String,
    /// project の名を行に出すか（読んだ記録に、project がいくつもある時）。
    show_project: bool,
    href: String,
    /// 今の様子。続きの頁では無い（新しい側の記録を読んでいないので、分からない）。
    status: Option<Status>,
    /// 返事待ちの、いつから（その者への、いちばん古い頼みの時刻）。
    since: Option<Time>,
    /// ほかの返事待ちの数（0 なら出さない）。
    others: usize,
    count: usize,
    first: Time,
    last: Time,
    /// 最新の 1 通の見出しと、本文の頭。
    heading: String,
    head: String,
}

#[derive(Template)]
#[template(path = "tasks.html")]
struct TasksPage<'a> {
    css: &'static str,
    nav: &'a Nav,
    script: &'static str,
    events: &'static str,
    /// 絞っている project（新着の印を、その project の記録でだけ出す）。
    project: Option<String>,
    messages_href: &'static str,
    reload_href: String,
    /// 続きの頁（`at` がある）か。続きの頁では、今の様子と返事待ちを出さない: 頼みは読んだ範囲に在っても、その返事は、もっと新しい
    /// （前の頁の）記録に在って読んでいないので、うその返事待ちになる。
    older: bool,
    /// 新しい頁（続きの印の無い一覧）の URL。
    newest_href: String,
    /// 返事待ちの課題の数（読んだ範囲の中）と、「返事待ちだけ」の釦の先（絞っていれば、外す先）・今絞っているか。
    waiting: usize,
    waiting_href: String,
    waiting_active: bool,
    /// 読んだ記録の数。
    read: usize,
    rows: Vec<Row>,
    more_href: Option<String>,
}

/// `GET /messages/tasks`。
pub async fn tasks(
    State(app): State<Arc<App>>,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Response {
    match page(&app, q).await {
        Ok(page) => render(StatusCode::OK, &page),
        Err(PageError(status, message)) => error_page(&app.nav, status, &message),
    }
}

/// 引数を読み、pagetray に聞き、画面を組む。
async fn page(
    app: &App,
    q: Result<Query<Pairs>, QueryRejection>,
) -> Result<TasksPage<'_>, PageError> {
    let pairs = params::pairs(q).map_err(|_| bad_request(Text::new("errors.badArguments")))?;
    let view = View::read(&pairs);
    let older = view.at.is_some();
    let found = app
        .pagetray
        .messages(&MessageSearch {
            project: view.project.clone(),
            after: view.at.clone(),
            limit: PAGE_SIZE,
            ..MessageSearch::default()
        })
        .await
        .map_err(|e| params::failed("pagetray", e))?;
    let projects: Vec<String> = found.messages.iter().map(|m| m.project.clone()).collect();
    let keys: Vec<Option<String>> = found.messages.iter().map(|m| m.key.clone()).collect();
    let notes = thread::notes(&found.messages);
    let all = summarize(&projects, &keys, &notes, app.exchanges_since);
    // project がいくつもある時は、行に名を出し、link に付ける（絞っている時は、絞った project を付ける）。
    let several = all.iter().any(|t| t.project != all[0].project);
    let waiting = all.iter().filter(|t| t.waiting.is_some()).count();
    let rows = all
        .into_iter()
        .filter(|t| older || !view.waiting || t.waiting.is_some())
        .map(|t| {
            let latest = &found.messages[t.latest];
            let project = view
                .project
                .as_deref()
                .or_else(|| several.then_some(t.project.as_str()));
            let waiting = t.waiting.filter(|_| !older);
            Row {
                href: thread::href(&t.key, project),
                status: (!older).then(|| thread::status(t.waiting.map(|w| w.holder), t.done)),
                since: waiting.map(|w| short(&DateTime::from_millis(w.since))),
                others: waiting.map_or(0, |w| w.count - 1),
                count: t.count,
                first: short(&DateTime::from_millis(t.first)),
                last: short(&DateTime::from_millis(t.last)),
                heading: latest.heading.clone(),
                head: thread::head(latest),
                show_project: several,
                project: t.project,
                key: t.key,
            }
        })
        .collect();
    // project だけを保った一覧（新しい頁・続きの頁の元）。返事待ちの絞りは、続きの頁には運ばない（そこでは効かない）。
    let base = View {
        project: view.project.clone(),
        ..View::default()
    };
    Ok(TasksPage {
        css: &CSS_HREF,
        nav: &app.nav,
        script: &TASKS_JS_HREF,
        events: messages::EVENTS_PATH,
        project: view.project.clone(),
        messages_href: messages::PATH,
        reload_href: view.href(),
        older,
        newest_href: base.href(),
        waiting,
        waiting_href: View {
            waiting: !view.waiting,
            ..base.clone()
        }
        .href(),
        waiting_active: view.waiting,
        read: found.messages.len(),
        rows,
        more_href: found.next_token.map(|at| {
            View {
                at: Some(at),
                ..base.clone()
            }
            .href()
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 記録 1 件（`at` は秒）と、その Key。
    fn note(
        id: &str,
        key: &str,
        heading: &str,
        from: &str,
        to: &str,
        at: i64,
        reply_to: Option<&str>,
    ) -> (Option<String>, Note) {
        (
            Some(key.to_owned()).filter(|k| !k.is_empty()),
            Note {
                id: id.to_owned(),
                heading: heading.to_owned(),
                from: from.to_owned(),
                to: to.to_owned(),
                sent: at * 1000,
                reply_to: reply_to.map(str::to_owned),
            },
        )
    }

    fn run(records: Vec<(Option<String>, Note)>, since: Option<i64>) -> Vec<Task> {
        let (keys, notes): (Vec<_>, Vec<_>) = records.into_iter().unzip();
        let projects = vec!["p".to_owned(); notes.len()];
        summarize(&projects, &keys, &notes, since)
    }

    #[test]
    fn 同じ_key_でも_project_が違えば_別の課題にまとめる() {
        let (keys, notes): (Vec<_>, Vec<_>) = vec![
            note("b", "A-1", "brief", "tpm", "A-1", 10, None),
            note("d", "A-1", "done", "A-1", "tpm", 20, Some("b")),
            // 別の project の、同じ Key の課題。id も同じ（project ごとに別の物）。
            note("b", "A-1", "brief", "tpm", "A-1", 30, None),
        ]
        .into_iter()
        .unzip();
        let projects = ["p".to_owned(), "p".to_owned(), "q".to_owned()];
        let tasks = summarize(&projects, &keys, &notes, None);
        assert_eq!(
            tasks
                .iter()
                .map(|t| (
                    t.project.as_str(),
                    t.key.as_str(),
                    t.count,
                    t.waiting.is_some()
                ))
                .collect::<Vec<_>>(),
            // q の依頼書は返事待ち。p の依頼書は、p の done が返っている。
            [("q", "A-1", 1, true), ("p", "A-1", 2, false)]
        );
    }

    #[test]
    fn 記録を_key_の点の前の字でまとめ_最新の_1_通が新しい課題から並べる() {
        // 新しい順で渡す（pagetray の答えの順）。
        let tasks = run(
            vec![
                note(
                    "v",
                    "A-1.staff",
                    "review",
                    "A-1.staff",
                    "A-1",
                    50,
                    Some("r"),
                ),
                note("n", "B-2", "notify", "tpm", "B-2", 40, None),
                note("r", "A-1", "ask-review", "A-1", "A-1.staff", 30, None),
                note("b", "A-1", "brief", "tpm", "A-1", 10, None),
                // Key の無い記録・課題の Key の形でない記録は、数えない。
                note("x", "", "notify", "tpm", "z", 60, None),
                note("y", "あ", "notify", "tpm", "z", 70, None),
            ],
            None,
        );
        assert_eq!(
            tasks
                .iter()
                .map(|t| (
                    t.key.as_str(),
                    t.count,
                    t.first / 1000,
                    t.last / 1000,
                    t.latest
                ))
                .collect::<Vec<_>>(),
            [("A-1", 3, 10, 50, 0), ("B-2", 1, 40, 40, 1)]
        );
    }

    #[test]
    fn 返事待ちは_帯と同じ順で前の者と_その者へのいちばん古い頼みの時刻と_数() {
        let tasks = run(
            vec![
                note("b", "A-1", "brief", "tpm", "A-1", 10, None),
                note("q1", "A-1", "ask-pm", "A-1", "tpm", 20, None),
                note("q2", "A-1", "ask-pm", "A-1", "tpm", 30, None),
                note("r", "A-1", "ask-review", "A-1", "A-1.staff", 40, None),
                note(
                    "v",
                    "A-1.staff",
                    "review",
                    "A-1.staff",
                    "A-1",
                    50,
                    Some("r"),
                ),
            ],
            None,
        );
        // 依頼書（担当）と確認 2 つ（PM）が返事待ち。前の者は PM、その古い方は 20 秒。レビューは返った。
        assert_eq!(
            tasks[0].waiting,
            Some(Waiting {
                holder: Holder::Pm,
                since: 20_000,
                count: 3
            })
        );
        assert!(!tasks[0].done);
    }

    #[test]
    fn end_があり返事待ちが無ければ済み_付け始めた時刻より前の頼みは待ちにしない() {
        let records = || {
            vec![
                note("b", "A-1", "brief", "tpm", "A-1", 10, None),
                note("d", "A-1", "done", "A-1", "tpm", 20, None),
                note("b2", "B-2", "brief", "tpm", "B-2", 10, None),
                note("d2", "B-2", "done", "B-2", "tpm", 20, Some("b2")),
                note("x2", "B-2", "dismiss", "tpm", "B-2", 30, None),
                note("e2", "B-2", "end", "B-2", "tpm", 40, Some("x2")),
            ]
        };
        let tasks = run(records(), Some(5_000));
        let of = |tasks: &[Task], key: &str| tasks.iter().find(|t| t.key == key).cloned().unwrap();
        assert!(of(&tasks, "B-2").done && of(&tasks, "B-2").waiting.is_none());
        // A-1 の依頼書は、付け始めた時刻（5 秒）より後で、返事の先の無い done は返事にならない: 返事待ち。
        assert_eq!(of(&tasks, "A-1").waiting.map(|w| w.count), Some(1));
        // 付け始めた時刻が後（100 秒）なら、前の頼みは待ちにしない。end も無いので、済みでもない。
        let tasks = run(records(), Some(100_000));
        assert!(of(&tasks, "A-1").waiting.is_none() && !of(&tasks, "A-1").done);
    }
}
