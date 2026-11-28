//! 課題の一覧（`GET /messages/tasks`）: メッセージの記録を課題ごとにまとめ、最新の順に並べる。課題の筋（`/messages/tasks/{key}`）の入口。
//! まだ作っていない（試験を先に書いた。形だけを置く）。
#![allow(dead_code)]
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;

use super::error_page;
use super::exchanges::{Holder, Note};
use super::i18n::Text;
use crate::App;

/// 一覧の道。
pub const PATH: &str = "/messages/tasks";

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
pub fn summarize(keys: &[Option<String>], notes: &[Note], since: Option<i64>) -> Vec<Task> {
    let _ = (keys, notes, since);
    Vec::new()
}

/// `GET /messages/tasks`。
pub async fn tasks(State(app): State<Arc<App>>) -> Response {
    error_page(
        &app.nav,
        StatusCode::NOT_FOUND,
        &Text::new("error.noSuchScreen"),
    )
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
        summarize(&keys, &notes, since)
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
