//! 往復を作る（課題の筋 `GET /messages/tasks/{key}` と、課題の一覧 `GET /messages/tasks` の元）。
//!
//! メッセージの記録（pagetray の `Message`）を、頼みから返事までの往復と、片道の知らせに分ける。画面は持たない（字と位置は `thread.rs`）。
//!
//! - **頼み**は、見出しが `brief`・`dismiss` か、`ask-` で始まる物。ほかの見出しは片道の知らせ（知らない見出しも片道。送る側（roster）が
//!   見出しを足しても、ここは壊れない）。
//! - **返事**は、記録の `replyTo`（返事の先の id）でつながる。送る側が、返事を送る命令で付ける。頼みを閉じるのは次の 3 つだけ:
//!   1. `replyTo` がその頼みを指す、いちばん早い記録（見出しは問わない。前の依頼書を取り消す `brief` のように、頼みが頼みを閉じることもある）。
//!   2. `dismiss`: その席への、まだ返事の無い `brief` をみな（取り消した）。
//!   3. `end`: その席が送った・受けた、まだ返事の無い頼みをみな（返事なしで終わった）。
//! - **古い記録**: 送る側が `replyTo` を付け始める前の記録は、頼みと返事がつながっていない。`since`（付け始めた時刻。設定
//!   `CALLBOARD_EXCHANGES_SINCE`）より前に送った頼みで、返事の無い物は、待ちにせず片道として出す。見出しで往復を推すことはしない
//!   （うその待ち・うその往復を出さない）。

use super::i18n::Text;

/// 往復を作るのに読む、記録 1 件の欄（時刻は UNIX のミリ秒）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Note {
    pub id: String,
    pub heading: String,
    pub from: String,
    pub to: String,
    pub sent: i64,
    pub reply_to: Option<String>,
}

/// 往復の間、球を持つ者（頼みの受け手）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Holder {
    /// Product Manager（`ask-pm`。受け手は TPM 担当でも、答えるのは Product Manager）。
    Pm,
    /// Staff Engineer 担当の席（受け手の名が `.staff` で終わる）。
    Staff,
    /// TPM 担当（受け手の名が `tpm`）。
    Tpm,
    /// 課題の担当（そのほか）。
    Seat,
}

impl Holder {
    /// 球の持ち手の並び（帯で重なった時は、前の物を塗る）。
    pub const ALL: [Holder; 4] = [Holder::Pm, Holder::Staff, Holder::Tpm, Holder::Seat];

    fn of(request: &Note) -> Self {
        if request.heading == "ask-pm" {
            Holder::Pm
        } else if request.to == "tpm" {
            Holder::Tpm
        } else if request.to.ends_with(".staff") {
            Holder::Staff
        } else {
            Holder::Seat
        }
    }

    /// CSS の class（色）。
    pub fn class(self) -> &'static str {
        match self {
            Holder::Pm => "h-pm",
            Holder::Staff => "h-staff",
            Holder::Tpm => "h-tpm",
            Holder::Seat => "h-seat",
        }
    }

    /// 画面に出す名。
    pub fn label(self) -> Text {
        Text::new(match self {
            Holder::Pm => "thread.holder.pm",
            Holder::Staff => "thread.holder.staff",
            Holder::Tpm => "thread.holder.tpm",
            Holder::Seat => "thread.holder.seat",
        })
    }
}

/// 往復の閉じ方（数は、閉じた記録の、渡した並びの中の位置）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Close {
    /// 返事が来た（`replyTo` がこの頼みを指す記録）。
    Replied(usize),
    /// `dismiss` が取り消した。
    Cancelled(usize),
    /// `end` で、返事なしで終わった。
    Ended(usize),
    /// まだ返事が無い。
    Waiting,
}

/// 往復 1 つ（頼みの位置と、閉じ方）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exchange {
    pub request: usize,
    pub close: Close,
    pub holder: Holder,
}

impl Exchange {
    /// 閉じた記録の位置（まだ返事が無ければ無い）。
    pub fn closed_by(&self) -> Option<usize> {
        match self.close {
            Close::Replied(i) | Close::Cancelled(i) | Close::Ended(i) => Some(i),
            Close::Waiting => None,
        }
    }
}

/// 記録を、往復と片道に分けた物。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exchanges {
    /// 往復（頼みの送った時刻の順）。
    pub exchanges: Vec<Exchange>,
    /// 片道の知らせの位置（送った時刻の順）。返事として往復に入った記録は入らない。
    pub singles: Vec<usize>,
    /// 返事の見出し（`done`・`end`・`review`・`answer`）なのに、返事の先でどの頼みにもつながらなかった記録の数（古い記録）。
    pub unlinked: usize,
}

/// 頼みの見出しか。
pub fn is_request(heading: &str) -> bool {
    heading == "brief" || heading == "dismiss" || heading.starts_with("ask-")
}

/// 返事の見出しか（送る側が、返事の先を必ず付ける見出し）。
fn is_reply(heading: &str) -> bool {
    matches!(heading, "done" | "end" | "review" | "answer")
}

/// 記録（送った時刻の順に並べた物）を、往復と片道に分ける。`since` は、送る側が返事の先を付け始めた時刻（ミリ秒）。
pub fn build(notes: &[Note], since: Option<i64>) -> Exchanges {
    let position = |id: &str| notes.iter().position(|n| n.id == id);
    // 頼みごとの、いちばん早い返事。
    let mut close = vec![Close::Waiting; notes.len()];
    // 返事として、頼みの往復に入った記録（頼みでもある記録は、自分の往復の行も持つので、片道からだけ外す）。
    let mut answered = vec![false; notes.len()];
    for (i, note) in notes.iter().enumerate() {
        let parent = note
            .reply_to
            .as_deref()
            .and_then(position)
            .filter(|&j| j != i && is_request(&notes[j].heading));
        if let Some(j) = parent {
            answered[i] = true;
            if close[j] == Close::Waiting {
                close[j] = Close::Replied(i);
            }
        }
    }
    // dismiss と end は、返事の無いまま残った頼みを閉じる（時刻の順に見るので、先に来た方が閉じる）。
    for (i, note) in notes.iter().enumerate() {
        let closes: fn(&Note, &Note) -> bool = match note.heading.as_str() {
            "dismiss" => |end, r| r.heading == "brief" && r.to == end.to,
            "end" => |end, r| r.from == end.from || r.to == end.from,
            _ => continue,
        };
        for j in 0..i {
            if close[j] == Close::Waiting
                && is_request(&notes[j].heading)
                && closes(note, &notes[j])
            {
                close[j] = if note.heading == "dismiss" {
                    Close::Cancelled(i)
                } else {
                    Close::Ended(i)
                };
            }
        }
    }
    let mut out = Exchanges::default();
    for (i, note) in notes.iter().enumerate() {
        let old = since.is_some_and(|since| note.sent < since);
        if is_request(&note.heading) && !(old && close[i] == Close::Waiting) {
            out.exchanges.push(Exchange {
                request: i,
                close: close[i],
                holder: Holder::of(note),
            });
        } else if !answered[i] {
            out.singles.push(i);
        }
        if is_reply(&note.heading) && !answered[i] {
            out.unlinked += 1;
        }
    }
    out
}

/// 球の持ち手の帯の 1 区間（`from` から `to` まで。ミリ秒）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub from: i64,
    pub to: i64,
    pub holder: Holder,
}

/// 球の持ち手の帯: 時間ごとに、開いている往復の持ち手（重なる時は [`Holder::ALL`] の前の物）。隣り合う同じ持ち手はまとめる。
/// まだ返事の無い往復は `now` まで続く。誰も持っていない間は、区間が無い。
pub fn ball(notes: &[Note], exchanges: &[Exchange], now: i64) -> Vec<Span> {
    let ends = |e: &Exchange| e.closed_by().map_or(now, |i| notes[i].sent);
    let mut edges: Vec<i64> = exchanges
        .iter()
        .flat_map(|e| [notes[e.request].sent, ends(e)])
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let mut out: Vec<Span> = Vec::new();
    for pair in edges.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let holder = exchanges
            .iter()
            .filter(|e| notes[e.request].sent <= from && ends(e) >= to)
            .map(|e| e.holder)
            .min();
        let Some(holder) = holder else { continue };
        match out.last_mut() {
            Some(last) if last.holder == holder && last.to == from => last.to = to,
            _ => out.push(Span { from, to, holder }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 記録 1 件（`at` は秒）。
    fn note(
        id: &str,
        heading: &str,
        from: &str,
        to: &str,
        at: i64,
        reply_to: Option<&str>,
    ) -> Note {
        Note {
            id: id.to_owned(),
            heading: heading.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            sent: at * 1000,
            reply_to: reply_to.map(str::to_owned),
        }
    }

    fn closes(notes: &[Note], since: Option<i64>) -> Vec<(String, Close)> {
        build(notes, since)
            .exchanges
            .iter()
            .map(|e| (notes[e.request].id.clone(), e.close))
            .collect()
    }

    #[test]
    fn 返事は_返事の先が指す頼み_1_つだけを閉じる() {
        // brief-1 → brief-2（--after）→ done（brief-1 へ）→ done（brief-2 へ）。
        let notes = [
            note("b1", "brief", "tpm", "K", 0, None),
            note("b2", "brief", "tpm", "K", 10, None),
            note("d1", "done", "K", "tpm", 20, Some("b1")),
            note("m", "merged", "K", "tpm", 25, None),
            note("d2", "done", "K", "tpm", 30, Some("b2")),
        ];
        let built = build(&notes, None);
        assert_eq!(
            closes(&notes, None),
            [
                ("b1".to_owned(), Close::Replied(2)),
                ("b2".to_owned(), Close::Replied(4))
            ]
        );
        // 返事は片道に入らない。片道は merged だけ。
        assert_eq!(built.singles, [3]);
        assert_eq!(built.unlinked, 0);
    }

    #[test]
    fn 間を飛ばした返事は_間の頼みを閉じない_新しい方に先に返しても_古い方は残る() {
        // --briefs brief-3 だけ: brief-2 は待ちのまま。
        let notes = [
            note("b2", "brief", "tpm", "K", 0, None),
            note("b3", "brief", "tpm", "K", 10, None),
            note("d3", "done", "K", "tpm", 20, Some("b3")),
            // レビューも同じ: 新しい頼みに先に返す。
            note("r1", "ask-review", "K", "K.staff", 30, None),
            note("r2", "ask-review", "K", "K.staff", 40, None),
            note("v2", "review", "K.staff", "K", 50, Some("r2")),
        ];
        assert_eq!(
            closes(&notes, None),
            [
                ("b2".to_owned(), Close::Waiting),
                ("b3".to_owned(), Close::Replied(2)),
                ("r1".to_owned(), Close::Waiting),
                ("r2".to_owned(), Close::Replied(5)),
            ]
        );
    }

    #[test]
    fn 同じ頼みへの_2_通目の返事は_片道にもならず_往復は初めの返事で閉じる() {
        let notes = [
            note("b", "brief", "tpm", "K", 0, None),
            note("d1", "done", "K", "tpm", 10, Some("b")),
            note("d2", "done", "K", "tpm", 20, Some("b")),
        ];
        let built = build(&notes, None);
        assert_eq!(built.exchanges[0].close, Close::Replied(1));
        assert_eq!(built.singles, Vec::<usize>::new());
    }

    #[test]
    fn 頼みが頼みを閉じる_取り消す依頼書は_前の依頼書の返事で_自分の往復も持つ() {
        let notes = [
            note("b2", "brief", "tpm", "K", 0, None),
            note("b3", "brief", "tpm", "K", 10, Some("b2")),
            note("d3", "done", "K", "tpm", 20, Some("b3")),
        ];
        assert_eq!(
            closes(&notes, None),
            [
                ("b2".to_owned(), Close::Replied(1)),
                ("b3".to_owned(), Close::Replied(2))
            ]
        );
        assert_eq!(build(&notes, None).singles, Vec::<usize>::new());
    }

    #[test]
    fn dismiss_は_その席への返事の無い依頼書を取り消し_end_は_その席の残りを閉じる() {
        let notes = [
            note("b1", "brief", "tpm", "K", 0, None),
            note("q", "ask-devops", "K", "tpm", 5, None),
            note("o", "brief", "tpm", "other", 6, None),
            note("x", "dismiss", "tpm", "K", 10, None),
            note("e", "end", "K", "tpm", 20, Some("x")),
        ];
        assert_eq!(
            closes(&notes, None),
            [
                ("b1".to_owned(), Close::Cancelled(3)),
                // 相談は dismiss では閉じず、end で「返事なしで終わった」。
                ("q".to_owned(), Close::Ended(4)),
                // ほかの席への依頼書は、どちらも閉じない。
                ("o".to_owned(), Close::Waiting),
                ("x".to_owned(), Close::Replied(4)),
            ]
        );
    }

    #[test]
    fn 返事の先が記録に無い返事と_頼みでない記録への返事は_片道() {
        let notes = [
            note("i", "incident", "K", "tpm", 0, None),
            note("n", "notify", "tpm", "K", 10, Some("i")),
            note("d", "done", "K", "tpm", 20, Some("gone")),
            note("a", "answer", "tpm", "K", 30, None),
        ];
        let built = build(&notes, None);
        assert_eq!(built.exchanges, []);
        assert_eq!(built.singles, [0, 1, 2, 3]);
        // 返事の見出しで、つながらなかった物（done と answer）。notify は数えない。
        assert_eq!(built.unlinked, 2);
    }

    #[test]
    fn 付け始めた時刻より前の_返事の無い頼みは_待ちにせず片道にする_またぐ課題は後ろだけ往復() {
        let notes = [
            // 前: 依頼書に、返事の先の無い done が返った（つながらない）。
            note("b1", "brief", "tpm", "K", 0, None),
            note("d1", "done", "K", "tpm", 10, None),
            // 前に送り、後で返事の先の付いた返事が来た頼みは、往復。
            note("q", "ask-pm", "K", "tpm", 20, None),
            note("a", "answer", "tpm", "K", 110, Some("q")),
            // 後: 返事の無い頼みは待ち。
            note("b2", "brief", "tpm", "K", 120, None),
        ];
        let since = Some(100 * 1000);
        let built = build(&notes, since);
        assert_eq!(
            closes(&notes, since),
            [
                ("q".to_owned(), Close::Replied(3)),
                ("b2".to_owned(), Close::Waiting)
            ]
        );
        assert_eq!(built.singles, [0, 1]);
        assert_eq!(built.unlinked, 1);
        // 時刻の設定が無ければ、前の頼みも待ちに数える。
        assert_eq!(closes(&notes, None)[0], ("b1".to_owned(), Close::Waiting));
    }

    #[test]
    fn 付け始めた時刻より前に済んだ課題は_dismiss_と_end_が在っても_往復にしない() {
        // 前の版の形: 返事（done・end）に返事の先が無い。依頼書には done が返っているので、「取り消した」と出すと、うその往復になる。
        let notes = [
            note("b", "brief", "tpm", "K", 0, None),
            note("q", "ask-tpm", "K", "tpm", 5, None),
            note("d", "done", "K", "tpm", 10, None),
            note("x", "dismiss", "tpm", "K", 20, None),
            note("e", "end", "K", "tpm", 30, None),
        ];
        let built = build(&notes, Some(100 * 1000));
        assert_eq!(built.exchanges, []);
        assert_eq!(built.singles, [0, 1, 2, 3, 4]);
        assert_eq!(built.unlinked, 2);
    }

    #[test]
    fn またぐ課題で_前の頼みは_後の_dismiss_や_end_で往復にならない_後の頼みは閉じる() {
        let notes = [
            note("b1", "brief", "tpm", "K", 0, None),
            note("d1", "done", "K", "tpm", 10, None),
            // ここから後（100 秒〜）。
            note("b2", "brief", "tpm", "K", 110, None),
            note("q2", "ask-devops", "K", "tpm", 120, None),
            note("x", "dismiss", "tpm", "K", 130, None),
            note("e", "end", "K", "tpm", 140, Some("x")),
        ];
        assert_eq!(
            closes(&notes, Some(100 * 1000)),
            [
                ("b2".to_owned(), Close::Cancelled(4)),
                ("q2".to_owned(), Close::Ended(5)),
                ("x".to_owned(), Close::Replied(5)),
            ]
        );
        assert_eq!(build(&notes, Some(100 * 1000)).singles, [0, 1]);
    }

    #[test]
    fn 知らない見出しは片道_ask_で始まる知らない見出しは頼み() {
        let notes = [
            note("x", "hello", "K", "tpm", 0, None),
            note("y", "ask-someone", "K", "tpm", 10, None),
        ];
        let built = build(&notes, None);
        assert_eq!(built.singles, [0]);
        assert_eq!(built.exchanges.len(), 1);
    }

    #[test]
    fn 球の持ち手は_頼みの受け手で決まる_ask_pm_は_product_manager() {
        let holder = |heading, to| Holder::of(&note("x", heading, "K", to, 0, None));
        assert_eq!(holder("ask-pm", "tpm"), Holder::Pm);
        assert_eq!(holder("ask-tpm", "tpm"), Holder::Tpm);
        assert_eq!(holder("ask-devops", "tpm"), Holder::Tpm);
        assert_eq!(holder("ask-review", "K.staff"), Holder::Staff);
        assert_eq!(holder("brief", "K"), Holder::Seat);
        assert_eq!(holder("dismiss", "K"), Holder::Seat);
    }

    #[test]
    fn 球の持ち手の帯は_重なりを_pm_staff_tpm_担当の順で塗り_隣の同じ持ち手をまとめ_待ちは今まで続く()
     {
        let notes = [
            note("b", "brief", "tpm", "K", 0, None),
            note("r", "ask-review", "K", "K.staff", 10, None),
            note("v", "review", "K.staff", "K", 20, Some("r")),
            note("d", "done", "K", "tpm", 30, Some("b")),
            // 誰も持っていない間（30〜40）。
            note("q", "ask-pm", "K", "tpm", 40, None),
        ];
        let built = build(&notes, None);
        let spans: Vec<(i64, i64, Holder)> = ball(&notes, &built.exchanges, 60_000)
            .into_iter()
            .map(|s| (s.from / 1000, s.to / 1000, s.holder))
            .collect();
        assert_eq!(
            spans,
            [
                (0, 10, Holder::Seat),
                (10, 20, Holder::Staff),
                (20, 30, Holder::Seat),
                (40, 60, Holder::Pm),
            ]
        );
    }
}
