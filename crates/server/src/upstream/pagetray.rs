//! pagetray を呼ぶ所（決まりは [`super`]）。
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use futures_util::StreamExt;
use pagetray_client::Client;
use pagetray_client::config::retry::RetryConfig;
use pagetray_client::config::timeout::TimeoutConfig;
use pagetray_client::config::{BehaviorVersion, Config};
pub use pagetray_client::operation::get_message_facets::GetMessageFacetsOutput;
pub use pagetray_client::operation::list_messages::ListMessagesOutput;
pub use pagetray_client::types::{FacetCount, Message};

use super::{Failure, Health, Raw, TIMEOUT, http_client, raw_client, reason, unreadable, within};
use crate::Closing;
use crate::config::Base;
use crate::errors::reply;

/// 記録に書く、呼ぶ先の名。
const NAME: &str = "pagetray";

/// pagetray の居場所と、呼ぶ client。
#[derive(Clone, Debug)]
pub struct Pagetray {
    /// 聞く先（`http://<host>:<port>`）。
    api: String,
    /// browser が pagetray を開く URL の頭（`http://<host>:<port>`）。
    link: String,
    client: Client,
    /// 新着の知らせを、そのまま browser に流す道（[`Pagetray::events`]）が使う。
    raw: Raw,
}

/// メッセージの記録を探す条件（ListMessages の引数。画面が、空でない値だけを入れる）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageSearch {
    pub project: Option<String>,
    pub key: Option<String>,
    pub to: Option<String>,
    pub from: Option<String>,
    pub state: Option<String>,
    /// 前の頁の答えの `nextToken`。
    pub after: Option<String>,
    /// 1 頁の数。
    pub limit: i32,
}

impl Pagetray {
    pub fn new(base: &Base) -> Self {
        let config = Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(&base.api)
            .http_client(http_client())
            .retry_config(RetryConfig::disabled())
            .timeout_config(TimeoutConfig::builder().operation_timeout(TIMEOUT).build())
            .build();
        Self {
            api: base.api.clone(),
            link: base.link.clone(),
            client: Client::from_conf(config),
            raw: raw_client(),
        }
    }

    /// browser が pagetray を開く URL の頭（`http://<host>:<port>`）。
    pub fn link(&self) -> &str {
        &self.link
    }

    /// 生きているかを聞く（GetHealth）。聞けなければ、その訳。
    pub async fn health(&self) -> Result<Health, String> {
        within(NAME, &self.api, async {
            let output = self
                .client
                .get_health()
                .customize()
                .mutate_request(|request| {
                    request.headers_mut().insert("host", "127.0.0.1");
                })
                .send()
                .await
                .map_err(|e| reason(NAME, &self.api, &e))?;
            // 生成した client は、答えに無い `@required` の欄を既定の値（空の字）で埋める。status が ok でない答えは、読めない物とする。
            if output.status() != "ok" || output.version().is_empty() {
                return Err(unreadable(NAME, &self.api));
            }
            Ok(Health {
                version: output.version().to_owned(),
            })
        })
        .await
    }

    /// メッセージの記録を探す（ListMessages。送った時刻の新しい順）。pagetray が引数を断れば [`Failure::Rejected`]。
    pub async fn messages(&self, search: &MessageSearch) -> Result<ListMessagesOutput, Failure> {
        self.client
            .list_messages()
            .set_project(search.project.clone())
            .set_key(search.key.clone())
            .set_to(search.to.clone())
            .set_from(search.from.clone())
            .set_state(search.state.clone())
            .set_next_token(search.after.clone())
            .max_results(search.limit)
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map_err(|e| match e.as_service_error() {
                Some(e) if e.is_validation_error() => {
                    Failure::Rejected(e.meta().message().unwrap_or_default().to_owned())
                }
                _ => Failure::Unavailable(reason(NAME, &self.api, &e)),
            })
    }

    /// メッセージの記録の、絞り込みの候補と件数（GetMessageFacets。pagetray 1.2.0 から）。数は置き場の全部。
    /// `to`・`from`・`key` は、今絞っている値（絞らない。候補に必ず入る）。
    pub async fn message_facets(
        &self,
        to: Option<&str>,
        from: Option<&str>,
        key: Option<&str>,
    ) -> Result<GetMessageFacetsOutput, Failure> {
        self.client
            .get_message_facets()
            .set_to(to.map(str::to_owned))
            .set_from(from.map(str::to_owned))
            .set_key(key.map(str::to_owned))
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map_err(|e| match e.as_service_error() {
                Some(e) if e.is_validation_error() => {
                    Failure::Rejected(e.meta().message().unwrap_or_default().to_owned())
                }
                _ => Failure::Unavailable(reason(NAME, &self.api, &e)),
            })
    }

    /// 新着の知らせ（pagetray の `GET /api/events`。SSE）を、そのまま流す（メッセージの一覧の script が browser から聞く）。
    /// - 渡すのは道だけで、browser の header は渡さない。答えの頭を `wait` まで待ち、来なければ 503 `Unavailable`。
    ///   知らせの流れでない答え（200 でない・型が `text/event-stream` でない）も 503。
    /// - 流れには時間切れを付けない（つなぎっぱなし）。pagetray が閉じれば（起こし直しなど）、この流れも終わる
    ///   （browser の `EventSource` が、つなぎ直す）。browser が切れば、pagetray へのつなぎも切れる。
    /// - 止める合図（`closing`）を受けたら、流れを閉じる。
    pub async fn events(&self, wait: Duration, closing: Closing) -> Response {
        let unavailable = |why: &str| {
            reply(
                StatusCode::SERVICE_UNAVAILABLE,
                "Unavailable",
                &format!("pagetray に聞けません（{why}）"),
            )
        };
        let warn = |error: &str| {
            tracing::warn!(upstream = NAME, api = self.api, error, "呼ぶ先に聞けない");
        };
        let request = match Request::get(format!("{}/api/events", self.api))
            .header(header::HOST, "127.0.0.1")
            .header(header::ACCEPT, EVENT_STREAM)
            .body(Body::empty())
        {
            Ok(request) => request,
            Err(e) => {
                warn(&e.to_string());
                return unavailable("つながりません");
            }
        };
        let (parts, body) = match tokio::time::timeout(wait, self.raw.request(request)).await {
            Err(_) => {
                warn("timeout");
                return unavailable("時間内に答えがありません");
            }
            Ok(Err(e)) => {
                warn(&e.to_string());
                return unavailable("つながりません");
            }
            Ok(Ok(response)) => response.into_parts(),
        };
        let stream = parts
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with(EVENT_STREAM));
        if parts.status != StatusCode::OK {
            warn(&format!("status {}", parts.status.as_u16()));
            return unavailable(&format!("{} が返りました", parts.status.as_u16()));
        }
        if !stream {
            warn("not an event stream");
            return unavailable("答えを読めません");
        }
        let body = Body::new(body)
            .into_data_stream()
            .take_until(closing.cancelled_owned());
        let mut response = Response::new(Body::from_stream(body));
        let headers = response.headers_mut();
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static(EVENT_STREAM),
        );
        headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-cache"),
        );
        response
    }
}

/// 新着の知らせの答えの型。
const EVENT_STREAM: &str = "text/event-stream";
