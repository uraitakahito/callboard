//! 画面の字の多言語化（AI-50）: 取り込んだ i18next を配る道。
mod common;

use axum::http::{StatusCode, header};
use common::{App, get};
use serde_json::Value;

const I18NEXT_PATH: &str = "/static/vendor/i18next-27.2.9.min.js";

#[tokio::test]
async fn i18next_は_callboard_が_application_javascript_で_長く持たせて配る() {
    let app = App::start().await;
    let (status, headers, script) = app.send(get(I18NEXT_PATH)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/javascript; charset=utf-8"
    );
    assert_eq!(
        headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
        "nosniff"
    );
    assert_eq!(
        headers.get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=31536000, immutable"
    );
    assert!(headers.get(header::CONTENT_SECURITY_POLICY).is_none());
    // 配る i18next は、repo に取り込んだ file と同じ字（npm の物と同じかは ci/check-vendor.sh が見る）。
    let vendored = include_str!("../../../static/vendor/i18next-27.2.9.min.js");
    assert!(script == vendored);
}

/// 訳の file の鍵の表（入れ子を `a.b` の平らな鍵にする）。
fn keys(json: &str) -> std::collections::BTreeMap<String, String> {
    fn walk(prefix: &str, value: &Value, out: &mut std::collections::BTreeMap<String, String>) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    let key = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    walk(&key, value, out);
                }
            }
            Value::String(text) => {
                out.insert(prefix.to_owned(), text.clone());
            }
            other => panic!("{prefix} が字でない: {other}"),
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk("", &serde_json::from_str(json).unwrap(), &mut out);
    out
}

const JA: &str = include_str!("../../../static/locales/ja.json");
const EN: &str = include_str!("../../../static/locales/en.json");

/// 数で言い分ける印（`_one`・`_other` など。i18next の形）を落とした鍵。
fn base(key: &str) -> &str {
    for suffix in ["_zero", "_one", "_two", "_few", "_many", "_other"] {
        if let Some(stripped) = key.strip_suffix(suffix) {
            return stripped;
        }
    }
    key
}

#[test]
fn 日本語と英語の訳は_同じ鍵を持つ() {
    let en_keys = keys(EN);
    let en: std::collections::BTreeSet<&str> = en_keys.keys().map(|k| base(k)).collect();
    let ja_keys = keys(JA);
    let ja: std::collections::BTreeSet<&str> = ja_keys.keys().map(|k| base(k)).collect();
    assert_eq!(
        ja.difference(&en).collect::<Vec<_>>(),
        Vec::<&&str>::new(),
        "日本語にだけある鍵"
    );
    assert_eq!(
        en.difference(&ja).collect::<Vec<_>>(),
        Vec::<&&str>::new(),
        "英語にだけある鍵"
    );
    for (key, text) in keys(EN) {
        assert!(!text.trim().is_empty(), "英語の {key} が空");
    }
}

#[test]
fn 訳の差し込みは_名だけの形() {
    // サーバーは {{名}} だけを入れる（i18next の入れ子 $t(…)・{{n, number}} の形・{{- 名}} は、サーバーとブラウザーで字が食い違う）。
    for (lang, json) in [("ja", JA), ("en", EN)] {
        for (key, text) in keys(json) {
            // 入れ子の字（$t）と名の並び（$join）は、data-i18n-options の値の側にだけ置く（訳の file には書かない）。
            assert!(
                !text.contains("$t") && !text.contains("$join"),
                "{lang} の {key}: {text}"
            );
            let mut rest = text.as_str();
            while let Some(at) = rest.find("{{") {
                let end = rest[at..]
                    .find("}}")
                    .unwrap_or_else(|| panic!("{lang} の {key} の {{{{ が閉じていない: {text}"));
                let name = &rest[at + 2..at + end];
                assert!(
                    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                    "{lang} の {key} の差し込み {{{{{name}}}}} は名だけの形でない: {text}"
                );
                rest = &rest[at + end + 2..];
            }
        }
    }
}

#[tokio::test]
async fn 画面に書いた鍵は_みな訳にあり_字は訳の字と同じで_鍵の要素は子を持たない() {
    let app = App::start().await;
    for path in [
        "/",
        "/actors/",
        "/graph/timeline",
        "/graph/network",
        "/usage/",
        "/messages",
        "/pages/",
        "/no/such",
    ] {
        let (_, _, body) = app.send(get(path)).await;
        common::check_i18n(path, &body);
    }
}

/// template の file の中の字から、`"<鍵>"` の形で書いた鍵を拾う（`data-i18n="…"`・`data-i18n-attr="名:鍵;…"`・
/// `"…"|t`・`"…"|t1(`・`"…"|t2(`）。
fn template_keys(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, _) in source.match_indices("data-i18n=\"") {
        let rest = &source[at + 11..];
        out.push(rest[..rest.find('"').unwrap()].to_owned());
    }
    for (at, _) in source.match_indices("data-i18n-attr=\"") {
        let rest = &source[at + 16..];
        for pair in rest[..rest.find('"').unwrap()].split(';') {
            out.push(pair.split_once(':').unwrap().1.trim().to_owned());
        }
    }
    for filter in ["\"|t }}", "\"|t1(", "\"|t2("] {
        for (at, _) in source.match_indices(filter) {
            let open = source[..at].rfind('"').unwrap();
            out.push(source[open + 1..at].to_owned());
        }
    }
    out
}

/// Rust の source の字から、訳の file の頭の名（`nav.`・`errors.` など）で始まる `"…"` の字を、鍵として拾う
/// （`Text::new` に渡す鍵と、鍵の表の鍵）。
fn rust_keys(source: &str, heads: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find('"') {
        rest = &rest[at + 1..];
        let Some(end) = rest.find('"') else { break };
        let word = &rest[..end];
        if word.contains('.')
            && word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
            && heads.iter().any(|h| word.starts_with(&format!("{h}.")))
            // file の名（`home.html` など）と、鍵の頭（`events.`）は鍵でない。
            && !word.ends_with('.')
            && ![".html", ".js", ".css", ".json", ".svg"].iter().any(|ext| word.ends_with(ext))
        {
            out.push(word.to_owned());
        }
        rest = &rest[end + 1..];
    }
    out
}

/// `dir` の下の file を、みな読む（名と中身）。
async fn sources(dir: std::path::PathBuf) -> Vec<(std::path::PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir];
    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let path = entry.path();
            if entry.file_type().await.unwrap().is_dir() {
                stack.push(path);
            } else {
                let source = tokio::fs::read_to_string(&path).await.unwrap();
                out.push((path, source));
            }
        }
    }
    out
}

#[tokio::test]
async fn template_と_rust_の_source_に書いた鍵は_みな訳にある() {
    let ja = keys(JA);
    let has = |key: &str| ja.contains_key(key) || ja.contains_key(&format!("{key}_other"));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let heads: Vec<String> = serde_json::from_str::<Value>(JA)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let mut found = 0;
    for (path, source) in sources(root.join("../../templates")).await {
        for key in template_keys(&source) {
            found += 1;
            assert!(has(&key), "{} の鍵 {key} が訳に無い", path.display());
        }
    }
    for (path, source) in sources(root.join("src")).await {
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        for key in rust_keys(&source, &heads) {
            found += 1;
            assert!(has(&key), "{} の鍵 {key} が訳に無い", path.display());
        }
    }
    assert!(found > 300, "拾えた鍵が少ない: {found}");
}

#[tokio::test]
async fn 字の差し替えの_script_と訳は_head_で_defer_無しに読み_長く持たせて配る() {
    let app = App::start().await;
    let body = app.html("/").await;
    // 4 つが head に、i18next・日本語・英語・i18n.js の順に 1 つずつ（without_i18n_scripts が見る）。
    let _ = common::without_i18n_scripts(&body);
    // head の script の tag に defer・async は無い（同期で読む）。
    let head = &body[..body.find("</head>").unwrap()];
    for (at, _) in head.match_indices("<script") {
        let tag = &head[at..at + head[at..].find('>').unwrap()];
        assert!(!tag.contains("defer") && !tag.contains("async"), "{tag}");
    }
    for (prefix, content_type, file) in [
        (
            "/static/locales/ja.js?v=",
            "application/javascript; charset=utf-8",
            None,
        ),
        (
            "/static/locales/en.js?v=",
            "application/javascript; charset=utf-8",
            None,
        ),
        (
            "/static/i18n.js?v=",
            "application/javascript; charset=utf-8",
            Some(include_str!("../../../static/i18n.js")),
        ),
    ] {
        let at = body.find(prefix).unwrap_or_else(|| panic!("{prefix}"));
        let href = &body[at..at + body[at..].find('"').unwrap()];
        let v = &href[prefix.len()..];
        assert!(
            v.len() == 12 && v.bytes().all(|b| b.is_ascii_hexdigit()),
            "{href}"
        );
        let (status, headers, script) = app.send(get(href)).await;
        assert_eq!(status, StatusCode::OK, "{href}");
        assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), content_type);
        assert_eq!(
            headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );
        assert_eq!(
            headers.get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );
        if let Some(file) = file {
            assert!(script == file, "{href}");
        }
    }
    // 訳の script は、JSON をそのまま callboardLocales に置く（先に callboardLocales が無くても止まらない）。
    for (lang, json) in [("ja", JA), ("en", EN)] {
        let at = body.find(&format!("/static/locales/{lang}.js?v=")).unwrap();
        let href = &body[at..at + body[at..].find('"').unwrap()];
        let (_, _, script) = app.send(get(href)).await;
        let head = format!("(window.callboardLocales ??= {{}}).{lang} = ");
        assert!(script.starts_with(&head), "{script}");
        let value: Value =
            serde_json::from_str(script[head.len()..].trim_end().trim_end_matches(';')).unwrap();
        assert_eq!(value, serde_json::from_str::<Value>(json).unwrap());
    }
}

#[tokio::test]
async fn 言語の切り替えは_javascript_が出すまで隠し_どちらの言語も自分の言葉で書く() {
    let app = App::start().await;
    let body = app.html("/").await;
    let at = body
        .find("<span class=\"lang\" id=\"lang-switch\"")
        .expect("切り替え");
    let switch = &body[at..at + body[at..].find("</span>").unwrap()];
    assert!(switch.contains(" hidden>"), "{switch}");
    assert!(
        switch.contains(r#"<button type="button" data-lang="ja" lang="ja">日本語</button>"#),
        "{switch}"
    );
    assert!(
        switch.contains(r#"<button type="button" data-lang="en" lang="en">English</button>"#),
        "{switch}"
    );
}
