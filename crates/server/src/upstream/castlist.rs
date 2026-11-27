//! castlist を呼ぶ所（決まりは [`super`]）。
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use castlist_client::Client;
use castlist_client::config::retry::RetryConfig;
use castlist_client::config::timeout::TimeoutConfig;
use castlist_client::config::{BehaviorVersion, Config};
use castlist_client::operation::get_actor::GetActorOutput;
use castlist_client::operation::get_lineage::GetLineageOutput;

use super::{Failure, Health, Raw, TIMEOUT, http_client, raw_client, reason, unreadable, within};
use crate::config::Base;
use crate::errors::reply;

/// 記録に書く、呼ぶ先の名。
const NAME: &str = "castlist";

/// castlist の居場所と、呼ぶ client。
#[derive(Clone, Debug)]
pub struct Castlist {
    /// 聞く先（`http://<host>:<port>`）。
    api: String,
    /// browser が castlist を開く URL の頭（`http://<host>:<port>`）。
    link: String,
    client: Client,
    /// 答えをそのまま browser に返す道（[`Castlist::graph`]）が使う、生の HTTP の client。
    raw: Raw,
}

impl Castlist {
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

    /// browser が castlist を開く URL の頭（`http://<host>:<port>`）。
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

    /// アクター 1 つ（GetActor）。
    pub async fn actor(&self, id: &str) -> Result<GetActorOutput, Failure> {
        self.client
            .get_actor()
            .actor_id(id)
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map_err(|e| match e.as_service_error() {
                Some(e) if e.is_not_found() => Failure::NotFound,
                _ => Failure::Unavailable(reason(NAME, &self.api, &e)),
            })
    }

    /// アクターから、起こした・継いだ線を上り下りした先（GetLineage。上は人の根まで、下は 1 段）。
    pub async fn lineage(&self, id: &str) -> Result<GetLineageOutput, Failure> {
        self.client
            .get_lineage()
            .actor_id(id)
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map_err(|e| match e.as_service_error() {
                Some(e) if e.is_not_found() => Failure::NotFound,
                _ => Failure::Unavailable(reason(NAME, &self.api, &e)),
            })
    }

    /// 図の点と線（GetGraph。`GET /api/graph`）の答えを、そのまま返す（相関図の script が browser から引く）。
    /// `query` は、問い合わせの引数の字（`label=…&from=…`。castlist が確かめる）。渡すのは引数だけで、browser の header は渡さない。
    /// 返すのは、状態・本文の型・誤りの型の名（`X-Amzn-Errortype`）・本文。聞けなければ 503 `Unavailable`。
    pub async fn graph(&self, query: Option<&str>) -> Response {
        let uri = match query {
            Some(query) => format!("{}/api/graph?{query}", self.api),
            None => format!("{}/api/graph", self.api),
        };
        let unavailable = |why: &str| {
            reply(
                StatusCode::SERVICE_UNAVAILABLE,
                "Unavailable",
                &format!("castlist に聞けません（{why}）"),
            )
        };
        let request = match Request::get(&uri)
            .header(header::HOST, "127.0.0.1")
            .header(header::ACCEPT, "application/json")
            .body(Body::empty())
        {
            Ok(request) => request,
            // 引数の字が URL として読めない（守りの層を通った要求では起きない見込み）。
            Err(_) => {
                return reply(
                    StatusCode::BAD_REQUEST,
                    "ValidationException",
                    "引数を読めません",
                );
            }
        };
        match tokio::time::timeout(TIMEOUT, self.raw.request(request)).await {
            Err(_) => {
                tracing::warn!(
                    upstream = NAME,
                    api = self.api,
                    error = "timeout",
                    "呼ぶ先に聞けない"
                );
                unavailable("時間内に答えがありません")
            }
            Ok(Err(e)) => {
                tracing::warn!(upstream = NAME, api = self.api, error = %e, "呼ぶ先に聞けない");
                unavailable("つながりません")
            }
            Ok(Ok(answer)) => {
                let (parts, body) = answer.into_parts();
                let mut response = Response::new(Body::new(body));
                *response.status_mut() = parts.status;
                for name in [header::CONTENT_TYPE.as_str(), "x-amzn-errortype"] {
                    if let Some(value) = parts.headers.get(name) {
                        response.headers_mut().insert(
                            header::HeaderName::from_bytes(name.as_bytes()).expect("header の名"),
                            value.clone(),
                        );
                    }
                }
                response
            }
        }
    }
}
