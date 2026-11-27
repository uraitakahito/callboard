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

/// 呼ぶ先に渡す HTTP の client。
fn http_client() -> castlist_client::config::SharedHttpClient {
    aws_smithy_http_client::Builder::new()
        .pool_max_idle_per_host(0)
        .build_http()
}

/// 聞けなかった訳（画面に出す決まった字）。詳しい字（聞く先と、中の誤り）は、ここで記録に書く。
/// castlist の client と pagetray の client の誤りは、同じ型（smithy の `SdkError`）。
fn reason<E, R>(name: &str, api: &str, e: &SdkError<E, R>) -> String
where
    E: std::error::Error + Send + Sync + 'static,
    R: std::fmt::Debug + Send + Sync + 'static,
    SdkError<E, R>: StatusOf,
{
    tracing::warn!(upstream = name, api, error = %castlist_client::error::DisplayErrorContext(e), "呼ぶ先に聞けない");
    match e {
        SdkError::TimeoutError(_) => SLOW.to_owned(),
        SdkError::DispatchFailure(e) if e.is_timeout() => SLOW.to_owned(),
        SdkError::DispatchFailure(_) => "つながりません".to_owned(),
        SdkError::ServiceError(_) | SdkError::ResponseError(_) => match e.status() {
            Some(status) if !(200..300).contains(&status) => format!("{status} が返りました"),
            _ => UNREADABLE.to_owned(),
        },
        _ => UNREADABLE.to_owned(),
    }
}

const SLOW: &str = "時間内に答えがありません";
const UNREADABLE: &str = "答えを読めません";

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
    call: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    match tokio::time::timeout(HEALTH_TIMEOUT, call).await {
        Ok(answer) => answer,
        Err(_) => {
            tracing::warn!(
                upstream = name,
                api,
                error = "health timeout",
                "呼ぶ先に聞けない"
            );
            Err(SLOW.to_owned())
        }
    }
}

/// 読めない答え（形は JSON でも、要る欄が無い・値が違う）。記録に残し、画面に出す訳を返す。
fn unreadable(name: &str, api: &str) -> String {
    tracing::warn!(
        upstream = name,
        api,
        error = "unreadable answer",
        "呼ぶ先に聞けない"
    );
    UNREADABLE.to_owned()
}
