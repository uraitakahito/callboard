//! 設定（環境変数）。起きる時に 1 度読み、形が違えば起きない。
//!
//! | 名 | 中身 | 既定 |
//! |---|---|---|
//! | `CALLBOARD_LISTEN` | 待ち受ける所 | `0.0.0.0:8080` |
//! | `CALLBOARD_CASTLIST_URL` | castlist に聞く先 | 無い（要る） |
//! | `CALLBOARD_PAGETRAY_URL` | pagetray に聞く先 | 無い（要る） |
//! | `CALLBOARD_PAGETRAY_LINK_URL` | browser が pagetray を開く URL の頭 | `http://127.0.0.1:8801` |
//!
//! 聞く先と link の頭は別に持つ。callboard は container の中から聞き（`http://castlist.box:8080` など）、link は人が host の
//! browser で開く（`http://127.0.0.1:8801` など）ので、届く所が違う。どれも `http://<host>:<port>` の形だけを受ける。
//! castlist の link の頭（`CALLBOARD_CASTLIST_LINK_URL`。0.1.0 にあった）は、castlist の画面をみな callboard に移したので無くした
//! （書いてあっても読まない）。

/// 待ち受ける所の既定（container の中）。
pub const DEFAULT_LISTEN: &str = "0.0.0.0:8080";
/// link の頭の既定。pagetray を、host の browser から開く所。
pub const DEFAULT_PAGETRAY_LINK: &str = "http://127.0.0.1:8801";

/// 呼ぶ先のうち、まだ画面を持つ物（pagetray）の居場所。どちらも `http://<host>:<port>`（後ろの `/` は無い）。
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
    /// castlist に聞く先（`http://<host>:<port>`）。
    pub castlist: String,
    pub pagetray: Base,
    /// 送る側が返事の先を付け始めた時刻（UNIX のミリ秒）。
    pub exchanges_since: Option<i64>,
}

impl Settings {
    /// 環境変数から読む。形が違えば、その訳。
    pub fn from_env() -> Result<Self, String> {
        Self::read(|name| std::env::var(name).ok())
    }

    /// 名から値を引く関数で読む（試験は環境変数を使わずに呼ぶ）。空の値は、無いのと同じ。
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let get = |name: &str| get(name).filter(|v| !v.is_empty());
        let api = |name: &str| -> Result<String, String> {
            let name = format!("CALLBOARD_{name}_URL");
            let value = get(&name).ok_or_else(|| {
                format!("{name} が無い（聞く先を http://<host>:<port> の形で書く）")
            })?;
            parse_base(&value).map_err(|rule| format!("{name} は {rule}"))
        };
        let link =
            get("CALLBOARD_PAGETRAY_LINK_URL").unwrap_or_else(|| DEFAULT_PAGETRAY_LINK.to_owned());
        Ok(Self {
            // まだ読まない（試験を先に書いた）。
            exchanges_since: None,
            listen: get("CALLBOARD_LISTEN").unwrap_or_else(|| DEFAULT_LISTEN.to_owned()),
            castlist: api("CASTLIST")?,
            pagetray: Base {
                api: api("PAGETRAY")?,
                link: parse_base(&link)
                    .map_err(|rule| format!("CALLBOARD_PAGETRAY_LINK_URL は {rule}"))?,
            },
        })
    }
}

/// `http://<host>:<port>`（後ろの `/` は無くてよい）を、後ろの `/` の無い形にそろえる。path・利用者の名・`https://` は受けない
/// （pagetray が castlist の居場所を読む決まりと同じ）。
fn parse_base(url: &str) -> Result<String, String> {
    let rule = || format!("http://<host>:<port> の形（{url}）");
    let addr = url
        .strip_prefix("http://")
        .map(|rest| rest.strip_suffix('/').unwrap_or(rest))
        .ok_or_else(rule)?;
    let (host, port) = addr.rsplit_once(':').ok_or_else(rule)?;
    let host_ok = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'));
    let port_ok =
        port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok_and(|p| p > 0);
    if !host_ok || !port_ok {
        return Err(rule());
    }
    Ok(format!("http://{addr}"))
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
        assert_eq!(settings.castlist, "http://castlist.box:8080");
        // 後ろの / は落とす。
        assert_eq!(settings.pagetray.api, "http://pagetray:8080");
        assert_eq!(settings.pagetray.link, "http://127.0.0.1:8801");
    }

    #[test]
    fn 書いた値を使い_空の値は無いのと同じ() {
        let mut pairs = BOTH.to_vec();
        pairs.push(("CALLBOARD_LISTEN", "127.0.0.1:18813"));
        // 0.1.0 にあった castlist の link の頭は、書いてあっても読まない（形が違っても断らない）。
        pairs.push(("CALLBOARD_CASTLIST_LINK_URL", "なんでも"));
        pairs.push(("CALLBOARD_PAGETRAY_LINK_URL", "http://localhost:18801"));
        let settings = read(&pairs).unwrap();
        assert_eq!(settings.listen, "127.0.0.1:18813");
        assert_eq!(settings.pagetray.link, "http://localhost:18801");
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
            ("CALLBOARD_PAGETRAY_LINK_URL", "http://127.0.0.1:8801/x"),
            ("CALLBOARD_PAGETRAY_LINK_URL", "http://:8801"),
        ] {
            let mut pairs: Vec<_> = BOTH.iter().filter(|(n, _)| *n != name).copied().collect();
            pairs.push((name, value));
            let error = read(&pairs).unwrap_err();
            assert!(error.contains(name) && error.contains(value), "{error}");
        }
    }

    #[test]
    fn 返事の先を付け始めた時刻は_rfc_3339_で読み_無ければ無い_形が違えば起きない() {
        let base = [
            ("CALLBOARD_CASTLIST_URL", "http://c:1"),
            ("CALLBOARD_PAGETRAY_URL", "http://p:2"),
        ];
        assert_eq!(read(&base).unwrap().exchanges_since, None);
        let with = |value: &'static str| {
            let mut pairs = base.to_vec();
            pairs.push(("CALLBOARD_EXCHANGES_SINCE", value));
            read(&pairs)
        };
        // 2026-11-28T04:00:00Z（日本の時刻で書いても同じ時）。
        assert_eq!(
            with("2026-11-28T04:00:00Z").unwrap().exchanges_since,
            Some(1_795_838_400_000)
        );
        assert_eq!(
            with("2026-11-28T13:00:00+09:00").unwrap().exchanges_since,
            Some(1_795_838_400_000)
        );
        assert_eq!(with("").unwrap().exchanges_since, None);
        assert_eq!(
            with("2026-11-28 04:00").unwrap_err(),
            "CALLBOARD_EXCHANGES_SINCE は RFC 3339 の時刻（2026-11-28T08:00:00Z の形。2026-11-28 04:00）"
        );
    }
}
