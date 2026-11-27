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

/// 誤りの型の名（`X-Amzn-Errortype`）の鍵。知らない型は `errors.kind.other`。
fn kind_key(kind: &str) -> &'static str {
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
/// 呼ぶ先の `message` は、そのまま残す（日本語の字は、鍵の字 `{{detail}}` が `message` と同じになる）。
/// JSON の object でない本文と、`code` を持つ本文は、そのまま返す。
pub fn with_code(kind: Option<&str>, body: &[u8]) -> Option<Vec<u8>> {
    let Ok(Value::Object(mut map)) = serde_json::from_slice::<Value>(body) else {
        return None;
    };
    if map.contains_key("code") {
        return None;
    }
    let detail = map
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
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

    #[test]
    fn 運んだ誤りに_型の鍵と訳を足す() {
        let body = with_code(
            Some("UnsupportedMediaType"),
            r#"{"message":"png でない"}"#.as_bytes(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["message"], "png でない");
        assert_eq!(value["code"], "errors.kind.unsupportedMediaType");
        assert_eq!(value["values"]["detail"], "png でない");
        let other: Value =
            serde_json::from_slice(&with_code(Some("Teapot"), b"{}").unwrap()).unwrap();
        assert_eq!(other["code"], "errors.kind.other");
        assert_eq!(with_code(None, b"not json"), None);
        assert_eq!(with_code(None, br#"{"code":"x"}"#), None);
    }
}
