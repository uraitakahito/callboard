//! 費え（トークンの数と額）の行を、全体・役割ごと・課題ごとにまとめる計算。外に触らない純な計算（DB も HTTP も触らない）。
//!
//! - 行は、castlist の `ListUsage` の行（アクター 1 人の、時計の 1 時間の内の、同じ種類の回のまとめ）。同じアクター・同じ時・
//!   同じ種類の行は 2 つ以上在りうるので、数は足し、いちばん長い文脈は大きい方を取り、段は番号ごとに足す。
//! - 本体の回（`helper` が false）と、手伝い（subagent）の回を分ける。文脈の長さの数（中央・上から 1 割・いちばん長い文脈・
//!   20 万以上の回）は、本体の回だけで出す（手伝いは自分の短い文脈で動くので、混ぜると中央が下がる）。
//! - 中央と上から 1 割は、段（2.5 万刻み）から出す近い値: 回を文脈の短い順に並べた時の、その順位の回が入る段の下の端。
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// 役割と課題を読むラベルの鍵。課題は `tpm.subject` を先に見て、無ければ `tpm.key`。
const ROLE: &str = "tpm.role";
const SUBJECT: &str = "tpm.subject";
const KEY: &str = "tpm.key";

/// 段の幅（トークン）。段 n は、文脈の長さが n×25,000 以上 (n+1)×25,000 未満（40 は 1,000,000 以上）。
pub const BUCKET_WIDTH: i64 = 25_000;

/// 最後の段（1,000,000 以上）。
pub const LAST_BUCKET: i32 = 40;

/// 20 万以上の回の、初めの段（8×25,000 = 200,000）。
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
    /// 本体の回の、段の回の数と額の計と、そのうち 20 万以上の段（[`OVER_BUCKET`] から）の分。
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

/// 段ごとの回の数と額（段の番号の順）。
type Buckets = BTreeMap<i32, (i64, i64)>;

/// 役割ごとに足していく物。
#[derive(Default)]
struct RoleSum<'a> {
    seats: BTreeSet<&'a str>,
    turns: i64,
    cost: i64,
    max_context: i64,
    helper_cost: i64,
    /// 本体の回の段。
    buckets: Buckets,
}

/// 課題ごとに足していく物。
#[derive(Default)]
struct SubjectSum<'a> {
    seats: BTreeSet<&'a str>,
    turns: i64,
    cost: i64,
    max_context: i64,
}

/// 回を文脈の短い順に並べた時の、下から `num / den` の所の回（順位は切り上げ）が入る段の、下の端（トークン）。回が 0 なら無い。
fn quantile(buckets: &Buckets, num: i64, den: i64) -> Option<i64> {
    let total = buckets
        .values()
        .fold(0i64, |sum, (turns, _)| sum.saturating_add(*turns));
    if total <= 0 {
        return None;
    }
    let rank = (total.saturating_mul(num).saturating_add(den - 1) / den).max(1);
    let mut seen = 0i64;
    for (bucket, (turns, _)) in buckets {
        seen = seen.saturating_add(*turns);
        if seen >= rank {
            return Some(i64::from(*bucket) * BUCKET_WIDTH);
        }
    }
    None
}

/// 額の大きい順。同じ額なら名の順で、名の無い物（「無し」）を後ろに。
fn by_cost<T>(items: &mut [T], cost: impl Fn(&T) -> i64, name: impl Fn(&T) -> &Option<String>) {
    items.sort_by(|a, b| {
        cost(b)
            .cmp(&cost(a))
            .then_with(|| (name(a).is_none(), name(a)).cmp(&(name(b).is_none(), name(b))))
    });
}

/// 行とアクターから、全体・役割ごと・課題ごとの数を出す。`actors` に無いアクターの行は、役割も課題も「無し」に入れる。
pub fn summarize(rows: &[Row], actors: &[Actor]) -> Summary {
    let labels: HashMap<&str, &HashMap<String, String>> =
        actors.iter().map(|a| (a.id.as_str(), &a.labels)).collect();
    let mut seats = BTreeSet::new();
    let mut total = Total::default();
    let mut roles: BTreeMap<Option<&str>, RoleSum> = BTreeMap::new();
    let mut subjects: BTreeMap<Option<&str>, SubjectSum> = BTreeMap::new();
    for row in rows {
        let id = row.actor_id.as_str();
        let label = |key: &str| labels.get(id).and_then(|l| l.get(key)).map(String::as_str);
        seats.insert(id);
        total.turns = total.turns.saturating_add(row.turns);
        total.cost = total.cost.saturating_add(row.cost);

        let role = roles.entry(label(ROLE)).or_default();
        role.seats.insert(id);
        let subject = subjects
            .entry(label(SUBJECT).or_else(|| label(KEY)))
            .or_default();
        subject.seats.insert(id);
        subject.turns = subject.turns.saturating_add(row.turns);
        subject.cost = subject.cost.saturating_add(row.cost);

        if row.helper {
            total.helper_turns = total.helper_turns.saturating_add(row.turns);
            total.helper_cost = total.helper_cost.saturating_add(row.cost);
            role.helper_cost = role.helper_cost.saturating_add(row.cost);
            continue;
        }
        // 文脈の長さの数は、本体の回だけ。
        role.turns = role.turns.saturating_add(row.turns);
        role.cost = role.cost.saturating_add(row.cost);
        role.max_context = role.max_context.max(row.max_context);
        subject.max_context = subject.max_context.max(row.max_context);
        for b in row
            .buckets
            .iter()
            .filter(|b| (0..=LAST_BUCKET).contains(&b.bucket))
        {
            let sum = role.buckets.entry(b.bucket).or_default();
            sum.0 = sum.0.saturating_add(b.turns);
            sum.1 = sum.1.saturating_add(b.cost);
        }
    }
    total.seats = seats.len();

    let mut roles: Vec<Role> = roles
        .into_iter()
        .map(|(name, sum)| {
            let add = |from: i32| {
                sum.buckets
                    .range(from..)
                    .fold((0i64, 0i64), |(turns, cost), (_, b)| {
                        (turns.saturating_add(b.0), cost.saturating_add(b.1))
                    })
            };
            let (bucket_turns, bucket_cost) = add(0);
            let (over_turns, over_cost) = add(OVER_BUCKET);
            Role {
                name: name.map(str::to_owned),
                seats: sum.seats.len(),
                turns: sum.turns,
                cost: sum.cost,
                median: quantile(&sum.buckets, 1, 2),
                p90: quantile(&sum.buckets, 9, 10),
                max_context: sum.max_context,
                bucket_turns,
                bucket_cost,
                over_turns,
                over_cost,
                helper_cost: sum.helper_cost,
            }
        })
        .collect();
    by_cost(&mut roles, |r| r.cost, |r| &r.name);
    let mut subjects: Vec<Subject> = subjects
        .into_iter()
        .map(|(name, sum)| Subject {
            name: name.map(str::to_owned),
            seats: sum.seats.len(),
            turns: sum.turns,
            cost: sum.cost,
            max_context: sum.max_context,
        })
        .collect();
    by_cost(&mut subjects, |s| s.cost, |s| &s.name);
    Summary {
        total,
        roles,
        subjects,
    }
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
