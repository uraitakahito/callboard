//! 呼ぶ先（castlist・pagetray）に頼む言語。
//!
//! 画面の字は、サーバーが日本語で組み、英語は browser が差し替える（[`crate::ui::i18n`]）。呼ぶ先が断った訳の字だけは、
//! 呼ぶ先が言語を見て文にする（castlist 2.2.0・pagetray 2.1.0 から。`Accept-Language`）。だから、画面の言語を
//! サーバーでも決めて、呼ぶ先への頼みに付ける。
//!
//! - 決め方は `static/i18n.js` の `pick` と同じ: cookie `callboard-lang` → browser の言語の並びの初めの 1 つ → 日本語。
//!   `ja`・`en` のほかは日本語（順を揃えないと、画面の字と、呼ぶ先の訳の字の言語がずれる）。
//! - 呼ぶ先に渡すのは `ja` か `en` の 2 つの値だけ。browser の header の字は、そのままでは渡さない。
//! - callboard は、今までどおり英語を組まない。呼ぶ先の字を、値として運ぶだけ。
use axum::extract::Request;
use axum::http::{HeaderMap, header};
use axum::middleware::Next;
use axum::response::Response;

/// 言語を覚える cookie の名（`static/i18n.js` の `COOKIE` と同じ）。
const COOKIE: &str = "callboard-lang";

/// 画面の言語。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Ja,
    En,
}

impl Lang {
    /// 呼ぶ先に渡す字（`Accept-Language` の値）。
    pub const fn code(self) -> &'static str {
        match self {
            Lang::Ja => "ja",
            Lang::En => "en",
        }
    }

    /// 言語の名（`en-US` など）を、ja か en に寄せる。どちらでもなければ None（`i18n.js` の `normalize`）。
    fn normalize(name: &str) -> Option<Self> {
        let base = name.split('-').next().unwrap_or_default();
        if base.eq_ignore_ascii_case("ja") {
            Some(Lang::Ja)
        } else if base.eq_ignore_ascii_case("en") {
            Some(Lang::En)
        } else {
            None
        }
    }
}

tokio::task_local! {
    /// 今の要求の言語。[`scope`] の内でだけ在る。
    static ASKED: Lang;
}

/// cookie の言語。無ければ None。名の合う初めの 1 つだけを見る（値が ja・en でなければ、無い物とみなす。`i18n.js` の `cookieLang`）。
fn cookie(headers: &HeaderMap) -> Option<Lang> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|part| part.split_once('='))
        .find(|(name, _)| name.trim() == COOKIE)
        .and_then(|(_, value)| Lang::normalize(value.trim()))
}

/// browser の言語の並び（`Accept-Language`）の初めの 1 つ。重み（`;q=`）は見ない（`navigator.languages[0]` に当たる）。
fn first_accepted(headers: &HeaderMap) -> Option<Lang> {
    let value = headers.get(header::ACCEPT_LANGUAGE)?.to_str().ok()?;
    let first = value.split(',').next()?;
    let name = first.split(';').next().unwrap_or_default().trim();
    Lang::normalize(name)
}

/// 使う言語: cookie → browser の言語の並びの初めの 1 つ → 日本語（`i18n.js` の `pick`）。
pub fn pick(headers: &HeaderMap) -> Lang {
    cookie(headers).unwrap_or_else(|| match first_accepted(headers) {
        Some(Lang::En) => Lang::En,
        _ => Lang::Ja,
    })
}

/// 要求の言語を決め、その要求を捌く間、[`asked`] で読めるようにする（道のすぐ外の層）。
pub async fn scope(request: Request, next: Next) -> Response {
    let lang = pick(request.headers());
    ASKED.scope(lang, next.run(request)).await
}

/// 今の要求の言語。[`scope`] の外（試験が呼ぶ先をじかに呼ぶ時など）では日本語。
pub fn asked() -> Lang {
    ASKED.try_with(|lang| *lang).unwrap_or(Lang::Ja)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(pairs: &[(header::HeaderName, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(name.clone(), HeaderValue::from_static(value));
        }
        map
    }

    #[test]
    fn 何も無ければ日本語() {
        assert_eq!(pick(&HeaderMap::new()), Lang::Ja);
    }

    #[test]
    fn browser_の言語の並びの初めの_1_つを見る() {
        let en = headers(&[(header::ACCEPT_LANGUAGE, "en-US,en;q=0.9,ja;q=0.8")]);
        assert_eq!(pick(&en), Lang::En);
        // 初めが ja・en のどちらでもなければ日本語（後ろの en は見ない。i18n.js と同じ）。
        let fr = headers(&[(header::ACCEPT_LANGUAGE, "fr-FR,en;q=0.9")]);
        assert_eq!(pick(&fr), Lang::Ja);
        let ja = headers(&[(header::ACCEPT_LANGUAGE, "ja,en;q=0.9")]);
        assert_eq!(pick(&ja), Lang::Ja);
        let any = headers(&[(header::ACCEPT_LANGUAGE, "*")]);
        assert_eq!(pick(&any), Lang::Ja);
        // 重みは見ない（初めの 1 つ）。
        let weighted = headers(&[(header::ACCEPT_LANGUAGE, "EN;q=0.1, ja")]);
        assert_eq!(pick(&weighted), Lang::En);
    }

    #[test]
    fn cookie_が_browser_の言語より先() {
        let chosen = headers(&[
            (header::COOKIE, "a=1; callboard-lang=ja; b=2"),
            (header::ACCEPT_LANGUAGE, "en-US"),
        ]);
        assert_eq!(pick(&chosen), Lang::Ja);
        let english = headers(&[
            (header::COOKIE, "callboard-lang=en"),
            (header::ACCEPT_LANGUAGE, "ja"),
        ]);
        assert_eq!(pick(&english), Lang::En);
    }

    #[test]
    fn 読めない_cookie_は無い物とみなす() {
        let broken = headers(&[
            (header::COOKIE, "callboard-lang=%E0%A4"),
            (header::ACCEPT_LANGUAGE, "en"),
        ]);
        assert_eq!(pick(&broken), Lang::En);
        // 似た名の cookie は見ない。
        let other = headers(&[(header::COOKIE, "xcallboard-lang=en")]);
        assert_eq!(pick(&other), Lang::Ja);
    }

    #[test]
    fn scope_の外では日本語() {
        assert_eq!(asked(), Lang::Ja);
    }
}
