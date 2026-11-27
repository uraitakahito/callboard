//! castlist を呼ぶ所（決まりは [`super`]）。
use std::collections::HashSet;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use castlist_client::Client;
use castlist_client::config::retry::RetryConfig;
use castlist_client::config::timeout::TimeoutConfig;
use castlist_client::config::{BehaviorVersion, Config};
use castlist_client::operation::get_actor::GetActorOutput;
use castlist_client::operation::get_lineage::GetLineageOutput;
use castlist_client::operation::get_timeline::GetTimelineOutput;
use castlist_client::operation::list_actors::ListActorsOutput;
use castlist_client::operation::list_usage::ListUsageOutput;
use castlist_client::types::{Usage, UsageActor};
use http_body_util::BodyExt;

use super::{
    Failure, Health, Raw, SLOW, TIMEOUT, http_client, raw_client, reason, unreadable, within,
};
use crate::errors::reply;

/// 記録に書く、呼ぶ先の名。
const NAME: &str = "castlist";

/// アクターを探す条件（ListActors の引数。画面が確かめた物）。
#[derive(Clone, Debug, Default)]
pub struct ActorSearch {
    /// `鍵:値`。重ねれば、どれも持つアクター。
    pub labels: Vec<String>,
    /// `種類:値`。
    pub binding: Option<String>,
    pub at: Option<aws_smithy_types::DateTime>,
    pub active_from: Option<aws_smithy_types::DateTime>,
    pub active_to: Option<aws_smithy_types::DateTime>,
    pub kind: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub active: Option<bool>,
    pub launched_by: Option<String>,
    pub skewed: bool,
    pub sort: String,
    pub order: String,
    pub limit: Option<i32>,
    /// 次の頁の印（前の答えの nextToken）。
    pub after: Option<String>,
}

/// トークンの数と費えの行を読む条件（ListUsage の引数。画面が確かめた物）。
#[derive(Clone, Debug)]
pub struct UsageSearch {
    /// 区間の終わりの時刻の窓（`from <= toAt < to`）。時の境（UTC）に揃えた物。
    pub from: aws_smithy_types::DateTime,
    pub to: aws_smithy_types::DateTime,
    /// `鍵:値`。重ねれば、どれも持つアクターの行。
    pub labels: Vec<String>,
}

/// 窓の内の、トークンの数と費えの行（ListUsage の頁を、みな引いた物）。
#[derive(Clone, Debug)]
pub struct UsageAnswer {
    /// castlist が、初めの頁を読み始めた時刻。
    pub as_of: aws_smithy_types::DateTime,
    pub rows: Vec<Usage>,
    /// 行に出てきたアクター（頁をまたいで、id で 1 つにした物）。
    pub actors: Vec<UsageActor>,
    /// 途中で止めた（`rows` は、そこまでの分）。訳は [`Cut`]。
    pub truncated: Option<Cut>,
}

/// 頁を引くのを、途中で止めた訳。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cut {
    /// 頁が [`USAGE_MAX_PAGES`] を越えた。
    Pages,
    /// 全部の頁を待つ時間（[`TIMEOUT`]）を過ぎた。
    Time,
}

/// ListUsage の 1 頁の数（castlist の上限）と、引く頁の上限。越えたら、そこで止める（[`UsageAnswer::truncated`]）。
pub const USAGE_PAGE_SIZE: i32 = 1000;
pub const USAGE_MAX_PAGES: usize = 50;

/// castlist の居場所と、呼ぶ client。
#[derive(Clone, Debug)]
pub struct Castlist {
    /// 聞く先（`http://<host>:<port>`）。
    api: String,
    client: Client,
    /// 答えをそのまま browser に返す道（[`Castlist::graph`]）が使う、生の HTTP の client。
    raw: Raw,
}

impl Castlist {
    /// `api` は聞く先（`http://<host>:<port>`）。
    pub fn new(api: &str) -> Self {
        let config = Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(api)
            .http_client(http_client())
            .retry_config(RetryConfig::disabled())
            .timeout_config(TimeoutConfig::builder().operation_timeout(TIMEOUT).build())
            .build();
        Self {
            api: api.to_owned(),
            client: Client::from_conf(config),
            raw: raw_client(),
        }
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

    /// アクターを探す（ListActors。監査の検索）。条件は、画面が確かめた物。
    pub async fn actors(&self, search: &ActorSearch) -> Result<ListActorsOutput, Failure> {
        self.client
            .list_actors()
            .set_label((!search.labels.is_empty()).then(|| search.labels.clone()))
            .set_binding(search.binding.clone())
            .set_at(search.at)
            .set_active_from(search.active_from)
            .set_active_to(search.active_to)
            .set_kind(search.kind.clone())
            .set_model(search.model.clone())
            .set_effort(search.effort.clone())
            .set_active(search.active)
            .set_launched_by(search.launched_by.clone())
            .set_skewed(search.skewed.then_some(true))
            .sort(&search.sort)
            .order(&search.order)
            .set_max_results(search.limit)
            .set_next_token(search.after.clone())
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map_err(|e| match e.as_service_error() {
                // castlist が引数を断った。画面が確かめた物なので、ふつうは起きない（castlist が確かめを足した版に向けた時）。
                Some(e) if e.is_validation_error() => {
                    Failure::Rejected(e.meta().message().unwrap_or_default().to_owned())
                }
                _ => Failure::Unavailable(reason(NAME, &self.api, &e)),
            })
    }

    /// タイムラインを描く元の値（GetTimeline。castlist 1.1.0 から）。`label` は `鍵:値`、`from`・`to` は UTC の RFC 3339 の字
    /// （画面が確かめた物）。castlist が引数を断れば [`Failure::Rejected`]。
    pub async fn timeline(
        &self,
        label: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<GetTimelineOutput, Failure> {
        let time = |value: Option<&str>| {
            value.and_then(|v| {
                aws_smithy_types::DateTime::from_str(
                    v,
                    aws_smithy_types::date_time::Format::DateTime,
                )
                .ok()
            })
        };
        self.client
            .get_timeline()
            .set_label(label.map(str::to_owned))
            .set_from(time(from))
            .set_to(time(to))
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map_err(|e| match e.as_service_error() {
                // castlist が引数を断った。画面が確かめた物なので、ふつうは起きない（castlist が確かめを足した版に向けた時）。
                Some(e) if e.is_validation_error() => {
                    Failure::Rejected(e.meta().message().unwrap_or_default().to_owned())
                }
                _ => Failure::Unavailable(reason(NAME, &self.api, &e)),
            })
    }

    /// 窓の内の、トークンの数と費えの行（ListUsage。castlist 2.1.0 から）。次の頁の印で、行をみな引く（[`USAGE_MAX_PAGES`] 頁まで）。
    /// 待つのは、全部の頁で [`TIMEOUT`]（頁ごとではない。callboard 自身の時間切れより先に、訳を書いた画面を返すため）。
    /// castlist が引数を断れば [`Failure::Rejected`]。
    /// 初めの頁が時間内に読めなければ 503 `Unavailable`。2 頁目より後で時間を過ぎたら、そこまでに読めた行を返す
    /// （[`Cut::Time`]。頁の上限で止めた時と同じ扱い）。2 頁目より後で castlist が頁の印を断った時は、人には直せないので 503。
    pub async fn usage(&self, search: &UsageSearch) -> Result<UsageAnswer, Failure> {
        let deadline = tokio::time::Instant::now() + TIMEOUT;
        let slow = || {
            tracing::warn!(
                upstream = NAME,
                api = self.api,
                error = "usage timeout",
                "呼ぶ先に聞けない"
            );
        };
        let Ok(first) = tokio::time::timeout_at(deadline, self.usage_page(search, None)).await
        else {
            slow();
            return Err(Failure::Unavailable(SLOW.to_owned()));
        };
        let mut page = first?;
        let mut answer = UsageAnswer {
            as_of: page.as_of,
            rows: Vec::new(),
            actors: Vec::new(),
            truncated: None,
        };
        let mut seen = HashSet::new();
        for asked in 1.. {
            answer.rows.extend(page.usage);
            answer.actors.extend(
                page.actors
                    .into_iter()
                    .filter(|a| seen.insert(a.id.clone())),
            );
            let Some(token) = page.next_token else { break };
            if asked == USAGE_MAX_PAGES {
                answer.truncated = Some(Cut::Pages);
                break;
            }
            page = match tokio::time::timeout_at(deadline, self.usage_page(search, Some(token)))
                .await
            {
                Ok(Ok(next)) => next,
                // 次の頁の印を断られた: 人の打った問いの誤りでないので、503 の側に寄せる。
                Ok(Err(Failure::Rejected(why))) => {
                    return Err(Failure::Unavailable(format!("次の頁を断られました: {why}")));
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    slow();
                    answer.truncated = Some(Cut::Time);
                    break;
                }
            };
        }
        Ok(answer)
    }

    /// ListUsage の 1 頁。
    async fn usage_page(
        &self,
        search: &UsageSearch,
        next: Option<String>,
    ) -> Result<ListUsageOutput, Failure> {
        self.client
            .list_usage()
            .from(search.from)
            .to(search.to)
            .set_label((!search.labels.is_empty()).then(|| search.labels.clone()))
            .max_results(USAGE_PAGE_SIZE)
            .set_next_token(next)
            .customize()
            .mutate_request(|request| {
                request.headers_mut().insert("host", "127.0.0.1");
            })
            .send()
            .await
            .map_err(|e| match e.as_service_error() {
                // castlist が引数を断った。画面が確かめた物なので、ふつうは起きない（castlist が確かめを足した版に向けた時）。
                Some(e) if e.is_validation_error() => {
                    Failure::Rejected(e.meta().message().unwrap_or_default().to_owned())
                }
                // ListUsage を持たない古い castlist の 404 も、ここ（「404 が返りました」）。
                _ => Failure::Unavailable(reason(NAME, &self.api, &e)),
            })
    }

    /// 図の点と線（GetGraph。`GET /api/graph`）の答えを、そのまま返す（相関図の script が browser から引く）。
    /// `query` は、問い合わせの引数の字（`label=…&from=…`。castlist が確かめる）。渡すのは引数だけで、browser の header は渡さない。
    /// 返すのは、状態・本文の型・誤りの型の名（`X-Amzn-Errortype`）・本文。聞けなければ（つながらない・本文を読み終えるまで
    /// [`TIMEOUT`]）503 `Unavailable`。
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
        // 答えの頭だけでなく、本文を読み終えるまでを [`TIMEOUT`] で区切る（[`super`] の決まり）。図の答えは小さいので、
        // 読み終えてから返す（頭を返した後に本文が止まっても、browser を待たせ続けない）。
        let answer = tokio::time::timeout(TIMEOUT, async {
            let (parts, body) = self
                .raw
                .request(request)
                .await
                .map_err(|e| e.to_string())?
                .into_parts();
            let body = body.collect().await.map_err(|e| e.to_string())?;
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
