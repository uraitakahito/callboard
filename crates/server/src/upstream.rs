//! 呼ぶ先（castlist・pagetray）。どちらも、その定義（Smithy）から作った client で呼ぶ（crates/castlist-client/・crates/pagetray-client/）。
//!
//! - HTTP の client は、TLS の無い HTTP/1.1（`aws-smithy-http-client` の hyper の作りの `build_http`）。呼ぶ先は同じ機械の中の
//!   `http://` だけなので、TLS の crate を build しない。接続は使い回さない（1 度ごとにつなぐ。古い接続で落ちて送り直す、が起きない）。
//! - 待つのは 1 度に [`TIMEOUT`]（つなぐ・送る・答えを読み終えるまでの全部）。送り直さない。
//! - castlist と pagetray は、`Host` が 127.0.0.1・localhost でない要求と、`Origin` の付いた書く要求を断る。だから、つなぐ先が
//!   どこでも `Host: 127.0.0.1` で送り、`Origin` は付けない（pagetray が castlist に聞く時と同じ）。
//! - 呼ぶ先の答えの `url` の欄は使わない（呼ぶ先は、要求の `Host` から URL を組むので、callboard から聞いた答えでは
//!   `http://127.0.0.1/…` になり、browser からは開けない）。画面の link は、答えの path を link の頭（[`crate::config`]）で組み直す。
//! - 聞けなかった時は、どの呼ぶ先か・聞く先・訳を記録（stdout）に 1 行残す。画面には、決まった字の訳だけを出し、聞く先の URL と
//!   中の誤りの字は出さない。
pub mod castlist;
pub mod pagetray;

use std::time::Duration;

use castlist_client::error::SdkError;

use crate::ui::i18n::Text;

/// 1 度に待つ長さ（つなぐ・送る・答えを読み終えるまでの全部）。callboard 自身の時間切れ（[`crate::REQUEST_TIMEOUT`]）より短くする
/// （呼ぶ先が答えない時に、守りの層の時間切れより先に、どこが答えないかを書いた画面を返すため）。
pub const TIMEOUT: Duration = Duration::from_secs(4);

/// 生きているかを聞く時に待つ長さ（入口の画面。呼ぶ先が止まっていても、画面を待たせない）。
pub const HEALTH_TIMEOUT: Duration = Duration::from_secs(1);

/// 呼ぶ先の、生きている答え。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Health {
    /// 呼ぶ先の版。
    pub version: String,
}

/// 呼べなかった訳。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// 名指した物が無い（呼ぶ先の 404 `NotFound`）。
    NotFound,
    /// 呼ぶ先が、引数を断った（400 `ValidationException`。中は、呼ぶ先の訳の字）。送り直しても直らない。
    Rejected(String),
    /// 聞けなかった（訳は [`reason`] の字）。
    Unavailable(Text),
}

/// 答えをそのまま browser に返す道が使う、生の HTTP の client（TLS の無い HTTP/1.1。接続は使い回さない）。
pub(crate) type Raw = hyper_util::client::legacy::Client<
    hyper_util::client::legacy::connect::HttpConnector,
    axum::body::Body,
>;

pub(crate) fn raw_client() -> Raw {
    hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .pool_max_idle_per_host(0)
        .build_http()
}

/// 呼ぶ先に渡す HTTP の client。
fn http_client() -> castlist_client::config::SharedHttpClient {
    aws_smithy_http_client::Builder::new()
        .pool_max_idle_per_host(0)
        .build_http()
}

/// 生成した client の頼みに付ける頭（`.customize().mutate_request(ask())`）: `Host: 127.0.0.1`（上の決まり）と、
/// 今の要求の言語（`Accept-Language: ja` か `en`。[`crate::lang`]）。言語は、呼ぶ先が断った訳の字の言語になる。
/// 言語は、頼みを組む時（要求を捌いている task の内）に読んで持たせる。
fn ask() -> impl Fn(&mut castlist_client::config::http::HttpRequest) + Send + Sync + 'static {
    let language = crate::lang::asked().code();
    move |request| {
        let headers = request.headers_mut();
        headers.insert("host", "127.0.0.1");
        headers.insert("accept-language", language);
    }
}

/// 聞けなかった訳（画面に出す決まった字）。詳しい字（聞く先と、中の誤り）は、ここで記録に書く。
/// castlist の client と pagetray の client の誤りは、同じ型（smithy の `SdkError`）。
fn reason<E, R>(name: &str, api: &str, e: &SdkError<E, R>) -> Text
where
    E: std::error::Error + Send + Sync + 'static,
    R: std::fmt::Debug + Send + Sync + 'static,
    SdkError<E, R>: StatusOf,
{
    tracing::warn!(upstream = name, api, error = %castlist_client::error::DisplayErrorContext(e), "呼ぶ先に聞けない");
    match e {
        SdkError::TimeoutError(_) => Text::new(SLOW),
        SdkError::DispatchFailure(e) if e.is_timeout() => Text::new(SLOW),
        SdkError::DispatchFailure(_) => Text::new(UNREACHABLE),
        SdkError::ServiceError(_) | SdkError::ResponseError(_) => match e.status() {
            Some(status) if !(200..300).contains(&status) => returned(status),
            _ => Text::new(UNREADABLE),
        },
        _ => Text::new(UNREADABLE),
    }
}

/// 聞けなかった訳の鍵（字は `static/locales/`）。
const SLOW: &str = "upstream.slow";
const UNREACHABLE: &str = "upstream.unreachable";
const UNREADABLE: &str = "upstream.unreadable";

/// 呼ぶ先が 2xx でない状態を返した訳。
fn returned(status: u16) -> Text {
    Text::new("upstream.returned").with_num("status", status)
}

/// 誤りに付いている、HTTP の状態。
trait StatusOf {
    fn status(&self) -> Option<u16>;
}

impl<E> StatusOf for SdkError<E, castlist_client::config::http::HttpResponse> {
    fn status(&self) -> Option<u16> {
        self.raw_response().map(|r| r.status().as_u16())
    }
}

/// 生きているかを [`HEALTH_TIMEOUT`] の内に聞く。過ぎれば、聞けなかった訳（記録にも残す）。
async fn within<T>(
    name: &str,
    api: &str,
    call: impl Future<Output = Result<T, Text>>,
) -> Result<T, Text> {
    match tokio::time::timeout(HEALTH_TIMEOUT, call).await {
        Ok(answer) => answer,
        Err(_) => {
            tracing::warn!(
                upstream = name,
                api,
                error = "health timeout",
                "呼ぶ先に聞けない"
            );
            Err(Text::new(SLOW))
        }
    }
}

/// 読めない答え（形は JSON でも、要る欄が無い・値が違う）。記録に残し、画面に出す訳を返す。
fn unreadable(name: &str, api: &str) -> Text {
    tracing::warn!(
        upstream = name,
        api,
        error = "unreadable answer",
        "呼ぶ先に聞けない"
    );
    Text::new(UNREADABLE)
}
