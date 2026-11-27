//! 費え（トークンの数と額）の行を、全体・役割ごと・課題ごとにまとめる計算。外に触らない純な計算（DB も HTTP も触らない）。
//!
//! - 行は、castlist の `ListUsage` の行（アクター 1 人の、時計の 1 時間の内の、同じ種類の回のまとめ）。同じアクター・同じ時・
//!   同じ種類の行は 2 つ以上在りうるので、数は足し、いちばん長い文脈は大きい方を取り、段は番号ごとに足す。
//! - 本体の回（`helper` が false）と、手伝い（subagent）の回を分ける。文脈の長さの数（中央・上から 1 割・いちばん長い文脈・
//!   20 万を越えた回）は、本体の回だけで出す（手伝いは自分の短い文脈で動くので、混ぜると中央が下がる）。
//! - 中央と上から 1 割は、段（2.5 万刻み）から出す近い値: 回を文脈の短い順に並べた時の、その順位の回が入る段の下の端。
// 形だけを先に置いた（試験を先に書く）。実装の commit で外す。
#![allow(dead_code)]

use std::collections::HashMap;

/// 段の幅（トークン）。段 n は、文脈の長さが n×25,000 以上 (n+1)×25,000 未満（40 は 1,000,000 以上）。
pub const BUCKET_WIDTH: i64 = 25_000;

/// 20 万を越えた回の、初めの段（8×25,000 = 200,000）。
pub const OVER_BUCKET: i32 = 8;

/// 文脈の長さの段 1 つの、回の数と額（100 万分の 1 ドル）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bucket {
    pub bucket: i32,
    pub turns: i64,
    pub cost: i64,
}

/// 数の 1 行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub actor_id: String,
    /// 手伝い（subagent）の回か（castlist の行に `agent` が在る）。
    pub helper: bool,
    pub turns: i64,
    /// 額（100 万分の 1 ドル）。
    pub cost: i64,
    /// いちばん長い文脈（トークン）。
    pub max_context: i64,
    pub buckets: Vec<Bucket>,
}

/// 行に出てきたアクター（役割と課題は、ラベルから読む）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Actor {
    pub id: String,
    pub labels: HashMap<String, String>,
}

/// まとめた物。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub total: Total,
    /// 役割ごと（本体の額の大きい順）。
    pub roles: Vec<Role>,
    /// 課題ごと（額の大きい順）。
    pub subjects: Vec<Subject>,
}

/// 全体。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Total {
    /// 行のあるアクターの数。
    pub seats: usize,
    /// 回の数と額（本体と手伝いの計）。
    pub turns: i64,
    pub cost: i64,
    pub helper_turns: i64,
    pub helper_cost: i64,
}

/// 役割（ラベル `tpm.role`）ごと。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Role {
    /// 役割の名。ラベルが無ければ無い。
    pub name: Option<String>,
    pub seats: usize,
    /// 本体の回の数と額。
    pub turns: i64,
    pub cost: i64,
    /// 文脈の長さの中央と、上から 1 割（その回が入る段の下の端。トークン）。本体の回が無ければ無い。
    pub median: Option<i64>,
    pub p90: Option<i64>,
    /// 本体の回の、いちばん長い文脈。
    pub max_context: i64,
    /// 本体の回の、段の回の数と額の計と、そのうち 20 万を越えた段（[`OVER_BUCKET`] から）の分。
    pub bucket_turns: i64,
    pub bucket_cost: i64,
    pub over_turns: i64,
    pub over_cost: i64,
    /// 手伝いの回の額。
    pub helper_cost: i64,
}

/// 課題（ラベル `tpm.subject`、無ければ `tpm.key`）ごと。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subject {
    /// 課題の名。どちらのラベルも無ければ無い。
    pub name: Option<String>,
    pub seats: usize,
    /// 回の数と額（本体と手伝いの計）。
    pub turns: i64,
    pub cost: i64,
    /// 本体の回の、いちばん長い文脈。
    pub max_context: i64,
}

/// 行とアクターから、全体・役割ごと・課題ごとの数を出す。`actors` に無いアクターの行は、役割も課題も「無し」に入れる。
pub fn summarize(_rows: &[Row], _actors: &[Actor]) -> Summary {
    Summary::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(id: &str, labels: &[(&str, &str)]) -> Actor {
        Actor {
            id: id.to_owned(),
            labels: labels
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    /// 本体の回の行。額は、段の額の計。
    fn main(actor_id: &str, max_context: i64, buckets: &[(i32, i64, i64)]) -> Row {
        Row {
            actor_id: actor_id.to_owned(),
            helper: false,
            turns: buckets.iter().map(|b| b.1).sum(),
            cost: buckets.iter().map(|b| b.2).sum(),
            max_context,
            buckets: buckets
                .iter()
                .map(|&(bucket, turns, cost)| Bucket {
                    bucket,
                    turns,
                    cost,
                })
                .collect(),
        }
    }

    fn helper(actor_id: &str, max_context: i64, buckets: &[(i32, i64, i64)]) -> Row {
        Row {
            helper: true,
            ..main(actor_id, max_context, buckets)
        }
    }

    #[test]
    fn 行が無ければ_どの数も_0_で_表は空() {
        assert_eq!(summarize(&[], &[]), Summary::default());
        // アクターだけ在って行が無くても、数えない。
        let summary = summarize(&[], &[actor("a", &[("tpm.role", "se")])]);
        assert_eq!(summary, Summary::default());
    }

    #[test]
    fn 中央と上から_1_割は_その順位の回が入る段の下の端() {
        let actors = [actor("a", &[("tpm.role", "se")])];
        // 10 回: 段 0 に 1・段 1 に 1・段 2 に 2・段 4 に 4・段 9 に 2。5 番目は段 4、9 番目は段 9。
        let rows = [
            main("a", 60_000, &[(0, 1, 10), (1, 1, 10), (2, 2, 20)]),
            main("a", 230_000, &[(4, 4, 40), (9, 2, 20)]),
        ];
        let role = &summarize(&rows, &actors).roles[0];
        assert_eq!(role.median, Some(100_000));
        assert_eq!(role.p90, Some(225_000));
        // 1 回だけなら、どちらもその回の段。段 40（100 万以上）の下の端は 100 万。
        let role = &summarize(&[main("a", 1_200_000, &[(40, 1, 5)])], &actors).roles[0];
        assert_eq!(role.median, Some(1_000_000));
        assert_eq!(role.p90, Some(1_000_000));
        // 2 回なら、中央は下の回（1 番目）、上から 1 割は上の回（2 番目）。
        let role = &summarize(&[main("a", 60_000, &[(0, 1, 5), (2, 1, 5)])], &actors).roles[0];
        assert_eq!(role.median, Some(0));
        assert_eq!(role.p90, Some(50_000));
        // 段は、行に並んだ順でなく番号の順に見る。
        let role = &summarize(&[main("a", 60_000, &[(2, 1, 5), (0, 2, 5)])], &actors).roles[0];
        assert_eq!(role.median, Some(0));
        assert_eq!(role.p90, Some(50_000));
    }

    #[test]
    fn 同じ時の行は_数を足し_いちばん長い文脈は大きい方_段は番号ごとに足す() {
        let actors = [actor("a", &[("tpm.role", "se"), ("tpm.key", "t-1")])];
        let rows = [
            main("a", 61_000, &[(1, 1, 500), (2, 2, 1_000)]),
            main("a", 230_000, &[(2, 1, 300), (9, 2, 700)]),
            main("a", 40_000, &[(1, 1, 100)]),
        ];
        let summary = summarize(&rows, &actors);
        assert_eq!(summary.total.seats, 1);
        assert_eq!(summary.total.turns, 7);
        assert_eq!(summary.total.cost, 2_600);
        let role = &summary.roles[0];
        assert_eq!((role.seats, role.turns, role.cost), (1, 7, 2_600));
        assert_eq!(role.max_context, 230_000);
        // 段 1 に 2・段 2 に 3・段 9 に 2。4 番目は段 2、7 番目は段 9。
        assert_eq!(role.median, Some(50_000));
        assert_eq!(role.p90, Some(225_000));
        assert_eq!(summary.subjects[0].max_context, 230_000);
    }

    #[test]
    fn 二十万を越えた回は_段_8_からの回の数と額() {
        let actors = [actor("a", &[("tpm.role", "se")])];
        // 段 7（17.5 万〜20 万）は越えていない。段 8（20 万〜）から。
        let rows = [main(
            "a",
            990_000,
            &[(0, 5, 50), (7, 2, 200), (8, 1, 300), (39, 2, 450)],
        )];
        let role = &summarize(&rows, &actors).roles[0];
        assert_eq!((role.bucket_turns, role.over_turns), (10, 3));
        assert_eq!((role.bucket_cost, role.over_cost), (1_000, 750));
        // 0〜40 の外の段は数えない。
        let role = &summarize(
            &[main("a", 10, &[(41, 3, 30), (-1, 1, 10), (0, 1, 5)])],
            &actors,
        )
        .roles[0];
        assert_eq!((role.bucket_turns, role.over_turns), (1, 0));
        assert_eq!(role.median, Some(0));
    }

    #[test]
    fn 本体と手伝いを分け_文脈の長さの数は本体だけで出す() {
        let actors = [actor("a", &[("tpm.role", "se"), ("tpm.key", "t-1")])];
        let rows = [
            main("a", 120_000, &[(4, 2, 2_000)]),
            // 手伝いは短い文脈で多く回る。混ぜると、中央は段 0 になる。
            helper("a", 900_000, &[(0, 8, 800), (36, 1, 200)]),
        ];
        let summary = summarize(&rows, &actors);
        assert_eq!(
            summary.total,
            Total {
                seats: 1,
                turns: 11,
                cost: 3_000,
                helper_turns: 9,
                helper_cost: 1_000,
            }
        );
        let role = &summary.roles[0];
        assert_eq!((role.turns, role.cost, role.helper_cost), (2, 2_000, 1_000));
        assert_eq!(role.median, Some(100_000));
        assert_eq!(role.p90, Some(100_000));
        assert_eq!(role.max_context, 120_000);
        assert_eq!((role.bucket_turns, role.over_turns), (2, 0));
        // 課題ごとの回の数と額は計。いちばん長い文脈は本体だけ。
        let subject = &summary.subjects[0];
        assert_eq!((subject.turns, subject.cost), (11, 3_000));
        assert_eq!(subject.max_context, 120_000);

        // 手伝いの回だけの役割は、文脈の長さの数が無い。
        let summary = summarize(&[helper("a", 30_000, &[(1, 4, 400)])], &actors);
        let role = &summary.roles[0];
        assert_eq!((role.seats, role.turns, role.cost), (1, 0, 0));
        assert_eq!((role.median, role.p90, role.max_context), (None, None, 0));
        assert_eq!(role.helper_cost, 400);
    }

    #[test]
    fn 役割と課題はラベルで分け_課題は_tpm_subject_を先に見て_額の大きい順に並べる() {
        let actors = [
            actor(
                "se1",
                &[
                    ("tpm.role", "se"),
                    ("tpm.subject", "t-new"),
                    ("tpm.key", "t-old"),
                ],
            ),
            actor("se2", &[("tpm.role", "se"), ("tpm.key", "t-key")]),
            actor("tpm", &[("tpm.role", "tpm")]),
            actor("pm", &[]),
        ];
        let rows = [
            main("se1", 10, &[(0, 1, 100)]),
            main("se2", 10, &[(0, 1, 200)]),
            main("tpm", 10, &[(0, 1, 5_000)]),
            main("pm", 10, &[(0, 1, 10)]),
            // actors に無いアクターの行は、役割も課題も「無し」。
            main("gone", 10, &[(0, 1, 20)]),
        ];
        let summary = summarize(&rows, &actors);
        assert_eq!(summary.total.seats, 5);
        let roles: Vec<(Option<&str>, usize, i64)> = summary
            .roles
            .iter()
            .map(|r| (r.name.as_deref(), r.seats, r.cost))
            .collect();
        assert_eq!(
            roles,
            [(Some("tpm"), 1, 5_000), (Some("se"), 2, 300), (None, 2, 30)]
        );
        let subjects: Vec<(Option<&str>, usize, i64)> = summary
            .subjects
            .iter()
            .map(|s| (s.name.as_deref(), s.seats, s.cost))
            .collect();
        assert_eq!(
            subjects,
            [
                (None, 3, 5_030),
                (Some("t-key"), 1, 200),
                (Some("t-new"), 1, 100),
            ]
        );
    }

    #[test]
    fn 額が同じなら_名の順で_無しを後ろに() {
        let actors = [
            actor("b", &[("tpm.role", "b"), ("tpm.key", "k-b")]),
            actor("a", &[("tpm.role", "a"), ("tpm.key", "k-a")]),
            actor("n", &[]),
        ];
        let rows = [
            main("n", 10, &[(0, 1, 100)]),
            main("b", 10, &[(0, 1, 100)]),
            main("a", 10, &[(0, 1, 100)]),
        ];
        let summary = summarize(&rows, &actors);
        let roles: Vec<Option<&str>> = summary.roles.iter().map(|r| r.name.as_deref()).collect();
        assert_eq!(roles, [Some("a"), Some("b"), None]);
        let subjects: Vec<Option<&str>> =
            summary.subjects.iter().map(|s| s.name.as_deref()).collect();
        assert_eq!(subjects, [Some("k-a"), Some("k-b"), None]);
    }
}
