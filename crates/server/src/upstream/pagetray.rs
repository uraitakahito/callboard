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
pub use pagetray_client::operation::get_file_rules::GetFileRulesOutput;
pub use pagetray_client::operation::get_message_facets::GetMessageFacetsOutput;
pub use pagetray_client::operation::get_page_facets::GetPageFacetsOutput;
pub use pagetray_client::operation::list_messages::ListMessagesOutput;
pub use pagetray_client::operation::list_pages::ListPagesOutput;
pub use pagetray_client::types::{FacetCount, File, Message, Page, ReviewOf, Sender};

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

/// ページを探す条件（ListPages の引数。画面が、確かめた値だけを入れる）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageSearch {
    pub project: Option<String>,
    pub key: Option<String>,
    pub kind: Option<String>,
    pub role: Option<String>,
    /// workspace の ID。空の字も条件（書かずに置いたページ）。
    pub workspace: Option<String>,
    pub q: Option<String>,
    /// 返事の状態（`waiting`・`answered`）と、待ちの相手（`review`・`tpm`。Product Manager は書かない）。
    pub reply: Option<String>,
    pub on: Option<String>,
    pub verdict: Option<String>,
    /// 送り手（castlist のアクターの id）。
    pub actor: Option<String>,
    /// 古い順に並べるか。
    pub oldest: bool,
    /// 前の頁の答えの `nextToken`。
    pub after: Option<String>,
    /// 1 頁の数。
    pub limit: i32,
}

/// 呼べなかった訳にする: pagetray が引数を断った物は [`Failure::Rejected`]、ほかは [`Failure::Unavailable`]。
macro_rules! failed {
    ($self:ident) => {
        |e| match e.as_service_error() {
            Some(e) if e.is_validation_error() => {
                Failure::Rejected(e.meta().message().unwrap_or_default().to_owned())
            }
            _ => Failure::Unavailable(reason(NAME, &$self.api, &e)),
        }
    };
}

impl Pagetray {
    /// ページを探す（ListPages。置いた時刻の新しい順が既定）。
    pub async fn pages(&self, search: &PageSearch) -> Result<ListPagesOutput, Failure> {
        self.client
            .list_pages()
            .set_project(search.project.clone())
            .set_key(search.key.clone())
            .set_kind(search.kind.clone())
            .set_role(search.role.clone())
            .set_workspace(search.workspace.clone())
            .set_q(search.q.clone())
            .set_reply(search.reply.clone())
            .set_on(search.on.clone())
            .set_verdict(search.verdict.clone())
            .set_actor(search.actor.clone())
            .set_sort(search.oldest.then(|| "createdAt".to_owned()))
            .set_next_token(search.after.clone())
            .max_results(search.limit)
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map_err(failed!(self))
    }

    /// ページの、絞り込みの候補と件数・待ちの数（GetPageFacets。pagetray 1.3.0 から）。数は置き場の全部。
    pub async fn page_facets(&self) -> Result<GetPageFacetsOutput, Failure> {
        self.client
            .get_page_facets()
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map_err(|e| Failure::Unavailable(reason(NAME, &self.api, &e)))
    }

    /// 名指したページ（path）を見たレビューのページ（ListReviews。pagetray 1.3.0 から）。名指す物が無ければ、聞かずに空。
    pub async fn reviews(&self, targets: Vec<String>) -> Result<Vec<ReviewOf>, Failure> {
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        self.client
            .list_reviews()
            .set_target(Some(targets))
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map(|output| output.reviews)
            .map_err(failed!(self))
    }

    /// Key の file（ListFiles。新しい順に `limit` まで）。
    pub async fn files(&self, project: &str, key: &str, limit: i32) -> Result<Vec<File>, Failure> {
        self.client
            .list_files()
            .project(project)
            .key(key)
            .max_results(limit)
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map(|output| output.files)
            .map_err(failed!(self))
    }

    /// 受ける file の種類と、大きさの上限（GetFileRules。pagetray 1.3.0 から）。
    pub async fn file_rules(&self) -> Result<GetFileRulesOutput, Failure> {
        self.client
            .get_file_rules()
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map_err(|e| Failure::Unavailable(reason(NAME, &self.api, &e)))
    }

    /// 画面の form が上げた file を、pagetray に置く（CreateFile。`POST /api/projects/{project}/keys/{key}/files`）。
    /// - 役割は [`UPLOAD_ROLE`] に決める。browser からは名（`name`）・中身の種類・中身だけを渡し、workspace・番号・時刻・
    ///   付随する id は渡さない（browser から担当を名乗らせない・取り込ませない。pagetray の画面の form と同じ決まり）。
    /// - `project`・`key` は、呼ぶ側が形（URL の 1 段になる字）を見てある。
    /// - 返すのは、pagetray の答えの状態・本文の型・誤りの型の名・本文（置いた記録か、断った訳）。pagetray が要求の `Host` から
    ///   組んだ URL の header（`Location`）は返さない。聞けなければ（つながらない・答えを読み終えるまで [`TIMEOUT`]）503。
    pub async fn upload(
        &self,
        project: &str,
        key: &str,
        name: &str,
        content_type: Option<header::HeaderValue>,
        body: Body,
    ) -> Response {
        let unavailable = |why: &str| {
            reply(
                StatusCode::SERVICE_UNAVAILABLE,
                "Unavailable",
                &format!("pagetray に聞けません（{why}）"),
            )
        };
        let uri = format!(
            "{}/api/projects/{project}/keys/{key}/files?name={}&role={UPLOAD_ROLE}",
            self.api,
            escape(name)
        );
        let mut request = Request::post(&uri).header(header::HOST, "127.0.0.1");
        if let Some(content_type) = content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }
        let Ok(request) = request.body(body) else {
            return reply(
                StatusCode::BAD_REQUEST,
                "ValidationException",
                "引数を読めません",
            );
        };
        let answer = tokio::time::timeout(TIMEOUT, async {
            let (parts, body) = self
                .raw
                .request(request)
                .await
                .map_err(|e| e.to_string())?
                .into_parts();
            let body = http_body_util::BodyExt::collect(body)
                .await
                .map_err(|e| e.to_string())?;
            Ok::<_, String>((parts, body.to_bytes()))
        })
        .await;
        match answer {
            Err(_) => {
                tracing::warn!(
                    upstream = NAME,
                    api = self.api,
                    error = "timeout",
                    "呼ぶ先に聞けない"
                );
                unavailable("時間内に答えがありません")
            }
            Ok(Err(error)) => {
                tracing::warn!(upstream = NAME, api = self.api, error, "呼ぶ先に聞けない");
                unavailable("つながりません")
            }
            Ok(Ok((parts, body))) => {
                let mut response = Response::new(Body::from(body));
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

/// 画面の form から上げた file の役割（pagetray の画面の form と同じ）。
pub const UPLOAD_ROLE: &str = "product-manager";

/// URL の引数の値を % で包む（英数字と `-._~` の外）。
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// 新着の知らせの答えの型。
const EVENT_STREAM: &str = "text/event-stream";
