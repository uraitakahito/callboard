//! 画面の字（AI-50）。字は `static/locales/{ja,en}.json`（i18next の JSON）の 1 か所に置き、画面は鍵で引く。
//!
//! - サーバーは日本語の字を HTML に書く（JavaScript が無くても読める・日本語の人に字の空きが無い）。字を書く要素には、鍵も
//!   `data-i18n="<鍵>"` で付ける。日本語でない時は、ブラウザーの i18next（`static/i18n.js`）が鍵から字を引いて差し替える。
//! - サーバーがするのは、日本語の字を鍵で引くことだけ（値の差し込みは、使う段で足す）。複数形・言語の選び・訳の形式は i18next が
//!   持つ（サーバーは持たない）。そのため、訳の file に使ってよい差し込みは `{{名}}` だけにする（試験が見る）。
//! - ブラウザーへは、同じ JSON を `/static/locales/{ja,en}.js` の script（`(window.callboardLocales ??= {}).ja = {…};`）で配る。
//!   同期の script なので、i18next は訳の中身ごと同期で起き、本文の script が動く前から字を引ける。

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde_json::Value;

use super::versioned;

/// 日本語の字（i18next の JSON）。
pub(crate) const JA_JSON: &str = include_str!("../../../../static/locales/ja.json");
/// 英語の字（i18next の JSON）。
pub(crate) const EN_JSON: &str = include_str!("../../../../static/locales/en.json");

/// 入れ子の JSON を、`a.b.c` の鍵の平らな表にする。値が字でない葉は、訳の file の誤りとして落とす（試験で当たる）。
fn flatten(prefix: &str, value: &Value, out: &mut BTreeMap<String, String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&key, value, out);
            }
        }
        Value::String(text) => {
            out.insert(prefix.to_owned(), text.clone());
        }
        other => panic!("訳の file の {prefix} が字でない: {other}"),
    }
}

/// 訳の file を、鍵の平らな表にする。
pub(crate) fn table(json: &str) -> BTreeMap<String, String> {
    let value: Value = serde_json::from_str(json).expect("訳の file が JSON でない");
    let mut out = BTreeMap::new();
    flatten("", &value, &mut out);
    out
}

/// 日本語の字の表（起きた時に 1 度だけ読む）。
static JA: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| table(JA_JSON));

/// 鍵の日本語の字。数で言い分ける字（i18next の `<鍵>_other`）は、`<鍵>` でも引ける（日本語は言い分けない）。
/// 鍵が無ければ鍵をそのまま返す（i18next と同じふるまい。鍵の抜けは試験が見る）。
pub(crate) fn text(key: &str) -> &str {
    JA.get(key)
        .or_else(|| JA.get(&format!("{key}_other")))
        .map_or(key, String::as_str)
}

/// 差し込む値。数の型で渡した値だけを数にする（i18next が `count` で英語の言い分けを選ぶ。字の `"007"` は字のまま）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Arg {
    Str(String),
    Num(i64),
}

impl std::fmt::Display for Arg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Str(text) => f.write_str(text),
            Self::Num(n) => write!(f, "{n}"),
        }
    }
}

impl Arg {
    fn json(&self) -> Value {
        match self {
            Self::Str(text) => Value::String(text.clone()),
            Self::Num(n) => Value::from(*n),
        }
    }
}

/// template の filter（`t1`・`opts1` など）に渡せる値。数の型は数、字の型は字になる。
pub(crate) trait ToArg {
    fn to_arg(&self) -> Arg;
}

impl ToArg for str {
    fn to_arg(&self) -> Arg {
        Arg::Str(self.to_owned())
    }
}

impl ToArg for String {
    fn to_arg(&self) -> Arg {
        Arg::Str(self.clone())
    }
}

impl<T: ToArg + ?Sized> ToArg for &T {
    fn to_arg(&self) -> Arg {
        (**self).to_arg()
    }
}

macro_rules! num_arg {
    ($($t:ty),*) => {$(
        impl ToArg for $t {
            fn to_arg(&self) -> Arg {
                Arg::Num(i64::try_from(*self).unwrap_or(i64::MAX))
            }
        }
    )*};
}
num_arg!(i32, i64, u8, u16, u32, u64, usize, u128);

/// 字の `{{名}}` に値を入れる（i18next の差し込みのうち、訳の file に使ってよい形はこれだけ）。元の字を 1 度だけ走るので、
/// 入れた値の中の `{{名}}` は置き換えない。値の無い `{{名}}` は、そのまま残す。
fn fill(text: &str, values: &[(&str, Arg)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("{{") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let Some(end) = after.find("}}") else {
            rest = &rest[at..];
            break;
        };
        let name = &after[..end];
        match values.iter().find(|(n, _)| *n == name) {
            Some((_, value)) => out.push_str(&value.to_string()),
            None => {
                out.push_str("{{");
                out.push_str(name);
                out.push_str("}}");
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// `data-i18n-options` に書く値（JSON。ブラウザーの i18next が同じ値で字を組む）。
fn options(values: &[(&str, Arg)]) -> String {
    options_map(values).to_string()
}

/// [`options`] の JSON の物。
fn options_map(values: &[(&str, Arg)]) -> Value {
    Value::Object(
        values
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.json()))
            .collect(),
    )
}

/// 英語の字の表（言語ごとに字を組んで値にする時だけ引く。[`Text::with_text`]）。
static EN: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| table(EN_JSON));

/// 鍵の英語の字。`count` が 1 なら `<鍵>_one`、ほかは `<鍵>_other`、無ければ `<鍵>`（i18next の英語の言い分けと同じ）。
fn english<'a>(key: &'a str, count: Option<&Arg>) -> &'a str {
    let suffix = if count == Some(&Arg::Num(1)) {
        "_one"
    } else {
        "_other"
    };
    EN.get(&format!("{key}{suffix}"))
        .or_else(|| EN.get(key))
        .map_or(key, String::as_str)
}

/// HTML の属性の値として逃がす（`& < > " '`。askama と同じ数字の形）。
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&#38;"),
            '<' => out.push_str("&#60;"),
            '>' => out.push_str("&#62;"),
            '"' => out.push_str("&#34;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Rust が組んで画面に出す字（鍵と、差し込む値）。画面の字を日本語で返す関数は、日本語の代わりにこれを返す。
///
/// - `Display` は日本語の字（[`text`] の字に値を入れた物）。template では `{{ x }}` で書く（askama が逃がす）。
/// - [`Text::attrs`] は、その字の要素に付ける ` data-i18n="<鍵>"`（値があれば ` data-i18n-options="…"` も）。template では
///   `<span{{ x.attrs()|safe }}>{{ x }}</span>`。ブラウザーの i18next が、同じ鍵と値で英語に差し替える。
/// - 言語で形の替わる値（`2.5 万`・ほかの字の中に入れる字）は [`Text::with_both`]: 日本語の字は `{{名}}`、英語の字は `{{名En}}`
///   で引く（`data-i18n-options` には両方を書く）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    key: &'static str,
    values: Vec<(String, Arg)>,
}

impl Text {
    pub(crate) fn new(key: &'static str) -> Self {
        Self {
            key,
            values: Vec::new(),
        }
    }

    /// 鍵の無い字（数・時刻など、言語で替わらない字）。そのまま書き、鍵の属性は付けない（[`Text::attrs`] は空）。
    pub(crate) fn plain(text: impl Into<String>) -> Self {
        Self {
            key: "",
            values: vec![(String::new(), Arg::Str(text.into()))],
        }
    }

    fn as_plain(&self) -> Option<String> {
        self.key.is_empty().then(|| {
            self.values
                .first()
                .map(|(_, v)| v.to_string())
                .unwrap_or_default()
        })
    }

    /// 言語で形の替わらない字の値（id・呼ぶ先の字など）を `{{名}}` に入れる。数も字として書く（[`Text::with_num`] と違う）。
    pub(crate) fn with(mut self, name: &str, value: impl std::fmt::Display) -> Self {
        self.values
            .push((name.to_owned(), Arg::Str(value.to_string())));
        self
    }

    /// 数の値（`data-i18n-options` に数で書く。英語の言い分けは `count` の名で渡す）。
    pub(crate) fn with_num<N: TryInto<i64>>(mut self, name: &str, n: N) -> Self {
        let n = n.try_into().unwrap_or(i64::MAX);
        self.values.push((name.to_owned(), Arg::Num(n)));
        self
    }

    /// 言語で形の替わる値。日本語の字の `{{名}}` に `ja`、英語の字の `{{名En}}` に `en` を入れる。
    pub(crate) fn with_both(
        mut self,
        name: &str,
        ja: impl Into<String>,
        en: impl Into<String>,
    ) -> Self {
        self.values.push((name.to_owned(), Arg::Str(ja.into())));
        self.values.push((format!("{name}En"), Arg::Str(en.into())));
        self
    }

    /// ほかの字を値に入れる（日本語の字に日本語の字を、英語の字に英語の字を）。
    pub(crate) fn with_text(self, name: &str, text: &Text) -> Self {
        self.with_both(name, text.to_string(), text.english())
    }

    pub(crate) fn key(&self) -> &'static str {
        self.key
    }

    fn pairs(&self) -> Vec<(&str, Arg)> {
        self.values
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect()
    }

    /// 英語の字（値を入れた物）。ほかの字の値にする時だけ使う（画面の英語は、ブラウザーが組む）。
    pub(crate) fn english(&self) -> String {
        if let Some(plain) = self.as_plain() {
            return plain;
        }
        let count = self
            .values
            .iter()
            .find(|(name, _)| name == "count")
            .map(|(_, value)| value);
        fill(english(self.key, count), &self.pairs())
    }

    /// 差し込む値（JSON の object。誤りの JSON の `values` と、`data-i18n-options` に書く物）。
    pub(crate) fn values(&self) -> Value {
        options_map(&self.pairs())
    }

    /// `data-i18n-options` の属性（値が無ければ空）。
    fn options_attr(&self) -> String {
        if self.values.is_empty() {
            String::new()
        } else {
            format!(
                " data-i18n-options=\"{}\"",
                escape(&self.values().to_string())
            )
        }
    }

    /// 字の要素に付ける属性（` data-i18n="<鍵>"` と、値があれば ` data-i18n-options="…"`）。逃がしてあるので `|safe` で書く。
    pub(crate) fn attrs(&self) -> String {
        if self.key.is_empty() {
            return String::new();
        }
        format!(" data-i18n=\"{}\"{}", escape(self.key), self.options_attr())
    }

    /// 属性（`title` など）の字にする時の属性（` data-i18n-attr="<属性>:<鍵>"` と `data-i18n-options`）。字の鍵と同じ要素に
    /// 付けるのは、値の無い字の時だけ（`data-i18n-options` は要素に 1 つ）。
    pub(crate) fn attr(&self, name: &str) -> String {
        if self.key.is_empty() {
            return String::new();
        }
        format!(
            " data-i18n-attr=\"{}:{}\"{}",
            escape(name),
            escape(self.key),
            self.options_attr()
        )
    }
}

/// 試験で、日本語の字と比べる（`assert_eq!(text, "字")`）。
#[cfg(test)]
impl PartialEq<&str> for Text {
    fn eq(&self, other: &&str) -> bool {
        self.to_string().as_str() == *other
    }
}

impl std::fmt::Display for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.as_plain() {
            Some(plain) => f.write_str(&plain),
            None => f.write_str(&fill(text(self.key), &self.pairs())),
        }
    }
}

/// ブラウザーへ配る訳の script の中身（`(window.callboardLocales ??= {}).<言語> = {…};`）。
fn script(lang: &str, json: &str) -> String {
    let value: Value = serde_json::from_str(json).expect("訳の file が JSON でない");
    format!("(window.callboardLocales ??= {{}}).{lang} = {value};\n")
}

/// `/static/locales/ja.js` の中身。
pub(crate) static JA_JS: LazyLock<String> = LazyLock::new(|| script("ja", JA_JSON));
/// `/static/locales/en.js` の中身。
pub(crate) static EN_JS: LazyLock<String> = LazyLock::new(|| script("en", EN_JSON));
/// 画面が読む `/static/locales/ja.js` の path（`?v=` に中身の写しの頭）。
pub(crate) static JA_JS_HREF: LazyLock<String> =
    LazyLock::new(|| versioned("/static/locales/ja.js", &JA_JS));
/// 画面が読む `/static/locales/en.js` の path。
pub(crate) static EN_JS_HREF: LazyLock<String> =
    LazyLock::new(|| versioned("/static/locales/en.js", &EN_JS));

/// 画面の字を差し替える script（`static/i18n.js`）。
pub(crate) const I18N_JS: &str = include_str!("../../../../static/i18n.js");
/// 画面が読む `/static/i18n.js` の path。
pub(crate) static I18N_JS_HREF: LazyLock<String> =
    LazyLock::new(|| versioned("/static/i18n.js", I18N_JS));

/// 取り込んだ i18next（npm の物をそのまま。static/vendor/SOURCES.md）。
pub(crate) const I18NEXT_JS: &str = include_str!("../../../../static/vendor/i18next-27.2.9.min.js");
/// i18next の path。版が名に入るので、中身が替われば path も替わる。
pub(crate) const I18NEXT_PATH: &str = "/static/vendor/i18next-27.2.9.min.js";

/// template の filter。`{{ "nav.actors"|t }}` で、鍵の日本語の字を書く（字は HTML として逃がす）。
/// 画面の template を持つ module は、`use super::i18n::filters;` で取り込む。
///
/// 値を差し込む字は、`t1`・`t2` で名と値を渡し、同じ名と値を `opts1`・`opts2` で `data-i18n-options` に書く:
/// `<p data-i18n="list.defaulted" data-i18n-options="{{ "days"|opts1(days) }}">{{ "list.defaulted"|t1("days", days) }}</p>`
pub(crate) mod filters {
    use std::fmt::Display;

    use super::ToArg;

    #[askama::filter_fn]
    pub fn t(key: impl Display, _: &dyn askama::Values) -> askama::Result<String> {
        Ok(super::text(&key.to_string()).to_owned())
    }

    /// 値 1 つを差し込む字。
    #[askama::filter_fn]
    pub fn t1<N: Display, V: ToArg>(
        key: impl Display,
        _: &dyn askama::Values,
        name: N,
        value: V,
    ) -> askama::Result<String> {
        Ok(super::fill(
            super::text(&key.to_string()),
            &[(&name.to_string(), value.to_arg())],
        ))
    }

    /// 値 2 つを差し込む字。
    #[askama::filter_fn]
    pub fn t2<N1: Display, V1: ToArg, N2: Display, V2: ToArg>(
        key: impl Display,
        _: &dyn askama::Values,
        name1: N1,
        value1: V1,
        name2: N2,
        value2: V2,
    ) -> askama::Result<String> {
        Ok(super::fill(
            super::text(&key.to_string()),
            &[
                (&name1.to_string(), value1.to_arg()),
                (&name2.to_string(), value2.to_arg()),
            ],
        ))
    }

    /// 値 1 つの `data-i18n-options`（`{{ "days"|opts1(days) }}`）。数の型の値は数で、字の型の値は字で書く。
    #[askama::filter_fn]
    pub fn opts1<V: ToArg>(
        name: impl Display,
        _: &dyn askama::Values,
        value: V,
    ) -> askama::Result<String> {
        Ok(super::options(&[(&name.to_string(), value.to_arg())]))
    }

    /// 値 2 つの `data-i18n-options`（`{{ "n"|opts2(n, "m", m) }}`）。
    #[askama::filter_fn]
    pub fn opts2<V1: ToArg, N2: Display, V2: ToArg>(
        name1: impl Display,
        _: &dyn askama::Values,
        value1: V1,
        name2: N2,
        value2: V2,
    ) -> askama::Result<String> {
        Ok(super::options(&[
            (&name1.to_string(), value1.to_arg()),
            (&name2.to_string(), value2.to_arg()),
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 入れ子の鍵を平らにして引く() {
        assert_eq!(text("nav.actors"), "アクター");
        assert_eq!(text("無い.鍵"), "無い.鍵");
    }

    #[test]
    fn 値を名で入れ_同じ値を_options_に書く() {
        let values = [("n", Arg::Num(3)), ("who", Arg::Str("se.1".to_owned()))];
        assert_eq!(fill("{{who}} の {{n}} 件", &values), "se.1 の 3 件");
        assert_eq!(fill("{{n}} 件", &[("m", Arg::Num(3))]), "{{n}} 件");
        assert_eq!(options(&values), r#"{"n":3,"who":"se.1"}"#);
    }

    #[test]
    fn 数にするのは数の型の値だけ() {
        // 字の "007" は字のまま（数にすると、ブラウザーの字が 7 になり、サーバーの字と食い違う）。
        let values = [("id", "007".to_arg()), ("n", 7_u64.to_arg())];
        assert_eq!(options(&values), r#"{"id":"007","n":7}"#);
        assert_eq!(fill("{{id}}・{{n}}", &values), "007・7");
        let text = Text::new("nav.actors").with("id", "007").with_num("n", 7);
        assert_eq!(text.values(), serde_json::json!({"id": "007", "n": 7}));
    }

    #[test]
    fn 差し込みは元の字を_1_度だけ走る() {
        // 1 つ目の値に 2 つ目の名の字があっても、置き換えない。
        let values = [
            ("a", Arg::Str("{{b}}".to_owned())),
            ("b", Arg::Str("x".to_owned())),
        ];
        assert_eq!(fill("{{a}}-{{b}}", &values), "{{b}}-x");
        assert_eq!(fill("閉じない {{a", &values), "閉じない {{a");
        assert_eq!(fill("{{}} と {{c}}", &values), "{{}} と {{c}}");
    }

    #[test]
    fn 字は日本語で書き_属性に鍵と値を逃がして書く() {
        let text = Text::new("home.version");
        assert_eq!(text.to_string(), "版");
        assert_eq!(text.attrs(), r#" data-i18n="home.version""#);
        assert_eq!(
            text.attr("title"),
            r#" data-i18n-attr="title:home.version""#
        );
        let text = Text::new("messages.via").with("via", r#"<a href="x">'&'</a>"#);
        assert_eq!(text.to_string(), r#"渡した時: <a href="x">'&'</a>"#);
        assert_eq!(
            text.attrs(),
            " data-i18n=\"messages.via\" data-i18n-options=\"{&#34;via&#34;:&#34;&#60;a href=\\&#34;x\\&#34;&#62;&#39;&#38;&#39;&#60;/a&#62;&#34;}\""
        );
        assert_eq!(
            text.values(),
            serde_json::json!({"via": r#"<a href="x">'&'</a>"#})
        );
    }

    #[test]
    fn 言語で形の替わる値は_名と_名_en_で両方を渡す() {
        let inner = Text::new("upstream.unreachable");
        let text = Text::new("errors.unreachable")
            .with("service", "pagetray")
            .with_text("why", &inner);
        assert_eq!(text.to_string(), "pagetray に聞けません（つながりません）");
        assert_eq!(
            text.values(),
            serde_json::json!({"service": "pagetray", "why": "つながりません", "whyEn": "cannot connect"})
        );
        assert_eq!(text.english(), "Cannot reach pagetray (cannot connect)");
    }

    #[test]
    fn 英語の字は_count_で言い分け_日本語は_other_を引く() {
        let bar = |n: i64| {
            Text::new("usage.byContext.bar")
                .with_both("range", "0〜2.5 万", "0–25k")
                .with("cost", "$1.00")
                .with("turns", n)
                .with_num("count", n)
        };
        assert_eq!(bar(1).english(), "0–25k: $1.00 (1 turn)");
        assert_eq!(bar(2).english(), "0–25k: $1.00 (2 turns)");
        assert_eq!(bar(1).to_string(), "0〜2.5 万: $1.00（1 回）");
        // 鍵の無い字は、そのまま書き、属性を付けない。
        let plain = Text::plain("$2");
        assert_eq!(plain.to_string(), "$2");
        assert_eq!(plain.english(), "$2");
        assert_eq!(plain.attrs(), "");
        assert_eq!(plain.attr("title"), "");
    }

    #[test]
    fn 配る_script_は_訳を_callboard_locales_に置く() {
        assert!(JA_JS.starts_with("(window.callboardLocales ??= {}).ja = {"));
        assert!(EN_JS.starts_with("(window.callboardLocales ??= {}).en = {"));
        assert!(JA_JS.trim_end().ends_with("};"));
    }
}
