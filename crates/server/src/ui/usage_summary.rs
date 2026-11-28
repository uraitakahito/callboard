//! 使用量（トークンの数と額）の行を、全体・役割ごと・課題ごとにまとめる計算。外に触らない純な計算（DB も HTTP も触らない）。
//!
//! - 行は、castlist の `ListUsage` の行（アクター 1 人の、同じ種類の回のまとめ。区間は UTC の時の内で、長さは送り手が決める:
//!   roster は UTC の 10 分の枠ごと。前の行は 1 時間の枠ごと）。同じアクター・同じ枠・同じ種類の行は 2 つ以上在りうるので、数は足し、いちばん長い文脈は大きい方を取り、段は番号ごとに足す。
//! - 本体の回（`agent` が無い）と、手伝い（subagent）の回（`agent` が手伝いの種類）を分ける。文脈の長さの数（中央・上から 1 割・いちばん長い文脈・
//!   20 万以上の回）は、本体の回だけで出す（手伝いは自分の短い文脈で動くので、混ぜると中央が下がる）。
//! - 区切り（日・時・10 分）ごとの額は、行の区間が区切りに収まれば正確に入れ、境をまたぐ行は時間の比で配る（見立て。[`spread`]）。
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
    /// 区間の始めの時刻（castlist の `fromAt`。行にまとめた回の、初めの回の時刻。UNIX の時からの秒。秒より下は切り捨て）。
    pub from_at: i64,
    /// 区間の終わりの時刻（castlist の `toAt`。行にまとめた回の、終わりの回の時刻。UNIX の時からの秒。秒より下は切り捨て）。
    pub to_at: i64,
    /// model の名（castlist の行の `model`）。
    pub model: String,
    /// 手伝い（subagent）の種類の名（castlist の行の `agent`）。本体の回なら無い。手伝いかは [`Row::helper`] で見る。
    pub agent: Option<String>,
    pub turns: i64,
    /// 額（100 万分の 1 ドル）。
    pub cost: i64,
    /// いちばん長い文脈（トークン）。
    pub max_context: i64,
    pub buckets: Vec<Bucket>,
}

impl Row {
    /// 手伝い（subagent）の回か（castlist の行に `agent` が在る）。
    pub fn helper(&self) -> bool {
        self.agent.is_some()
    }
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

        if row.helper() {
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

/// 段ごとの、本体の回の数と額（添字が段の番号。0 から [`LAST_BUCKET`] までの 41 個）。手伝いの回は混ぜない（文脈の長さの数は本体だけ）。
pub fn by_bucket(rows: &[Row]) -> Vec<(i64, i64)> {
    let mut out = vec![(0i64, 0i64); usize::try_from(LAST_BUCKET).unwrap_or(0) + 1];
    for b in rows.iter().filter(|r| !r.helper()).flat_map(|r| &r.buckets) {
        if let Some(sum) = usize::try_from(b.bucket).ok().and_then(|i| out.get_mut(i)) {
            sum.0 = sum.0.saturating_add(b.turns);
            sum.1 = sum.1.saturating_add(b.cost);
        }
    }
    out
}

/// 区切り 1 つの額（100 万分の 1 ドル）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotSum {
    /// 本体の額。
    pub main: i64,
    /// 手伝いの額。
    pub helper: i64,
    /// `main + helper` のうち、区切りの境をまたぐ行から、時間で均して配った額（見立て）。0 なら、この区切りの額は正確。
    pub estimated: i64,
}

impl SlotSum {
    /// 本体と手伝いの計。
    pub fn total(&self) -> i64 {
        self.main.saturating_add(self.helper)
    }
}

/// 行 1 つの額を、区切りに配る。`starts` は区切りの初めの時刻（秒。昇順）、`end` は最後の区切りの終わり。返すのは、区切りの
/// 添字と、そこに入る額と、見立てか（行の区間が区切りの境をまたぐ）。区切りの外に当たる分は返さない。
///
/// - 行の区間（`from_at`〜`to_at`。終わりの秒も含む）が 1 つの区切りに収まれば、額をみなその区切りに入れる（正確）。
///   castlist の行は UTC の時をまたがないので、時ごと・日ごとの区切りでは、いつもこれになる。
/// - 境をまたぐ行（10 分で切る前の記録を、10 分の区切りで見た時）は、行の回がどの区切りに在ったかが分からないので、
///   区間と区切りの重なる秒の比で配る（見立て）。端数は、余りの大きい区切りへ（同じなら早い方へ）。配った額の和は、元の額。
pub fn spread_row(row: &Row, starts: &[i64], end: i64) -> Vec<(usize, i64, bool)> {
    let Some(&first) = starts.first() else {
        return Vec::new();
    };
    // 行の区間（終わりの秒を含む。始めが終わりより後の行は、終わりの 1 秒とみなす）。
    let (a, b) = (row.from_at.min(row.to_at), row.to_at.saturating_add(1));
    if b <= first || a >= end {
        return Vec::new();
    }
    let slot_of = |t: i64| starts.partition_point(|s| *s <= t).checked_sub(1);
    let (lo, hi) = (slot_of(a), slot_of(b - 1));
    if a >= first && b <= end && lo == hi {
        return lo.map(|i| (i, row.cost, false)).into_iter().collect();
    }
    // 区切りごとの重なり（秒）。区切りの外（初めより前・終わりより後）の分は、配る先が無い（捨てる）。
    let total = i128::from(b - a);
    let cost = i128::from(row.cost.max(0));
    let mut parts: Vec<(Option<usize>, i128)> = Vec::new();
    if a < first {
        parts.push((None, i128::from(first - a)));
    }
    for i in lo.unwrap_or(0)..=hi.unwrap_or(0).min(starts.len() - 1) {
        let (s, e) = (starts[i], starts.get(i + 1).copied().unwrap_or(end));
        let overlap = b.min(e) - a.max(s);
        if overlap > 0 {
            parts.push((Some(i), i128::from(overlap)));
        }
    }
    if b > end {
        parts.push((None, i128::from(b - end)));
    }
    // 切り捨てた額を配り、足りない分（端数の和）を、余りの大きい順に 1 ずつ足す。
    let mut shares: Vec<(Option<usize>, i128, i128)> = parts
        .iter()
        .map(|(i, overlap)| (*i, cost * overlap / total, cost * overlap % total))
        .collect();
    let mut left = cost - shares.iter().map(|s| s.1).sum::<i128>();
    let mut order: Vec<usize> = (0..shares.len()).collect();
    order.sort_by(|x, y| shares[*y].2.cmp(&shares[*x].2).then(x.cmp(y)));
    for at in order {
        if left <= 0 {
            break;
        }
        shares[at].1 += 1;
        left -= 1;
    }
    shares
        .into_iter()
        .filter_map(|(i, share, _)| Some((i?, i64::try_from(share).unwrap_or(i64::MAX), true)))
        .filter(|(_, share, _)| *share > 0)
        .collect()
}

/// 区切り（日・時・10 分）ごとの、本体の額と手伝いの額と、そのうちの見立ての額（[`spread_row`] で配る）。
pub fn spread(rows: &[Row], starts: &[i64], end: i64) -> Vec<SlotSum> {
    let mut out = vec![SlotSum::default(); starts.len()];
    for row in rows {
        for (i, share, estimated) in spread_row(row, starts, end) {
            let sum = &mut out[i];
            if row.helper() {
                sum.helper = sum.helper.saturating_add(share);
            } else {
                sum.main = sum.main.saturating_add(share);
            }
            if estimated {
                sum.estimated = sum.estimated.saturating_add(share);
            }
        }
    }
    out
}

/// 区切りごと × 役割の升の 1 行（役割 1 つ）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleLine {
    /// 役割の名（アクターのラベル `tpm.role`）。ラベルの無い席・`actors` に無い席は無い（「無し」）。
    pub name: Option<String>,
    /// 区切りごとの額（本体と手伝いの計）と、そのうちの見立ての額。添字は区切り。
    pub cells: Vec<(i64, i64)>,
    /// 行の額の計。
    pub total: i64,
}

/// 区切り（日・時・10 分）ごと × 役割の額。額は [`spread_row`] で配り、**手伝い（subagent）の額は、起こした席の役割に入れる**
/// （「どの役割が使ったか」を見るため。「model と役割ごと」の行列は、手伝いを別の行に分ける）。額の在る役割だけを、
/// 額（本体と手伝いの計）の大きい順に並べる（同じ額なら名の順で、「無し」は後ろ。「役割ごと」の表と同じ決まり）。
pub fn by_slot_role(rows: &[Row], actors: &[Actor], starts: &[i64], end: i64) -> Vec<RoleLine> {
    let labels: HashMap<&str, &HashMap<String, String>> =
        actors.iter().map(|a| (a.id.as_str(), &a.labels)).collect();
    let mut lines: BTreeMap<Option<String>, RoleLine> = BTreeMap::new();
    for row in rows {
        let role = labels
            .get(row.actor_id.as_str())
            .and_then(|l| l.get(ROLE))
            .cloned();
        for (i, share, estimated) in spread_row(row, starts, end) {
            let line = lines.entry(role.clone()).or_insert_with(|| RoleLine {
                name: role.clone(),
                cells: vec![(0, 0); starts.len()],
                total: 0,
            });
            let cell = &mut line.cells[i];
            cell.0 = cell.0.saturating_add(share);
            if estimated {
                cell.1 = cell.1.saturating_add(share);
            }
            line.total = line.total.saturating_add(share);
        }
    }
    let mut lines: Vec<RoleLine> = lines.into_values().filter(|l| l.total > 0).collect();
    by_cost(&mut lines, |l| l.total, |l| &l.name);
    lines
}

/// 升の地の色の段の境（升の額の、その図でいちばん多い升の額に対する比の、分母）。[0] 分の 1 以上は段 3、[1] 分の 1 以上は段 2、
/// 0 より大きければ段 1。「model と役割ごと」の行列の境（全体の額に対する比。[`HEAT`]）とは違う: 升は区切りの数だけ細かく
/// 割れ、全体に対する比では、みな下の段になるため。色は同じ 3 色。
pub const SLOT_HEAT: [i64; 2] = [2, 10];

/// 升の地の色の段（1〜3）。額が 0 以下か、いちばん多い升の額が 0 以下なら無い。
pub fn slot_heat(cost: i64, max: i64) -> Option<u8> {
    if cost <= 0 || max <= 0 {
        return None;
    }
    let (cost, max) = (i128::from(cost), i128::from(max));
    Some(if cost * i128::from(SLOT_HEAT[0]) >= max {
        3
    } else if cost * i128::from(SLOT_HEAT[1]) >= max {
        2
    } else {
        1
    })
}

/// 席（アクター）1 つの数。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Seat {
    /// 額（本体と手伝いの計）。
    pub cost: i64,
    /// 本体の回の、いちばん長い文脈（本体の回が無ければ 0）。
    pub max_context: i64,
}

/// 席ごとの、額（本体と手伝いの計）といちばん長い文脈（本体の回だけ）。
pub fn by_seat(rows: &[Row]) -> BTreeMap<String, Seat> {
    let mut out: BTreeMap<String, Seat> = BTreeMap::new();
    for row in rows {
        let seat = out.entry(row.actor_id.clone()).or_default();
        seat.cost = seat.cost.saturating_add(row.cost);
        if !row.helper() {
            seat.max_context = seat.max_context.max(row.max_context);
        }
    }
    out
}

/// 時（`to_at` を時の初めに切り下げた時刻。秒）ごとの、本体の回のいちばん長い文脈（時の順）。本体の回の無い時は入れない。
pub fn context_by_hour(rows: &[Row]) -> Vec<(i64, i64)> {
    let mut hours: BTreeMap<i64, i64> = BTreeMap::new();
    for row in rows.iter().filter(|r| !r.helper()) {
        let hour = row.to_at - row.to_at.rem_euclid(3600);
        let max = hours.entry(hour).or_default();
        *max = (*max).max(row.max_context);
    }
    hours.into_iter().collect()
}

/// 行列の地の色の段の境（升の額の、全体の額に対する比の %。下の端）。[0] 以上は段 3、[1] 以上は段 2、0 より大きければ段 1。
pub const HEAT: [i64; 2] = [10, 1];

/// 行列の升 1 つ（回の数と額。額は 100 万分の 1 ドル）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub turns: i64,
    pub cost: i64,
}

/// 行列の行。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MatrixRow {
    /// 上の組は役割の名（ラベル `tpm.role`。無ければ無い）、下の組は手伝いの種類の名。
    pub name: Option<String>,
    /// 列（[`Matrix::models`]）の順の升。その行に、その model の行が無ければ無い。
    pub cells: Vec<Option<Cell>>,
    /// 行の計。
    pub total: Cell,
    /// 上の組だけ: その行の本体の回を持つ席の、最新の effort ごとの席の数（席の多い順、同じなら名の順）。effort の分からない席は数えない。
    pub efforts: Vec<(String, usize)>,
    /// 上の組だけ: その行の本体の回を持つ席の数（effort の分からない席も入れる）。下の組は 0。
    pub seats: usize,
}

/// 「役割（本体）と手伝いの種類」×「model」の行列。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matrix {
    /// 列: model の名と、その列の計（本体と手伝い）。列の額の大きい順、同じなら名の順。
    pub models: Vec<(String, Cell)>,
    /// 上の組: 本体の回を、役割ごとに（役割ごとの表と同じ並び: 本体の額の大きい順・同じなら名の順・「無し」は後ろ）。
    pub roles: Vec<MatrixRow>,
    /// 下の組: 手伝いの回を、手伝いの種類ごとに（役割では分けない。額の大きい順・同じなら名の順）。
    pub helpers: Vec<MatrixRow>,
    /// 全体の計。
    pub total: Cell,
}

/// 升の地の色の段（1〜3）。全体の額が 0 か、升の額が 0 なら無い。
pub fn heat(cost: i64, whole: i64) -> Option<u8> {
    if whole <= 0 || cost <= 0 {
        return None;
    }
    // 比（%）と境を、割らずに比べる（cost × 100 ≥ whole × 境）。i128 なので溢れない。
    let part = i128::from(cost) * 100;
    let at = |percent: i64| i128::from(whole) * i128::from(percent);
    Some(if part >= at(HEAT[0]) {
        3
    } else if part >= at(HEAT[1]) {
        2
    } else {
        1
    })
}

/// 行とアクターと、席の id ごとの最新の effort から、行列を作る。`actors` に無いアクターの行は、役割「無し」に入れる。
pub fn by_model(rows: &[Row], actors: &[Actor], efforts: &HashMap<String, String>) -> Matrix {
    let labels: HashMap<&str, &HashMap<String, String>> =
        actors.iter().map(|a| (a.id.as_str(), &a.labels)).collect();
    let mut total = Cell::default();
    let mut columns: BTreeMap<&str, Cell> = BTreeMap::new();
    let mut roles: BTreeMap<Option<&str>, LineSum> = BTreeMap::new();
    let mut helpers: BTreeMap<&str, LineSum> = BTreeMap::new();
    for row in rows {
        total.add(row);
        columns.entry(row.model.as_str()).or_default().add(row);
        let line = match &row.agent {
            Some(agent) => helpers.entry(agent.as_str()).or_default(),
            None => {
                let role = labels
                    .get(row.actor_id.as_str())
                    .and_then(|l| l.get(ROLE))
                    .map(String::as_str);
                let line = roles.entry(role).or_default();
                line.seats.insert(row.actor_id.as_str());
                line
            }
        };
        line.total.add(row);
        line.cells.entry(row.model.as_str()).or_default().add(row);
    }

    let mut models: Vec<(String, Cell)> = columns
        .into_iter()
        .map(|(model, cell)| (model.to_owned(), cell))
        .collect();
    // 額の大きい順、同じなら名の順（BTreeMap の順を、並べ替えで崩さない）。
    models.sort_by_key(|a| std::cmp::Reverse(a.1.cost));
    let order: Vec<&str> = models.iter().map(|(m, _)| m.as_str()).collect();
    let line = |name: Option<&str>, sum: LineSum, efforts: Vec<(String, usize)>| MatrixRow {
        name: name.map(str::to_owned),
        cells: order.iter().map(|m| sum.cells.get(m).copied()).collect(),
        total: sum.total,
        seats: sum.seats.len(),
        efforts,
    };

    let mut role_rows: Vec<MatrixRow> = roles
        .into_iter()
        .map(|(name, sum)| {
            let seats = count_efforts(&sum.seats, efforts);
            line(name, sum, seats)
        })
        .collect();
    by_cost(&mut role_rows, |r| r.total.cost, |r| &r.name);
    let mut helper_rows: Vec<MatrixRow> = helpers
        .into_iter()
        .map(|(name, sum)| line(Some(name), sum, Vec::new()))
        .collect();
    by_cost(&mut helper_rows, |r| r.total.cost, |r| &r.name);
    Matrix {
        models,
        roles: role_rows,
        helpers: helper_rows,
        total,
    }
}

impl Cell {
    fn add(&mut self, row: &Row) {
        self.turns = self.turns.saturating_add(row.turns);
        self.cost = self.cost.saturating_add(row.cost);
    }
}

/// 行列の行 1 つに足していく物。
#[derive(Default)]
struct LineSum<'a> {
    /// model ごとの升。
    cells: BTreeMap<&'a str, Cell>,
    total: Cell,
    /// 行の回を持つ席（上の組だけ使う）。
    seats: BTreeSet<&'a str>,
}

/// 席の、最新の effort ごとの数（席の多い順、同じなら名の順）。effort の分からない席は数えない。
fn count_efforts(
    seats: &BTreeSet<&str>,
    efforts: &HashMap<String, String>,
) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for effort in seats.iter().filter_map(|id| efforts.get(*id)) {
        *counts.entry(effort.as_str()).or_default() += 1;
    }
    let mut out: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(effort, n)| (effort.to_owned(), n))
        .collect();
    // 安定な並べ替えなので、同じ数の間は名の順のまま。
    out.sort_by_key(|a| std::cmp::Reverse(a.1));
    out
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
            from_at: 0,
            to_at: 0,
            model: "claude-opus-5-5".to_owned(),
            agent: None,
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
            agent: Some("Explore".to_owned()),
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

    /// 区切りごとの、本体の額と手伝いの額（[`spread`] の、見立ての額を落とした物）。
    fn by_slot(rows: &[Row], starts: &[i64], end: i64) -> Vec<(i64, i64)> {
        spread(rows, starts, end)
            .into_iter()
            .map(|s| (s.main, s.helper))
            .collect()
    }

    fn at(row: Row, to_at: i64) -> Row {
        Row {
            from_at: to_at,
            to_at,
            ..row
        }
    }

    #[test]
    fn 段ごとの額は_本体の回の段を番号ごとに足し_手伝いは混ぜない() {
        let rows = [
            main("a", 61_000, &[(1, 1, 500), (2, 2, 1_000)]),
            main("b", 230_000, &[(2, 1, 300), (9, 2, 700)]),
            helper("a", 900_000, &[(1, 8, 800), (36, 1, 200)]),
            // 0〜40 の外の段は数えない。
            main("a", 10, &[(41, 3, 30), (-1, 1, 10), (40, 1, 5)]),
        ];
        let buckets = by_bucket(&rows);
        assert_eq!(buckets.len(), 41);
        assert_eq!(buckets[1], (1, 500));
        assert_eq!(buckets[2], (3, 1_300));
        assert_eq!(buckets[9], (2, 700));
        assert_eq!(buckets[36], (0, 0));
        assert_eq!(buckets[40], (1, 5));
        assert_eq!(buckets.iter().map(|b| b.1).sum::<i64>(), 2_505);
        // 行が無くても、41 の段。
        assert_eq!(by_bucket(&[]), vec![(0, 0); 41]);
    }

    #[test]
    fn 区切りごとの額は_区間の終わりの時刻で入れ_本体と手伝いを分ける() {
        let rows = [
            at(main("a", 10, &[(0, 1, 10)]), 50),
            at(helper("a", 10, &[(0, 1, 5)]), 150),
            at(main("b", 10, &[(0, 1, 3)]), 100),
            at(main("b", 10, &[(0, 1, 1)]), 299),
            // 窓の外（終わりちょうどと、初めより前）は数えない。
            at(main("b", 10, &[(0, 1, 99)]), 300),
            at(main("b", 10, &[(0, 1, 77)]), -1),
        ];
        assert_eq!(
            by_slot(&rows, &[0, 100, 200], 300),
            [(10, 0), (3, 5), (1, 0)]
        );
        // 行が無ければ、どの区切りも 0。
        assert_eq!(by_slot(&[], &[0, 100], 200), [(0, 0), (0, 0)]);
    }

    /// 区間（始めと終わりの秒）と額を決めた、本体の行。
    fn span(from_at: i64, to_at: i64, cost: i64) -> Row {
        Row {
            from_at,
            to_at,
            cost,
            ..main("a", 10, &[(0, 1, cost)])
        }
    }

    #[test]
    fn 区切りに収まる行は正確に入れ_境をまたぐ行は_重なる時間の比で配って見立てにする() {
        // 10 分の区切り 3 つ（0〜600・600〜1200・1200〜1800）。
        let starts = [0, 600, 1200];
        // 収まる行: 区間の終わりの秒まで同じ区切り（599 は初めの区切り）。
        assert_eq!(
            spread_row(&span(0, 599, 100), &starts, 1800),
            [(0, 100, false)]
        );
        assert_eq!(
            spread_row(&span(700, 700, 7), &starts, 1800),
            [(1, 7, false)]
        );
        // 境をまたぐ行（300〜899 の 600 秒。初めの区切りに 300 秒・次に 300 秒）は、半分ずつの見立て。
        assert_eq!(
            spread_row(&span(300, 899, 100), &starts, 1800),
            [(0, 50, true), (1, 50, true)]
        );
        // 終わりがちょうど境の秒（300〜600）の行も、またぐ（600 の秒は次の区切り）。次の区切りには 1 秒の分だけ。
        assert_eq!(
            spread_row(&span(300, 600, 301), &starts, 1800),
            [(0, 300, true), (1, 1, true)]
        );
        // 1 時間の行（0〜1799）を 3 つの区切りに。端数は余りの大きい区切りへ（同じなら早い方へ）で、和は元の額。
        let parts = spread_row(&span(0, 1799, 100), &starts, 1800);
        assert_eq!(parts, [(0, 34, true), (1, 33, true), (2, 33, true)]);
        assert_eq!(parts.iter().map(|p| p.1).sum::<i64>(), 100);
        // 区切りの外に当たる分は、返さない（窓の端をまたぐ行。300〜899 のうち 600 から後だけが窓の内）。
        assert_eq!(
            spread_row(&span(300, 899, 100), &[600, 1200], 1800),
            [(0, 50, true)]
        );
        assert_eq!(
            spread_row(&span(1500, 2099, 100), &starts, 1800),
            [(2, 50, true)]
        );
        // 窓の外の行と、区切りが無い時は、何も返さない。
        assert!(spread_row(&span(1800, 1900, 5), &starts, 1800).is_empty());
        assert!(spread_row(&span(-50, -1, 5), &starts, 1800).is_empty());
        assert!(spread_row(&span(0, 10, 5), &[], 0).is_empty());
    }

    #[test]
    fn 区切りごとの額は_見立ての額を別に数え_時をまたがない行は_時ごとでも日ごとでも正確() {
        let rows = [
            span(0, 599, 100),
            Row {
                agent: Some("Explore".to_owned()),
                ..span(300, 899, 40)
            },
            span(1200, 1300, 9),
        ];
        assert_eq!(
            spread(&rows, &[0, 600, 1200], 1800),
            [
                SlotSum {
                    main: 100,
                    helper: 20,
                    estimated: 20
                },
                SlotSum {
                    main: 0,
                    helper: 20,
                    estimated: 20
                },
                SlotSum {
                    main: 9,
                    helper: 0,
                    estimated: 0
                },
            ]
        );
        // 同じ行を、境をまたがない区切り（30 分ごと）で見ると、見立ては無く、行の終わりの時刻で入れた時と同じ額になる。
        assert_eq!(
            spread(&rows, &[0, 1800], 3600),
            [
                SlotSum {
                    main: 109,
                    helper: 40,
                    estimated: 0
                },
                SlotSum::default(),
            ]
        );
        assert_eq!(by_slot(&rows, &[0, 1800], 3600), [(109, 40), (0, 0)]);
        assert_eq!(
            SlotSum {
                main: 3,
                helper: 4,
                estimated: 0
            }
            .total(),
            7
        );
    }

    #[test]
    fn 区切りごと_役割の升は_手伝いを席の役割に入れ_額の大きい順に並べる() {
        let actor = |id: &str, role: Option<&str>| Actor {
            id: id.to_owned(),
            labels: role
                .map(|r| (ROLE.to_owned(), r.to_owned()))
                .into_iter()
                .collect(),
        };
        let actors = [
            actor("se", Some("software-engineer")),
            actor("tpm", Some("tpm")),
            actor("pm", None),
        ];
        let of = |id: &str, row: Row| Row {
            actor_id: id.to_owned(),
            ..row
        };
        let helper = |row: Row| Row {
            agent: Some("Explore".to_owned()),
            ..row
        };
        let rows = [
            of("se", span(0, 599, 100)),
            // 手伝いの額も、起こした席の役割（software-engineer）に入れる。
            of("se", helper(span(600, 700, 30))),
            // 境をまたぐ行は、配った額を見立てとして数える。
            of("tpm", span(300, 899, 40)),
            // ラベルの無い席と、actors に無い席は「無し」。
            of("pm", span(0, 10, 39)),
            of("gone", span(1200, 1210, 1)),
            // 額の無い役割の行は出さない。
            of("zero", span(0, 10, 0)),
        ];
        let lines = by_slot_role(&rows, &actors, &[0, 600, 1200], 1800);
        assert_eq!(
            lines,
            [
                RoleLine {
                    name: Some("software-engineer".to_owned()),
                    cells: vec![(100, 0), (30, 0), (0, 0)],
                    total: 130,
                },
                RoleLine {
                    name: Some("tpm".to_owned()),
                    cells: vec![(20, 20), (20, 20), (0, 0)],
                    total: 40,
                },
                // 同じ額（40）なら、「無し」は後ろ。
                RoleLine {
                    name: None,
                    cells: vec![(39, 0), (0, 0), (1, 0)],
                    total: 40,
                },
            ]
        );
        // 行の和は、区切りごとの額（spread）の和と同じ。
        let slots = spread(&rows, &[0, 600, 1200], 1800);
        for (i, slot) in slots.iter().enumerate() {
            assert_eq!(
                lines.iter().map(|l| l.cells[i].0).sum::<i64>(),
                slot.total()
            );
        }
        assert!(by_slot_role(&[], &actors, &[0, 600], 1200).is_empty());
    }

    #[test]
    fn 升の色の段は_いちばん多い升の半分以上_1_割以上_それより下の_3_段() {
        assert_eq!(slot_heat(100, 100), Some(3));
        assert_eq!(slot_heat(50, 100), Some(3));
        assert_eq!(slot_heat(49, 100), Some(2));
        assert_eq!(slot_heat(10, 100), Some(2));
        assert_eq!(slot_heat(9, 100), Some(1));
        assert_eq!(slot_heat(1, 100), Some(1));
        assert_eq!(slot_heat(0, 100), None);
        assert_eq!(slot_heat(5, 0), None);
    }

    #[test]
    fn 席ごとの数は_額が本体と手伝いの計_いちばん長い文脈は本体だけ() {
        let rows = [
            main("a", 120_000, &[(4, 2, 10)]),
            main("a", 230_000, &[(9, 1, 20)]),
            helper("a", 900_000, &[(36, 1, 5)]),
            helper("b", 30_000, &[(1, 4, 7)]),
        ];
        let seats = by_seat(&rows);
        assert_eq!(seats.len(), 2);
        assert_eq!(
            seats["a"],
            Seat {
                cost: 35,
                max_context: 230_000
            }
        );
        // 手伝いの回だけの席は、いちばん長い文脈が無い（0）。
        assert_eq!(
            seats["b"],
            Seat {
                cost: 7,
                max_context: 0
            }
        );
        assert!(by_seat(&[]).is_empty());
    }

    #[test]
    fn 時ごとのいちばん長い文脈は_本体の回の大きい方を時の順に() {
        const H: i64 = 3600;
        let rows = [
            at(main("a", 120_000, &[(4, 1, 1)]), 3 * H + 1),
            at(main("a", 61_000, &[(2, 1, 1)]), H + 10),
            at(main("a", 230_000, &[(9, 1, 1)]), H + 2_000),
            // 手伝いの回は数えない（その時に本体の回が無ければ、その時は入れない）。
            at(helper("a", 900_000, &[(36, 1, 1)]), H + 5),
            at(helper("a", 900_000, &[(36, 1, 1)]), 2 * H + 5),
        ];
        assert_eq!(context_by_hour(&rows), [(H, 230_000), (3 * H, 120_000)]);
        assert!(context_by_hour(&[]).is_empty());
    }

    // --- model と役割ごとの行列 ---

    /// model と手伝いの種類を決めた 1 行（段は 0 に、回の数と額をみな入れる）。
    fn of(actor_id: &str, model: &str, agent: Option<&str>, turns: i64, cost: i64) -> Row {
        Row {
            model: model.to_owned(),
            agent: agent.map(str::to_owned),
            ..main(actor_id, 0, &[(0, turns, cost)])
        }
    }

    fn cell(turns: i64, cost: i64) -> Option<Cell> {
        Some(Cell { turns, cost })
    }

    fn no_efforts() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn 行が無ければ_行列は空() {
        assert_eq!(by_model(&[], &[], &no_efforts()), Matrix::default());
        let actors = [actor("a", &[("tpm.role", "se")])];
        assert_eq!(by_model(&[], &actors, &no_efforts()), Matrix::default());
    }

    #[test]
    fn 列は_model_の額の大きい順で_同じなら名の順_行と升と計を出す() {
        const OPUS: &str = "claude-opus-5-5";
        const FABLE: &str = "claude-fable-5-2";
        const HAIKU: &str = "claude-haiku-5-5";
        let actors = [
            actor("a", &[("tpm.role", "se")]),
            actor("b", &[("tpm.role", "tpm")]),
            actor("c", &[]),
        ];
        let rows = [
            of("a", OPUS, None, 3, 300),
            of("a", FABLE, None, 2, 400),
            of("b", OPUS, None, 1, 500),
            of("c", HAIKU, None, 1, 100),
            // 手伝いは、種類ごとに 1 行。役割（a と b）では分けない。
            of("a", OPUS, Some("general-purpose"), 4, 40),
            of("b", OPUS, Some("general-purpose"), 1, 10),
            of("a", HAIKU, Some("Explore"), 5, 5),
        ];
        let matrix = by_model(&rows, &actors, &no_efforts());
        // 列の計: opus 850・fable 400・haiku 105。
        assert_eq!(
            matrix.models,
            vec![
                (
                    OPUS.to_owned(),
                    Cell {
                        turns: 9,
                        cost: 850
                    }
                ),
                (
                    FABLE.to_owned(),
                    Cell {
                        turns: 2,
                        cost: 400
                    }
                ),
                (
                    HAIKU.to_owned(),
                    Cell {
                        turns: 6,
                        cost: 105
                    }
                ),
            ]
        );
        // 上の組は、役割ごとの表と同じ並び（本体の額: se 700・tpm 500・無し 100）。
        let roles: Vec<_> = matrix
            .roles
            .iter()
            .map(|r| (r.name.as_deref(), r.cells.clone(), r.total))
            .collect();
        assert_eq!(
            roles,
            vec![
                (
                    Some("se"),
                    vec![cell(3, 300), cell(2, 400), None],
                    Cell {
                        turns: 5,
                        cost: 700
                    }
                ),
                (
                    Some("tpm"),
                    vec![cell(1, 500), None, None],
                    Cell {
                        turns: 1,
                        cost: 500
                    }
                ),
                (
                    None,
                    vec![None, None, cell(1, 100)],
                    Cell {
                        turns: 1,
                        cost: 100
                    }
                ),
            ]
        );
        // 下の組は、手伝いの種類ごと（額の大きい順）。
        let helpers: Vec<_> = matrix
            .helpers
            .iter()
            .map(|r| (r.name.as_deref(), r.cells.clone(), r.total, r.efforts.len()))
            .collect();
        assert_eq!(
            helpers,
            vec![
                (
                    Some("general-purpose"),
                    vec![cell(5, 50), None, None],
                    Cell { turns: 5, cost: 50 },
                    0
                ),
                (
                    Some("Explore"),
                    vec![None, None, cell(5, 5)],
                    Cell { turns: 5, cost: 5 },
                    0
                ),
            ]
        );
        assert_eq!(
            matrix.total,
            Cell {
                turns: 17,
                cost: 1_355
            }
        );
    }

    #[test]
    fn 列の額が同じなら_model_の名の順_手伝いの額が同じなら種類の名の順() {
        let actors = [actor("a", &[("tpm.role", "se")])];
        let rows = [
            of("a", "m-b", None, 1, 100),
            of("a", "m-a", None, 1, 100),
            of("a", "m-a", Some("z"), 1, 7),
            of("a", "m-a", Some("y"), 1, 7),
        ];
        let matrix = by_model(&rows, &actors, &no_efforts());
        let models: Vec<_> = matrix.models.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(models, ["m-a", "m-b"]);
        let helpers: Vec<_> = matrix.helpers.iter().map(|r| r.name.as_deref()).collect();
        assert_eq!(helpers, [Some("y"), Some("z")]);
    }

    #[test]
    fn 同じ席_同じ時_同じ_model_の行は足す() {
        let actors = [actor("a", &[("tpm.role", "se")])];
        let rows = [of("a", "m", None, 3, 300), of("a", "m", None, 2, 200)];
        let matrix = by_model(&rows, &actors, &no_efforts());
        assert_eq!(matrix.roles.len(), 1);
        assert_eq!(matrix.roles[0].cells, vec![cell(5, 500)]);
        assert_eq!(
            matrix.models,
            vec![(
                "m".to_owned(),
                Cell {
                    turns: 5,
                    cost: 500
                }
            )]
        );
    }

    #[test]
    fn 段は全体の額に対する比で_10_以上は_3_1_以上は_2_0_より大きければ_1() {
        assert_eq!(heat(1_000, 10_000), Some(3));
        assert_eq!(heat(999, 10_000), Some(2));
        assert_eq!(heat(100, 10_000), Some(2));
        assert_eq!(heat(99, 10_000), Some(1));
        assert_eq!(heat(1, 10_000), Some(1));
        assert_eq!(heat(0, 10_000), None);
        assert_eq!(heat(10_000, 10_000), Some(3));
        // 全体の額が 0（額 0 で回だけ在る期間）なら、どの升も段なし。
        assert_eq!(heat(0, 0), None);
        // 大きな額でも溢れない。
        assert_eq!(heat(i64::MAX / 2, i64::MAX), Some(3));
    }

    #[test]
    fn effort_は_行の本体の回を持つ席を_effort_ごとに数え_多い順_同じなら名の順() {
        let actors = [
            actor("a", &[("tpm.role", "se")]),
            actor("b", &[("tpm.role", "se")]),
            actor("c", &[("tpm.role", "se")]),
            actor("d", &[("tpm.role", "se")]),
            actor("e", &[("tpm.role", "tpm")]),
        ];
        let rows = [
            of("a", "m", None, 1, 1),
            // 同じ席の行が 2 つでも、席は 1 つ。
            of("a", "m2", None, 1, 1),
            of("b", "m", None, 1, 1),
            of("c", "m", None, 1, 1),
            // effort の分からない席（d）は数えない。
            of("d", "m", None, 1, 1),
            // 手伝いの回だけの席（e）は、上の組の行を持たない。
            of("e", "m", Some("Explore"), 1, 1),
        ];
        let efforts: HashMap<String, String> = [
            ("a", "medium"),
            ("b", "low"),
            ("c", "medium"),
            ("e", "high"),
        ]
        .into_iter()
        .map(|(id, e)| (id.to_owned(), e.to_owned()))
        .collect();
        let matrix = by_model(&rows, &actors, &efforts);
        assert_eq!(matrix.roles.len(), 1);
        assert_eq!(
            matrix.roles[0].efforts,
            vec![("medium".to_owned(), 2), ("low".to_owned(), 1)]
        );
        // 行の席の数は、effort の分からない席（d）も入れる（画面が、数えた席が足りないと分かるように）。
        assert_eq!(matrix.roles[0].seats, 4);
        assert_eq!(matrix.helpers[0].seats, 0);
        // 下の組は effort を持たない。
        assert!(matrix.helpers.iter().all(|r| r.efforts.is_empty()));

        // 席の数が同じなら、名の順。
        let tie: HashMap<String, String> = [("a", "medium"), ("b", "high")]
            .into_iter()
            .map(|(id, e)| (id.to_owned(), e.to_owned()))
            .collect();
        let matrix = by_model(&rows[..3], &actors, &tie);
        assert_eq!(
            matrix.roles[0].efforts,
            vec![("high".to_owned(), 1), ("medium".to_owned(), 1)]
        );
    }

    #[test]
    fn 額が_0_でも_回の在る升は出す() {
        let actors = [actor("a", &[("tpm.role", "se")])];
        let matrix = by_model(&[of("a", "m", None, 2, 0)], &actors, &no_efforts());
        assert_eq!(matrix.roles[0].cells, vec![cell(2, 0)]);
        assert_eq!(matrix.total, Cell { turns: 2, cost: 0 });
    }
}
