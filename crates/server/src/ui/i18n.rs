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
pub(crate) mod filters {
    use std::fmt::Display;

    #[askama::filter_fn]
    pub fn t(key: impl Display, _: &dyn askama::Values) -> askama::Result<String> {
        Ok(super::text(&key.to_string()).to_owned())
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
    fn 配る_script_は_訳を_callboard_locales_に置く() {
        assert!(JA_JS.starts_with("(window.callboardLocales ??= {}).ja = {"));
        assert!(EN_JS.starts_with("(window.callboardLocales ??= {}).en = {"));
        assert!(JA_JS.trim_end().ends_with("};"));
    }
}
