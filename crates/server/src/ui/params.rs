//! 画面の URL の引数を読む所（castlist の画面の `ui/params.rs` と、期間の欄の所を移した物）。
//!
//! - 引数の名は snake_case。知らない名・1 つだけの名の重なり・読めない値は、引数の名を書いた 400 の画面。
//! - 期間の欄は browser の日時の欄（`datetime-local`）。送られた字は日本時間として RFC 3339 にしてから読む（[`period`]）。
//!   欄の横に、期間をすぐ選ぶ link（[`quick_links`]）を置く。
use axum::extract::Query;
use axum::extract::rejection::QueryRejection;
use axum::http::StatusCode;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::i18n::Text;
use super::time::{OFFSET, stamp};
use crate::upstream::Failure;

/// 画面の誤り（状態と、画面に書く訳）。
pub struct PageError(pub StatusCode, pub Text);

/// 引数の誤り（400）。訳には、誤った引数の名を入れる。
pub fn bad_request(message: Text) -> PageError {
    PageError(StatusCode::BAD_REQUEST, message)
}

/// 呼ぶ先（`service`: castlist・pagetray）に聞けなかった訳を、画面の誤りにする。断った訳（呼ぶ先の字）は、そのまま書く。
pub fn failed(service: &'static str, e: Failure) -> PageError {
    match e {
        Failure::Rejected(message) => bad_request(
            Text::new("errors.rejected")
                .with("service", service)
                .with("detail", message),
        ),
        // 名指す物の無い問いの 404。
        Failure::NotFound => bad_request(Text::new("errors.rejectedBare").with("service", service)),
        Failure::Unavailable(why) => PageError(
            StatusCode::SERVICE_UNAVAILABLE,
            Text::new("errors.unreachableRetry")
                .with("service", service)
                .with_text("why", &why),
        ),
    }
}

pub type Pairs = Vec<(String, String)>;

/// 問い合わせの引数を、名と値の並びで読む（読めなければ 400）。
pub fn pairs(q: Result<Query<Pairs>, QueryRejection>) -> Result<Pairs, PageError> {
    q.map(|Query(pairs)| pairs)
        .map_err(|e| bad_request(Text::new("params.unreadable").with("detail", e)))
}

/// ラベルの鍵の名: 英小文字・数字・`-`・`.` で 64 字まで。
fn is_name(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
}

/// 値: 空でなく、制御文字の無い、256 字までの字。
fn is_value(s: &str) -> bool {
    !s.is_empty() && s.chars().count() <= 256 && !s.chars().any(char::is_control)
}

/// 問い合わせの引数（名と値の並び）。知らない名と、1 つだけの名の重なりは、読む時に断る。
pub struct Params(Pairs);

impl Params {
    pub fn new(pairs: Pairs, known: &[&str], repeatable: &[&str]) -> Result<Self, PageError> {
        for (i, (name, _)) in pairs.iter().enumerate() {
            if !known.contains(&name.as_str()) {
                return Err(bad_request(
                    Text::new("params.unknown")
                        .with("name", name)
                        .with_list("known", known),
                ));
            }
            if !repeatable.contains(&name.as_str()) && pairs[..i].iter().any(|(n, _)| n == name) {
                return Err(bad_request(Text::new("params.once").with("name", name)));
            }
        }
        Ok(Self(pairs))
    }

    pub fn one(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> {
        self.0
            .iter()
            .filter(move |(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// 時刻（RFC 3339）を、castlist の API に渡す字（UTC・ミリ秒まで）にする。
    pub fn time(&self, name: &str) -> Result<Option<String>, PageError> {
        self.one(name)
            .map(|v| {
                OffsetDateTime::parse(v, &Rfc3339)
                    .map(stamp)
                    .map_err(|_| bad_request(Text::new("params.time").with("name", name)))
            })
            .transpose()
    }
}

impl Params {
    pub fn flag(&self, name: &str) -> Result<Option<bool>, PageError> {
        match self.one(name) {
            None => Ok(None),
            Some("true") => Ok(Some(true)),
            Some("false") => Ok(Some(false)),
            Some(_) => Err(bad_request(Text::new("params.flag").with("name", name))),
        }
    }

    /// `true` だけを受ける絞り（付けなければ絞らない）。
    pub fn only_true(&self, name: &str) -> Result<bool, PageError> {
        match self.one(name) {
            None => Ok(false),
            Some("true") => Ok(true),
            Some(_) => Err(bad_request(Text::new("params.onlyTrue").with("name", name))),
        }
    }

    /// 名の形の値（model・effort など）。
    pub fn name(&self, name: &str) -> Result<Option<String>, PageError> {
        self.one(name)
            .map(|v| {
                if is_name(v) {
                    Ok(v.to_owned())
                } else {
                    Err(bad_request(Text::new("params.name").with("name", name)))
                }
            })
            .transpose()
    }

    /// 空でない 256 字までの値（id など）。
    pub fn value(&self, name: &str) -> Result<Option<String>, PageError> {
        self.one(name)
            .map(|v| {
                if is_value(v) {
                    Ok(v.to_owned())
                } else {
                    Err(bad_request(Text::new("params.value").with("name", name)))
                }
            })
            .transpose()
    }

    /// 決まった物のどれか。
    pub fn choice(
        &self,
        name: &str,
        choices: &[&'static str],
    ) -> Result<Option<&'static str>, PageError> {
        self.one(name)
            .map(|v| {
                choices.iter().find(|c| **c == v).copied().ok_or_else(|| {
                    bad_request(
                        Text::new("params.choice")
                            .with("name", name)
                            .with_list("choices", choices),
                    )
                })
            })
            .transpose()
    }
}

/// `鍵:値` の形の字（誤りの訳に書く）。
pub const LABEL_SHAPE: &str = "params.shape.label";
/// `種類:値` の形の字。
pub const BINDING_SHAPE: &str = "params.shape.binding";

/// `鍵:値` を初めの `:` で分けられるか。`shape` は形の字の鍵（[`LABEL_SHAPE`]・[`BINDING_SHAPE`]）。
pub fn pair(name: &str, v: &str, shape: &'static str) -> Result<(), PageError> {
    match v.split_once(':') {
        Some((key, value)) if is_name(key) && is_value(value) => Ok(()),
        _ => Err(bad_request(
            Text::new("params.pair")
                .with("name", name)
                .with_text("shape", &Text::new(shape))
                .with("value", v),
        )),
    }
}

// --- 図 ---

/// 図の画面（`GET /graph/network`）の引数の名。どれも 1 つだけ。
pub const GRAPH_PARAMS: &[&str] = &["label", "from", "to"];

/// 図の引数を確かめ、castlist の図の API（GetGraph）に渡す引数にする（期間は UTC の字。API の時刻は時差の付いた字を受けないので）。
pub fn graph_query(pairs: &Pairs) -> Result<Pairs, PageError> {
    let p = Params::new(pairs.clone(), GRAPH_PARAMS, &[])?;
    let mut out = Pairs::new();
    for (name, value) in pairs {
        let value = match name.as_str() {
            "label" => {
                pair("label", value, LABEL_SHAPE)?;
                value.clone()
            }
            _ => p.time(name)?.unwrap_or_default(),
        };
        out.push((name.clone(), value));
    }
    Ok(out)
}

// --- 期間の欄 ---

/// 一覧の期間の欄（稼働した期間）と、図の期間の欄（登録した期間）の名。
pub const LIST_PERIOD: [&str; 2] = ["active_from", "active_to"];
pub const GRAPH_PERIOD: [&str; 2] = ["from", "to"];

/// 期間の欄の引数の名と、画面の見出しの字の鍵（読めない値の 400 の画面に書く）。
const PERIOD_LABELS: [(&str, &str); 4] = [
    ("active_from", "list.filters.activeFrom"),
    ("active_to", "list.filters.activeTo"),
    ("from", "filters.registeredFrom"),
    ("to", "filters.registeredTo"),
];

/// 期間の欄の値を RFC 3339 にする。browser の日時の欄（`datetime-local`）が送る
/// `YYYY-MM-DDTHH:MM`（`:SS`・`:SS.sss` も）は日本時間として `+09:00` を付ける。
/// RFC 3339 の値（前の link）は、そのまま。どちらでもない値もそのまま返す（[`period`] が 400 にする）。
fn from_local(value: &str) -> String {
    if OffsetDateTime::parse(value, &Rfc3339).is_ok() {
        return value.to_owned();
    }
    let local = value.len() >= 16
        && value.as_bytes()[10] == b'T'
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || b"-:T.".contains(&b));
    let seconds = if value.len() == 16 { ":00" } else { "" };
    let iso = format!("{value}{seconds}+09:00");
    if local && OffsetDateTime::parse(&iso, &Rfc3339).is_ok() {
        iso
    } else {
        value.to_owned()
    }
}

/// RFC 3339 の値を、日時の欄（`datetime-local`）に入れる日本時間の字にする（form を送り直しても同じ時刻）。
/// 秒が 0 なら分まで、ミリ秒が 0 なら秒まで。読めない値は、そのまま。
pub fn to_local(value: &str) -> String {
    let Ok(t) = OffsetDateTime::parse(value, &Rfc3339) else {
        return value.to_owned();
    };
    let t = t.to_offset(OFFSET);
    let mut out = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute()
    );
    if t.second() != 0 || t.millisecond() != 0 {
        out.push_str(&format!(":{:02}", t.second()));
    }
    if t.millisecond() != 0 {
        out.push_str(&format!(".{:03}", t.millisecond()));
    }
    out
}

/// 引数のうち、期間の欄（`names`）の値を RFC 3339 にそろえる（[`from_local`]）。読めない値は、欄の見出しと
/// 直し方を書いた 400（API の字「RFC 3339 の時刻」は、欄から打つ人には硬いので、画面では使わない）。
pub fn period(pairs: &mut Pairs, names: [&str; 2]) -> Result<(), PageError> {
    fn label(name: &str) -> &'static str {
        PERIOD_LABELS
            .iter()
            .find(|(n, _)| *n == name)
            .map_or("filters.period", |(_, l)| l)
    }
    period_named(pairs, names.map(|name| (name, label(name))))
}

/// [`period`] と同じ。欄の見出しの字の鍵を、画面が渡す（同じ名の欄でも、画面ごとに見出しが違う時）。
pub fn period_named(pairs: &mut Pairs, fields: [(&str, &'static str); 2]) -> Result<(), PageError> {
    for (name, value) in pairs.iter_mut() {
        if let Some((_, label)) = fields.iter().find(|(n, _)| n == name) {
            *value = from_local(value);
            if OffsetDateTime::parse(value, &Rfc3339).is_err() {
                return Err(bad_request(
                    Text::new("params.period").with_text("label", &Text::new(label)),
                ));
            }
        }
    }
    Ok(())
}

/// 期間をすぐ選ぶ link の名と、その期間（RFC 3339・`+09:00`。秒まで）。`now` から決める。
/// 直近 1 時間・直近 7 日は今まで（今の秒を切り上げる。今の秒の中に来た記録も入れる）、
/// 今日・昨日は日本時間の 0 時から次の 0 時まで。
fn quick_ranges(now: OffsetDateTime) -> [(Text, String, String); 4] {
    let now = now.to_offset(OFFSET);
    let now = if now.nanosecond() == 0 {
        now
    } else {
        now.truncate_to_second() + time::Duration::SECOND
    };
    let today = now.replace_time(time::Time::MIDNIGHT);
    let (hour, day) = (time::Duration::HOUR, time::Duration::DAY);
    let iso = |t: OffsetDateTime| t.format(&Rfc3339).unwrap_or_default();
    [
        ("common.quickRange.hour", now - hour, now),
        ("common.quickRange.today", today, today + day),
        ("common.quickRange.yesterday", today - day, today),
        ("common.quickRange.week", now - day * 7, now),
    ]
    .map(|(name, from, to)| (Text::new(name), iso(from), iso(to)))
}

/// 期間をすぐ選ぶ link（名と URL）。今の引数のまま、期間（`names`）だけを替え、次のページの `after` は落とす。
pub fn quick_links(
    path: &str,
    pairs: &Pairs,
    names: [&str; 2],
    now: OffsetDateTime,
) -> Vec<(Text, String)> {
    let kept = pairs
        .iter()
        .filter(|(n, _)| n != "after" && !names.contains(&n.as_str()));
    quick_ranges(now)
        .into_iter()
        .map(|(name, from, to)| {
            let mut link: Pairs = kept.clone().cloned().collect();
            link.push((names[0].to_owned(), from));
            link.push((names[1].to_owned(), to));
            (name, format!("{path}?{}", query(&link)))
        })
        .collect()
}

/// 図（タイムライン・相関図）の絞りの form（今の値）。
#[derive(Default)]
pub struct GraphForm {
    pub label: String,
    pub from: String,
    pub to: String,
}

impl GraphForm {
    pub fn new(pairs: &Pairs) -> Self {
        let mut form = Self::default();
        for (name, value) in pairs {
            match name.as_str() {
                "label" => form.label.clone_from(value),
                "from" => form.from = to_local(value),
                "to" => form.to = to_local(value),
                _ => {}
            }
        }
        form
    }
}

/// URL の引数の字（`名=値&…`。`?` は付けない）。
pub fn query(pairs: &[(String, String)]) -> String {
    let query: Vec<String> = pairs
        .iter()
        .map(|(n, v)| format!("{}={}", encode(n), encode(v)))
        .collect();
    query.join("&")
}

/// URL の引数の値を % で包む（英数字と `-._~` の外。`:` も包む。pagetray の画面の link と同じ字にする）。
pub fn strict(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// URL の引数の値を % で包む（英数字と `-._~:` の外）。
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~:".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(iso: &str) -> OffsetDateTime {
        OffsetDateTime::parse(iso, &Rfc3339).unwrap()
    }

    fn pairs(list: &[(&str, &str)]) -> Pairs {
        list.iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn 日時の欄の値は日本時間として_rfc_3339_にする() {
        assert_eq!(from_local("2026-11-22T11:00"), "2026-11-22T11:00:00+09:00");
        assert_eq!(
            from_local("2026-11-22T11:00:05"),
            "2026-11-22T11:00:05+09:00"
        );
        assert_eq!(
            from_local("2026-11-22T11:00:05.250"),
            "2026-11-22T11:00:05.250+09:00"
        );
    }

    #[test]
    fn rfc_3339_の値はそのまま() {
        for iso in [
            "2026-11-22T11:00:00+09:00",
            "2026-11-22T02:00:00Z",
            "2026-11-22T02:00:00.123Z",
            "2026-11-21T21:00:00-05:00",
        ] {
            assert_eq!(from_local(iso), iso);
        }
    }

    #[test]
    fn 読めない値はそのまま返し_引数を読む所が断る() {
        for bad in [
            "きのう",
            "",
            "2026-11-22",
            "2026-11-22T11",
            "2026-13-01T00:00",
            "2026-11-22T25:00",
            "2026-11-22 11:00",
            "2026-11-22T11:00Z",
            "2026-11-22T11:00+09:00",
        ] {
            assert_eq!(from_local(bad), bad, "{bad}");
        }
    }

    #[test]
    fn 期間の欄だけを_rfc_3339_にそろえる() {
        let mut graph = pairs(&[
            ("label", "tpm.key:t-23zs"),
            ("from", "2026-11-22T00:00"),
            ("to", "2026-11-23T00:00:00+09:00"),
        ]);
        assert!(period(&mut graph, GRAPH_PERIOD).is_ok());
        assert_eq!(
            graph,
            pairs(&[
                ("label", "tpm.key:t-23zs"),
                ("from", "2026-11-22T00:00:00+09:00"),
                ("to", "2026-11-23T00:00:00+09:00"),
            ])
        );
    }

    #[test]
    fn rfc_3339_の値を日時の欄の日本時間の字にする() {
        assert_eq!(to_local("2026-11-22T11:00:00+09:00"), "2026-11-22T11:00");
        assert_eq!(to_local("2026-11-22T02:00:00Z"), "2026-11-22T11:00");
        assert_eq!(to_local("2026-11-22T20:30:00Z"), "2026-11-23T05:30");
        assert_eq!(to_local("2026-11-22T02:00:07Z"), "2026-11-22T11:00:07");
        assert_eq!(
            to_local("2026-11-22T02:00:07.120Z"),
            "2026-11-22T11:00:07.120"
        );
        assert_eq!(to_local("きのう"), "きのう");
        // 欄の字を送り直すと、同じ時刻。
        for iso in ["2026-11-22T02:00:00Z", "2026-11-22T02:00:07.120Z"] {
            assert_eq!(at(&from_local(&to_local(iso))), at(iso), "{iso}");
        }
    }

    #[test]
    fn すぐ選ぶ期間は今から決め_今日と昨日は日本時間の_0_時から() {
        // 直近の期間の終わりは、今の秒を切り上げた時刻（今の秒の中に来た記録も入れる）。
        let ranges = quick_ranges(at("2026-11-23T01:30:15.250Z"));
        let want = [
            (
                "直近 1 時間",
                "2026-11-23T09:30:16+09:00",
                "2026-11-23T10:30:16+09:00",
            ),
            (
                "今日",
                "2026-11-23T00:00:00+09:00",
                "2026-11-24T00:00:00+09:00",
            ),
            (
                "昨日",
                "2026-11-22T00:00:00+09:00",
                "2026-11-23T00:00:00+09:00",
            ),
            (
                "直近 7 日",
                "2026-11-16T10:30:16+09:00",
                "2026-11-23T10:30:16+09:00",
            ),
        ];
        let got: Vec<(String, &str, &str)> = ranges
            .iter()
            .map(|(n, f, t)| (n.to_string(), f.as_str(), t.as_str()))
            .collect();
        let got: Vec<(&str, &str, &str)> =
            got.iter().map(|(n, f, t)| (n.as_str(), *f, *t)).collect();
        assert_eq!(got, want);
        // ちょうどの秒なら、切り上げない。
        let ranges = quick_ranges(at("2026-11-23T01:30:15Z"));
        assert_eq!(ranges[0].2, "2026-11-23T10:30:15+09:00");
        // UTC ではまだ前の日でも、日本時間の日で決める。
        let ranges = quick_ranges(at("2026-11-22T20:00:00Z"));
        assert_eq!(ranges[1].1, "2026-11-23T00:00:00+09:00");
        assert_eq!(ranges[2].1, "2026-11-22T00:00:00+09:00");
    }

    #[test]
    fn すぐ選ぶ_link_はほかの条件を残し_期間を替える() {
        let now = at("2026-11-23T01:30:15Z");
        let graph = pairs(&[("to", "2026-11-01T00:00:00+09:00"), ("label", "a:b")]);
        let links = quick_links("/graph/network", &graph, GRAPH_PERIOD, now);
        let names: Vec<String> = links.iter().map(|(n, _)| n.to_string()).collect();
        assert_eq!(names, ["直近 1 時間", "今日", "昨日", "直近 7 日"]);
        let keys: Vec<&str> = links.iter().map(|(n, _)| n.key()).collect();
        assert_eq!(
            keys,
            [
                "common.quickRange.hour",
                "common.quickRange.today",
                "common.quickRange.yesterday",
                "common.quickRange.week"
            ]
        );
        assert_eq!(
            links[0].1,
            "/graph/network?label=a:b&from=2026-11-23T09:30:15%2B09:00&to=2026-11-23T10:30:15%2B09:00"
        );
        // 条件が無くても、期間だけを付ける。
        let links = quick_links("/graph/network", &Vec::new(), GRAPH_PERIOD, now);
        assert_eq!(
            links[2].1,
            "/graph/network?from=2026-11-22T00:00:00%2B09:00&to=2026-11-23T00:00:00%2B09:00"
        );
    }

    #[test]
    fn 図の引数は_確かめてから_期間を_utc_の字にして渡す() {
        let api = graph_query(&pairs(&[
            ("to", "2026-11-22T14:00:00+09:00"),
            ("label", "tpm.key:t-23zs"),
            ("from", "2026-11-22T04:00:00.5Z"),
        ]));
        assert_eq!(
            api.ok().unwrap(),
            pairs(&[
                ("to", "2026-11-22T05:00:00.000Z"),
                ("label", "tpm.key:t-23zs"),
                ("from", "2026-11-22T04:00:00.500Z"),
            ])
        );
        for (bad, word) in [
            (pairs(&[("kind", "ai")]), "知らない引数: kind"),
            (
                pairs(&[("from", "a"), ("from", "b")]),
                "from は 1 つだけ付ける",
            ),
            (pairs(&[("label", "x")]), "label は「鍵:値」の形（x）"),
            (pairs(&[("label", ":x")]), "label は「鍵:値」の形"),
            (
                pairs(&[("to", "2026-11-22T14:00")]),
                "to は RFC 3339 の時刻",
            ),
        ] {
            let PageError(status, message) = graph_query(&bad).err().unwrap();
            assert_eq!(status, StatusCode::BAD_REQUEST);
            let message = message.to_string();
            assert!(message.contains(word), "{message}");
        }
    }

    #[test]
    fn url_の値を包む() {
        assert_eq!(encode("tpm.key:t-23zs"), "tpm.key:t-23zs");
        assert_eq!(encode("a b&c=d+"), "a%20b%26c%3Dd%2B");
        assert_eq!(encode("<"), "%3C");
    }
}
