//! 守りの層。どの道（画面・配る file・/healthz）の前にも、外から順に掛ける（[`crate::app_with`]）:
//!
//! 1. [`stamp`]: どの答えにも型の推し量りを止める header などを付ける（断った答えにも付く）。
//! 2. [`guard`]: `Host` と `Origin` で断る。`HEAD` は `GET` と同じに答え、本文を捨てる。
//! 3. [`timeout`]: 答えの頭までに時間がかかりすぎれば 503。
//! 4. [`limit`]: 本文を上限まで読む。超えれば 413。
//!
//! castlist・pagetray の守りと同じ決まり（callboard は、その 2 つの画面を browser に出す所なので、同じ守りを自分の入口に持つ）。
//! 断る答えは [`reply`] の形。`Host`・`Origin` で断った要求は、記録に 1 行残す（問い合わせごとの行は出さない。
//! callboard は volume を持たず、記録は stdout だけなので）。
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use http_body_util::{BodyExt, LengthLimitError, Limited};

use crate::errors::reply;
use crate::ui::i18n::Text;
use crate::{BODY_LIMIT, CSP, UPLOAD_LIMIT};

/// host（callboard を動かす機械）の中からの要求だけを受ける。
/// - `Host` が 127.0.0.1・localhost でなければ断る（外の site が名前を付け替えて届ける要求を防ぐ）。
/// - `GET`・`HEAD` のほかの要求に `Origin` が付いていれば断る（browser の中のページからは書かせない）。
///   例外は 1 つだけ: ページの一覧の form が file を上げる道（[`is_upload`]）。この道は、この callboard の画面の form からの
///   要求（[`from_form`]）だけを通し、ほかは `Origin` が無くても断る。
///
/// `HEAD` は `GET` にして通し、答えの本文を捨てる（状態と header は `GET` と同じ）。
pub async fn guard(mut request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.rsplit_once(':').map_or(v, |(name, _port)| name));
    if !matches!(host, Some("127.0.0.1" | "localhost")) {
        refused("host", &request);
        return reply(
            StatusCode::FORBIDDEN,
            "Forbidden",
            &Text::new("errors.host"),
        );
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        // 書く道は 1 つだけ: ページの一覧の form が file を上げる道。この callboard の画面の form からの要求だけを通す。
        if request.method() == Method::POST && is_upload(request.uri().path()) {
            if !from_form(&request) {
                refused("form", &request);
                return reply(
                    StatusCode::FORBIDDEN,
                    "Forbidden",
                    &Text::new("errors.formOnly"),
                );
            }
        } else if request.headers().contains_key(header::ORIGIN) {
            refused("origin", &request);
            return reply(
                StatusCode::FORBIDDEN,
                "Forbidden",
                &Text::new("errors.origin"),
            );
        }
    }
    let head = request.method() == Method::HEAD;
    if head {
        *request.method_mut() = Method::GET;
    }
    let response = next.run(request).await;
    if head {
        // 本文を捨てる前に、本文の大きさが決まっていれば Content-Length を付ける（GET と同じ header にする。
        // 付けずに捨てると、外の層が空の本文の 0 を付ける）。流す本文（大きさが決まらない物）は付けない。
        let (mut parts, body) = response.into_parts();
        if !parts.headers.contains_key(header::CONTENT_LENGTH)
            && let Some(len) = http_body::Body::size_hint(&body).exact()
        {
            parts
                .headers
                .insert(header::CONTENT_LENGTH, HeaderValue::from(len));
        }
        Response::from_parts(parts, Body::empty())
    } else {
        response
    }
}

/// file を上げる道（`/api/pagetray/projects/{project}/keys/{key}/files`。後ろに何も付かない物）か。
pub fn is_upload(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/pagetray/projects/") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('/').collect();
    matches!(parts[..], [project, "keys", key, "files"] if !project.is_empty() && !key.is_empty())
}

/// この callboard の画面の form からの要求か（pagetray の画面の form を見る決まりと同じ）: `Origin` が `http://<Host（port も）>`
/// とそのまま同じで、`Sec-Fetch-Site` が無いか `same-origin`。ほかの site・本文の枠の中のページ（pagetray の origin か、
/// sandbox の `null`）・browser でない要求（`Origin` が無い）は当たらない。
fn from_form(request: &Request) -> bool {
    let text = |name: &str| request.headers().get(name).and_then(|v| v.to_str().ok());
    let same_origin = match (text("origin"), text("host")) {
        (Some(origin), Some(host)) => origin.strip_prefix("http://") == Some(host),
        _ => false,
    };
    same_origin && matches!(text("sec-fetch-site"), None | Some("same-origin"))
}

/// 断った要求を、記録に 1 行残す（訳・method・道・`Host`・`Origin`）。問い合わせごとの行は出さないので、
/// 「画面が 403 になる」時に訳を引けるのは、この行だけ。
fn refused(reason: &str, request: &Request) {
    let header = |name| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    };
    tracing::info!(
        reason,
        method = %request.method(),
        path = %request.uri().path(),
        host = header(header::HOST),
        origin = header(header::ORIGIN),
        "refused"
    );
}

/// `limit` の内に答えの頭を返せなければ、503 で返す（待っている物は捨てる）。
pub async fn timeout(State(limit): State<Duration>, request: Request, next: Next) -> Response {
    match tokio::time::timeout(limit, next.run(request)).await {
        Ok(response) => response,
        Err(_) => reply(
            StatusCode::SERVICE_UNAVAILABLE,
            "Unavailable",
            &too_slow(limit),
        ),
    }
}

/// 時間切れの訳。長さは、秒で割り切れれば秒、ほかは ms（単位は言語で替わるので、鍵を分ける）。
fn too_slow(d: Duration) -> Text {
    if d.subsec_nanos() == 0 {
        Text::new("errors.tooSlow.seconds").with_num("n", d.as_secs())
    } else {
        Text::new("errors.tooSlow.ms").with_num("n", d.as_millis())
    }
}

/// 本文を [`BODY_LIMIT`]（file を上げる道だけは [`UPLOAD_LIMIT`]）まで読んでから通す。超えれば 413 で断る
/// （`Content-Length` の有る無しを問わない）。
pub async fn limit(request: Request, next: Next) -> Response {
    let (parts, body) = request.into_parts();
    let max = if parts.method == Method::POST && is_upload(parts.uri.path()) {
        UPLOAD_LIMIT
    } else {
        BODY_LIMIT
    };
    match Limited::new(body, max).collect().await {
        Ok(body) => {
            next.run(Request::from_parts(parts, Body::from(body.to_bytes())))
                .await
        }
        Err(e) if e.is::<LengthLimitError>() => reply(
            StatusCode::PAYLOAD_TOO_LARGE,
            "PayloadTooLarge",
            &Text::new("errors.tooLarge").with_num("n", max / (1024 * 1024)),
        ),
        // 送り手が途中で切った、など。castlist・pagetray が読めない本文に返すのと同じ型にする。
        Err(e) => {
            tracing::info!(error = %e, "本文を読めない");
            reply(
                StatusCode::BAD_REQUEST,
                "SerializationException",
                &Text::new("errors.unreadableBody"),
            )
        }
    }
}

/// どの答えにも、型の推し量りを止める header と、referrer を送らせない header を付け、画面（HTML）の答えには CSP を付ける。
pub async fn stamp(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    let html = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));
    // 自分の CSP を付けた画面（ページの一覧）は、そのまま。
    if html && !headers.contains_key(header::CONTENT_SECURITY_POLICY) {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP),
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::too_slow;
    use std::time::Duration;

    #[test]
    fn 長さの字() {
        let seconds = too_slow(Duration::from_secs(5));
        assert!(seconds.to_string().contains("（5 秒）"), "{seconds}");
        assert_eq!(seconds.key(), "errors.tooSlow.seconds");
        let ms = too_slow(Duration::from_millis(200));
        assert!(ms.to_string().contains("（200 ms）"), "{ms}");
        assert_eq!(ms.key(), "errors.tooSlow.ms");
        assert_eq!(
            seconds.to_string(),
            "時間がかかりすぎた（5 秒）。少し待ってから送り直す"
        );
    }
}
