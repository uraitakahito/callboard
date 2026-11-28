//! 課題の筋（`GET /messages/tasks/{key}`）: 課題 1 つのメッセージを、頼みから返事までの往復の棒で、時間の上に並べる。
//! まだ作っていない（試験を先に書いた。形だけを置く）。
#![allow(dead_code)]
use std::sync::Arc;

use aws_smithy_types::DateTime;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;

use super::error_page;
use super::i18n::Text;
use crate::App;
use crate::upstream::pagetray::Message;

/// 画面の道（axum の形）。
pub const ROUTE: &str = "/messages/tasks/{key}";

const TRACK: i64 = 1000;
const MIN_BAR: i64 = 5;
const HEAD_CHARS: usize = 60;
const MAX_CHIPS: usize = 3;

pub fn task_of(key: &str) -> &str {
    key
}

pub fn is_task_key(_key: &str) -> bool {
    false
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Chip {
    label: String,
    href: String,
}

fn chips(_text: &str) -> Vec<Chip> {
    Vec::new()
}

fn duration(_ms: i64) -> Text {
    Text::plain("")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Axis {
    from: i64,
    to: i64,
}

impl Axis {
    fn bar(self, _from: i64, _to: i64) -> (i64, i64) {
        (0, 0)
    }

    fn overlaps(self, _from: i64, _to: i64) -> bool {
        false
    }
}

fn head(_m: &Message) -> String {
    String::new()
}

/// `GET /messages/tasks/{key}`。
pub async fn thread(State(app): State<Arc<App>>) -> Response {
    let _ = DateTime::from_secs(0);
    error_page(
        &app.nav,
        StatusCode::NOT_FOUND,
        &Text::new("error.noSuchScreen"),
    )
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
