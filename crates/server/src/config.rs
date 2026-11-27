//! 設定（環境変数）。起きる時に 1 度読み、形が違えば起きない。
//!
//! | 名 | 中身 | 既定 |
//! |---|---|---|
//! | `CALLBOARD_LISTEN` | 待ち受ける所 | `0.0.0.0:8080` |
//! | `CALLBOARD_CASTLIST_URL` | castlist に聞く先 | 無い（要る） |
//! | `CALLBOARD_CASTLIST_LINK_URL` | browser が castlist を開く URL の頭 | `http://127.0.0.1:8802` |
//! | `CALLBOARD_PAGETRAY_URL` | pagetray に聞く先 | 無い（要る） |
//! | `CALLBOARD_PAGETRAY_LINK_URL` | browser が pagetray を開く URL の頭 | `http://127.0.0.1:8801` |
//!
//! 聞く先と link の頭は別に持つ。callboard は container の中から聞き（`http://castlist.box:8080` など）、link は人が host の
//! browser で開く（`http://127.0.0.1:8802` など）ので、届く所が違う。どれも `http://<host>:<port>` の形だけを受ける。

/// 呼ぶ先 1 つ（castlist か pagetray）の居場所。どちらも `http://<host>:<port>`（後ろの `/` は無い）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base {
    /// 聞く先。
    pub api: String,
    /// browser が開く URL の頭。
    pub link: String,
}

/// 読んだ設定。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub listen: String,
    pub castlist: Base,
    pub pagetray: Base,
}

impl Settings {
    /// 環境変数から読む。形が違えば、その訳。
    pub fn from_env() -> Result<Self, String> {
        Self::read(|name| std::env::var(name).ok())
    }

    /// 名から値を引く関数で読む（試験は環境変数を使わずに呼ぶ）。まだ読まない。
    pub fn read(_get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        Err("まだ無い".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(pairs: &[(&str, &str)]) -> Result<Settings, String> {
        Settings::read(|name| {
            pairs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v).to_owned())
        })
    }

    const BOTH: [(&str, &str); 2] = [
        ("CALLBOARD_CASTLIST_URL", "http://castlist.box:8080"),
        ("CALLBOARD_PAGETRAY_URL", "http://pagetray:8080/"),
    ];

    #[test]
    fn 聞く先だけを書けば_ほかは既定() {
        let settings = read(&BOTH).unwrap();
        assert_eq!(settings.listen, "0.0.0.0:8080");
        assert_eq!(settings.castlist.api, "http://castlist.box:8080");
        assert_eq!(settings.castlist.link, "http://127.0.0.1:8802");
        // 後ろの / は落とす。
        assert_eq!(settings.pagetray.api, "http://pagetray:8080");
        assert_eq!(settings.pagetray.link, "http://127.0.0.1:8801");
    }

    #[test]
    fn 書いた値を使い_空の値は無いのと同じ() {
        let mut pairs = BOTH.to_vec();
        pairs.push(("CALLBOARD_LISTEN", "127.0.0.1:18813"));
        pairs.push(("CALLBOARD_CASTLIST_LINK_URL", "http://localhost:18802"));
        pairs.push(("CALLBOARD_PAGETRAY_LINK_URL", ""));
        let settings = read(&pairs).unwrap();
        assert_eq!(settings.listen, "127.0.0.1:18813");
        assert_eq!(settings.castlist.link, "http://localhost:18802");
        assert_eq!(settings.pagetray.link, "http://127.0.0.1:8801");
    }

    #[test]
    fn 聞く先が無ければ_名を書いて断る() {
        for missing in ["CALLBOARD_CASTLIST_URL", "CALLBOARD_PAGETRAY_URL"] {
            let pairs: Vec<_> = BOTH
                .iter()
                .filter(|(n, _)| *n != missing)
                .copied()
                .collect();
            let error = read(&pairs).unwrap_err();
            assert!(error.contains(missing), "{error}");
        }
    }

    #[test]
    fn 形の違う値は_名と値を書いて断る() {
        for (name, value) in [
            ("CALLBOARD_CASTLIST_URL", "https://castlist.box:8080"),
            ("CALLBOARD_CASTLIST_URL", "http://castlist.box"),
            ("CALLBOARD_CASTLIST_URL", "http://castlist.box:8080/api"),
            ("CALLBOARD_CASTLIST_URL", "http://user@castlist.box:8080"),
            ("CALLBOARD_PAGETRAY_URL", "http://pagetray:0"),
            ("CALLBOARD_PAGETRAY_URL", "pagetray:8080"),
            (
                "CALLBOARD_CASTLIST_LINK_URL",
                "http://127.0.0.1:8802/actors",
            ),
            ("CALLBOARD_PAGETRAY_LINK_URL", "http://:8801"),
        ] {
            let mut pairs: Vec<_> = BOTH.iter().filter(|(n, _)| *n != name).copied().collect();
            pairs.push((name, value));
            let error = read(&pairs).unwrap_err();
            assert!(error.contains(name) && error.contains(value), "{error}");
        }
    }
}
