//! 画面でない答えの誤りの形。
use axum::http::{HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::ui::i18n::Text;

/// 守りの層などが返す誤りの答え。castlist・pagetray の API の誤りと同じ restJson1 の形にする:
/// header `X-Amzn-Errortype` に型の名（`kind`。ai.common の `Forbidden`・`PayloadTooLarge`・`Unavailable`）、本文は JSON の
/// `{"message":…,"code":…,"values":{…}}`。`message` は日本語の字、`code` はその字の鍵、`values` は差し込む値
/// （ブラウザーは `callboardT(code, values)` で、選んだ言語の字を組む）。
pub fn reply(status: StatusCode, kind: &'static str, message: &Text) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (HeaderName::from_static("x-amzn-errortype"), kind),
        ],
        serde_json::json!({
            "message": message.to_string(),
            "code": message.key(),
            "values": message.values(),
        })
        .to_string(),
    )
        .into_response()
}

/// 誤りの型の名（`X-Amzn-Errortype`）の鍵。知らない型は `errors.kind.other`。型の名は、名前空間（`#` の前）と、
/// 後ろに付く説明（`:` の後）を落としてから比べる。restJson1 の決まりの順（先に `:` の後を落とし、次に `#` の前を落とす。
/// `:` の後の URL に `#` があっても、名を取り違えない）。
fn kind_key(kind: &str) -> &'static str {
    let kind = kind.split_once(':').map_or(kind, |(name, _)| name);
    let kind = kind.rsplit_once('#').map_or(kind, |(_, name)| name).trim();
    match kind {
        "ValidationException" => "errors.kind.validation",
        "SerializationException" => "errors.kind.serialization",
        "UnsupportedMediaType" => "errors.kind.unsupportedMediaType",
        "PayloadTooLarge" => "errors.kind.payloadTooLarge",
        "NotFound" => "errors.kind.notFound",
        "Forbidden" => "errors.kind.forbidden",
        "Conflict" => "errors.kind.conflict",
        "Unavailable" => "errors.kind.unavailable",
        "InternalError" => "errors.kind.internal",
        _ => "errors.kind.other",
    }
}

/// 呼ぶ先（castlist・pagetray）から運んだ誤りの本文に、`code`（型の名の鍵）と `values`（`detail` に呼ぶ先の訳の字）を足す。
/// - 呼ぶ先の訳は `message`、無ければ `error` の字。どちらも無い本文は鍵を足さず、呼ぶ先の `code` だけを `upstreamCode` に
///   移す（`code` も無ければ None）。JSON の object でない本文は、そのまま返す（None）。
/// - 呼ぶ先の `message` は、そのまま残す（日本語の字は、鍵の字 `{{detail}}` が `message` と同じになる）。
/// - `code` は callboard が付けた物だけを信じる: いつも型の名から書き、呼ぶ先の本文に `code` があれば `upstreamCode` に移す。
pub fn with_code(kind: Option<&str>, body: &[u8]) -> Option<Vec<u8>> {
    let Ok(Value::Object(mut map)) = serde_json::from_slice::<Value>(body) else {
        return None;
    };
    let detail = ["message", "error"]
        .iter()
        .find_map(|name| map.get(*name).and_then(Value::as_str))
        .map(str::to_owned);
    let upstream = map.remove("code");
    let Some(detail) = detail else {
        // 訳が無いので鍵は足さない。呼ぶ先の code だけは移す（残すと、browser が字の鍵として組む）。
        let code = upstream?;
        map.insert("upstreamCode".to_owned(), code);
        return Some(Value::Object(map).to_string().into_bytes());
    };
    if let Some(code) = upstream {
        map.insert("upstreamCode".to_owned(), code);
    }
    map.insert(
        "code".to_owned(),
        Value::from(kind_key(kind.unwrap_or_default())),
    );
    map.insert("values".to_owned(), serde_json::json!({ "detail": detail }));
    Some(Value::Object(map).to_string().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(body: Option<Vec<u8>>) -> Value {
        serde_json::from_slice(&body.expect("code を足していない")).unwrap()
    }

    #[test]
    fn 型の名は_コロンの後を落としてから_シャープの前を落とす() {
        assert_eq!(kind_key("ValidationException"), "errors.kind.validation");
        assert_eq!(kind_key("ai.common#Forbidden"), "errors.kind.forbidden");
        assert_eq!(
            kind_key("ValidationException:http://internal/x#frag"),
            "errors.kind.validation"
        );
        assert_eq!(
            kind_key("ai.common#NotFound:http://internal/y#z"),
            "errors.kind.notFound"
        );
    }

    #[test]
    fn 運んだ誤りに_型の鍵と訳を足す() {
        let value = json(with_code(
            Some("UnsupportedMediaType"),
            r#"{"message":"png でない"}"#.as_bytes(),
        ));
        assert_eq!(value["message"], "png でない");
        assert_eq!(value["code"], "errors.kind.unsupportedMediaType");
        assert_eq!(value["values"]["detail"], "png でない");
        let other = json(with_code(Some("Teapot"), br#"{"message":"x"}"#));
        assert_eq!(other["code"], "errors.kind.other");
        assert_eq!(with_code(None, b"not json"), None);
        assert_eq!(with_code(None, b"[1]"), None);
    }

    #[test]
    fn 訳は_message_か_error_で_どちらも無ければ_code_を足さない() {
        let value = json(with_code(Some("NotFound"), br#"{"error":"no such"}"#));
        assert_eq!(value["code"], "errors.kind.notFound");
        assert_eq!(value["values"]["detail"], "no such");
        // message が先。
        let value = json(with_code(
            Some("NotFound"),
            br#"{"message":"m","error":"e"}"#,
        ));
        assert_eq!(value["values"]["detail"], "m");
        assert_eq!(with_code(Some("NotFound"), b"{}"), None);
        assert_eq!(with_code(Some("NotFound"), br#"{"message":3}"#), None);
    }

    #[test]
    fn 呼ぶ先の_code_は信じず_upstream_code_に移す() {
        let value = json(with_code(
            Some("Forbidden"),
            br#"{"message":"x","code":"nav.actors","values":{"a":1}}"#,
        ));
        assert_eq!(value["code"], "errors.kind.forbidden");
        assert_eq!(value["upstreamCode"], "nav.actors");
        assert_eq!(value["values"], serde_json::json!({"detail": "x"}));
    }

    #[test]
    fn 訳の無い本文でも_呼ぶ先の_code_は_upstream_code_に移す() {
        // 訳が無いので鍵は足さないが、呼ぶ先の code を残すと、browser がそれを字の鍵として組んでしまう。
        let value = json(with_code(
            Some("ValidationException"),
            br#"{"code":"nav.actors","values":{"a":1}}"#,
        ));
        assert_eq!(value.get("code"), None);
        assert_eq!(value["upstreamCode"], "nav.actors");
        assert_eq!(value["values"], serde_json::json!({"a": 1}));
    }

    #[test]
    fn 型の名は_名前空間と後ろの説明を落として比べる() {
        let body = br#"{"message":"x"}"#;
        for kind in [
            "ValidationException",
            "ai.common#ValidationException",
            "ValidationException:http://internal.amazon.com/coral/x/",
            "aws.protocols#ValidationException:http://x",
        ] {
            assert_eq!(
                json(with_code(Some(kind), body))["code"],
                "errors.kind.validation",
                "{kind}"
            );
        }
    }
}
