//! 偽の castlist の答え（アクター 1 つ・辿った先・図・トークンの使用量）。形は castlist の API（restJson1。欄は camelCase）の答えと同じ。
//!
//! 見本:
//!   pm.1（人）─起動した→ tpm.1 ─起動した→ se.1 ─引き継いだ→ se.2
//!                                         se.1 ─起動した→ sub.1
//! プロフィールを開くのは se.1（指定は opus、実際は sonnet。/clear で退いた）。
use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use serde_json::{Value, json};

use super::json as answer;

pub const PM: &str = "019a0000-0000-7000-8000-000000000001";
pub const TPM: &str = "019a0000-0000-7000-8000-000000000002";
pub const SE1: &str = "019a0000-0000-7000-8000-000000000003";
pub const SE2: &str = "019a0000-0000-7000-8000-000000000004";
pub const SUB: &str = "019a0000-0000-7000-8000-000000000005";
/// castlist に無いアクター。
pub const MISSING: &str = "019a0000-0000-7000-8000-0000000000ff";

/// 辿った先の行（ActorRow）。
pub fn row(id: &str, name: &str, kind: &str) -> Value {
    json!({
        "id": id, "name": name, "kind": kind, "issuer": "tpm",
        "sentAt": "2026-11-22T00:00:00Z", "receivedAt": "2026-11-22T00:00:00Z", "clockSkew": false,
        "labels": {}, "present": true, "inferred": false, "presence": "entered", "skewed": false,
        "url": format!("/actors/{id}"),
    })
}

pub fn link(id: &str, kind: &str, from: &str, to: &str) -> Value {
    json!({
        "id": id, "type": kind, "from": from, "to": to,
        "sentAt": "2026-11-22T00:00:00Z", "receivedAt": "2026-11-22T00:00:00Z", "clockSkew": false,
    })
}

fn setting(source: &str, model: &str, effort: &str, received: &str, skew: bool) -> Value {
    json!({
        "actorId": SE1, "model": model, "effort": effort, "source": source,
        "sentAt": received, "receivedAt": received, "clockSkew": skew,
    })
}

fn event(id: &str, kind: &str, detail: Value, sent: &str, received: &str, skew: bool) -> Value {
    json!({
        "id": id, "actorId": SE1, "type": kind, "detail": detail,
        "sentAt": sent, "receivedAt": received, "clockSkew": skew,
    })
}

fn binding(kind: &str, value: &str, is_key: bool, received: &str) -> Value {
    json!({
        "issuer": "tpm", "type": kind, "value": value, "isKey": is_key,
        "sentAt": received, "receivedAt": received,
    })
}

/// se.1 の GetActor の答え。
pub fn se1_actor() -> Value {
    json!({
        "id": SE1, "name": "se.1", "kind": "ai", "issuer": "tpm",
        "sentAt": "2026-11-22T01:00:00Z", "receivedAt": "2026-11-22T01:00:00.5Z", "clockSkew": false,
        "labels": {"tpm.role": "se", "tpm.key": "t-23zs", "note": "<b>太字</b> & \"引用\""},
        "bindings": [
            binding("claude.session", "s-se1", true, "2026-11-22T01:00:00.5Z"),
            binding("cmux.workspace", "workspace:1", false, "2026-11-22T01:00:00.5Z"),
            binding("cmux.workspace", "workspace:2", false, "2026-11-22T02:00:00Z"),
        ],
        "currentBindings": [
            binding("claude.session", "s-se1", true, "2026-11-22T01:00:00.5Z"),
            binding("cmux.workspace", "workspace:2", false, "2026-11-22T02:00:00Z"),
        ],
        "events": [
            event("e-1", "entered", json!({}), "2026-11-22T01:00:01Z", "2026-11-22T01:00:01Z", false),
            event(
                "e-2", "compacted",
                json!({"source": "auto", "reason": "context_full", "tokens": 1200, "ok": true}),
                "2026-11-22T03:00:00Z", "2026-11-22T03:00:00Z", false,
            ),
            // 送り手の時計が 1 時間ずれている。
            event("e-3", "cleared", json!({"reason": "clear"}), "2026-11-22T05:00:00Z", "2026-11-22T04:00:00Z", true),
        ],
        "settings": [
            setting("requested", "claude-opus-5-5", "high", "2026-11-22T01:00:00.5Z", false),
            setting("observed", "claude-sonnet-5", "high", "2026-11-22T01:30:00Z", false),
            setting("requested", "claude-sonnet-5", "high", "2026-11-22T02:00:00Z", false),
            setting("observed", "claude-sonnet-5", "high", "2026-11-22T02:30:00Z", false),
        ],
        "currentSettings": setting("observed", "claude-sonnet-5", "high", "2026-11-22T02:30:00Z", false),
        "linksIn": [link("l-tpm-se1", "launched", TPM, SE1)],
        "linksOut": [link("l-se1-se2", "continued", SE1, SE2), link("l-se1-sub", "launched", SE1, SUB)],
        "state": {
            "presence": "cleared", "present": false, "inferred": false,
            "enteredAt": "2026-11-22T01:00:01Z", "exitedAt": "2026-11-22T04:00:00Z",
        },
        "asOf": "2026-11-22T06:00:00Z",
    })
}

/// se.1 の GetLineage の答え（上は人の根まで、下は 1 段）。
pub fn se1_lineage() -> Value {
    let mut me = row(SE1, "se.1", "ai");
    me["labels"] = json!({"tpm.role": "se", "tpm.key": "t-23zs", "note": "<b>太字</b> & \"引用\""});
    me["present"] = json!(false);
    me["presence"] = json!("cleared");
    // 出来事に時刻ずれの行がある。
    me["skewed"] = json!(true);
    me["firstEnteredAt"] = json!("2026-11-22T01:00:01Z");
    me["lastExitedAt"] = json!("2026-11-22T04:00:00Z");
    me["lastExitType"] = json!("cleared");
    me["model"] = json!("claude-sonnet-5");
    me["effort"] = json!("high");
    json!({
        "asOf": "2026-11-22T06:00:00Z",
        "actorId": SE1,
        "nodes": [
            row(PM, "pm.1", "human"), row(TPM, "tpm.1", "ai"), me,
            row(SE2, "se.2", "ai"), row(SUB, "sub.1", "ai"),
        ],
        "edges": [
            link("l-pm-tpm", "launched", PM, TPM),
            link("l-tpm-se1", "launched", TPM, SE1),
            link("l-se1-se2", "continued", SE1, SE2),
            link("l-se1-sub", "launched", SE1, SUB),
        ],
    })
}

/// castlist の誤りの答え（restJson1: header に型の名・本文は JSON の message）。
pub fn error(status: StatusCode, kind: &'static str, message: &str) -> Response {
    let mut response = answer(status, &json!({ "message": message }).to_string());
    response
        .headers_mut()
        .insert("x-amzn-errortype", kind.parse().unwrap());
    response
}

pub fn not_found() -> Response {
    error(StatusCode::NOT_FOUND, "NotFound", "アクターは無い")
}

/// アクター 1 つと辿った先を、決めた答えで返す偽の castlist の道。`actor`・`lineage` は se.1 の答え（ほかの id は 404）。
pub fn routes(actor: Value, lineage: Value) -> Router {
    super::castlist_routes()
        .route(
            "/api/actors/{id}",
            get(move |Path(id): Path<String>| {
                let actor = actor.clone();
                async move {
                    if id == SE1 {
                        answer(StatusCode::OK, &actor.to_string())
                    } else {
                        not_found()
                    }
                }
            }),
        )
        .route(
            "/api/actors/{id}/lineage",
            get(move |Path(id): Path<String>| {
                let lineage = lineage.clone();
                async move {
                    if id == SE1 {
                        answer(StatusCode::OK, &lineage.to_string())
                    } else {
                        not_found()
                    }
                }
            }),
        )
}

// --- トークンの使用量（ListUsage。castlist 2.1.0 から） ---

/// 数の 1 行（Usage）。`agent` が在れば手伝い（subagent）の回。`to_at` は区間の終わりの時刻。`counts` は、既定の値に
/// 重ねる欄（`turns`・`costMicroUsd`・`maxContext`・`buckets` など）。
pub fn usage_row(id: &str, actor: &str, agent: Option<&str>, to_at: &str, counts: Value) -> Value {
    let mut row = json!({
        "id": id, "actorId": actor, "model": "claude-opus-5-5", "speed": "standard", "serviceTier": "standard",
        "fromAt": to_at, "toAt": to_at, "turns": 1,
        "inputTokens": 10, "outputTokens": 20, "cacheReadTokens": 30, "cacheWrite5mTokens": 0, "cacheWrite1hTokens": 40,
        "webSearchRequests": 0, "webFetchRequests": 0, "costMicroUsd": 0, "maxContext": 0,
        "buckets": [{"bucket": 0, "turns": 1, "costMicroUsd": 0}],
        "sentAt": to_at, "receivedAt": to_at, "clockSkew": false,
    });
    if let Some(agent) = agent {
        row["agent"] = json!(agent);
    }
    for (name, value) in counts.as_object().expect("counts は JSON の object") {
        row[name] = value.clone();
    }
    row
}

/// 段の並び（`(番号, 回の数, 額)`）。
pub fn usage_buckets(buckets: &[(i32, i64, i64)]) -> Value {
    buckets
        .iter()
        .map(
            |(bucket, turns, cost)| json!({"bucket": bucket, "turns": turns, "costMicroUsd": cost}),
        )
        .collect()
}

/// 行に出てきたアクター（UsageActor）。
pub fn usage_actor(id: &str, name: &str, labels: Value) -> Value {
    json!({"id": id, "name": name, "labels": labels})
}

/// ListUsage の答えの 1 頁。`next` が在れば、次の頁の印を付ける。
pub fn usage_page(rows: Vec<Value>, actors: Vec<Value>, next: Option<&str>) -> Value {
    let mut page = json!({"asOf": "2026-11-22T06:00:00Z", "usage": rows, "actors": actors});
    if let Some(next) = next {
        page["nextToken"] = json!(next);
    }
    page
}

/// [`usage_routes`] と同じだが、`nextToken=p<slow>` の問いにだけ、答える前に `delay` 待つ（途中の頁で時間切れになる castlist）。
pub fn usage_routes_slow(pages: Vec<Value>, slow: usize, delay: std::time::Duration) -> Router {
    super::castlist_routes().route(
        "/api/usage",
        get(
            move |axum::extract::RawQuery(query): axum::extract::RawQuery| {
                let token = query.as_deref().and_then(|q| {
                    q.split('&')
                        .find_map(|p| p.strip_prefix("nextToken=").map(str::to_owned))
                });
                let n = match token {
                    None => Some(0),
                    Some(token) => token
                        .strip_prefix('p')
                        .and_then(|n| n.parse::<usize>().ok()),
                };
                let page = n.and_then(|n| pages.get(n).cloned());
                async move {
                    if n == Some(slow) {
                        tokio::time::sleep(delay).await;
                    }
                    match page {
                        Some(page) => answer(StatusCode::OK, &page.to_string()),
                        None => error(
                            StatusCode::BAD_REQUEST,
                            "ValidationException",
                            "nextToken が読めない",
                        ),
                    }
                }
            },
        ),
    )
}

/// ListUsage（`GET /api/usage`）を返す偽の castlist の道。`pages[0]` は `nextToken` の無い問いへの答え、
/// `pages[n]` は `nextToken=p<n>` の問いへの答え（ほかの印は 400）。
pub fn usage_routes(pages: Vec<Value>) -> Router {
    with_usage(super::castlist_routes(), pages)
}

/// ほかの道（一覧・アクター 1 つ）に、ListUsage の道（[`usage_routes`] と同じ答え）を足す。
pub fn with_usage(routes: Router, pages: Vec<Value>) -> Router {
    routes.route(
        "/api/usage",
        get(
            move |axum::extract::RawQuery(query): axum::extract::RawQuery| {
                let token = query.as_deref().and_then(|q| {
                    q.split('&')
                        .find_map(|p| p.strip_prefix("nextToken=").map(str::to_owned))
                });
                let page = match token {
                    None => pages.first().cloned(),
                    Some(token) => token
                        .strip_prefix('p')
                        .and_then(|n| n.parse::<usize>().ok())
                        .and_then(|n| pages.get(n).cloned()),
                };
                async move {
                    match page {
                        Some(page) => answer(StatusCode::OK, &page.to_string()),
                        None => error(
                            StatusCode::BAD_REQUEST,
                            "ValidationException",
                            "nextToken が読めない",
                        ),
                    }
                }
            },
        ),
    )
}

/// 期間に稼働した席（ListActors の行）。ラベルだけを替える。
pub fn actor_row(id: &str, name: &str, labels: Value) -> Value {
    let mut row = row(id, name, "ai");
    row["labels"] = labels;
    row
}

/// ほかの道に、ListActors（`GET /api/actors`）の道を足す。どの問いにも `actors` を 1 頁で返す。
pub fn with_actors(routes: Router, actors: Vec<Value>) -> Router {
    let page = json!({"asOf": "2026-11-22T06:00:00Z", "actors": actors});
    routes.route(
        "/api/actors",
        get(move || {
            let page = page.clone();
            async move { answer(StatusCode::OK, &page.to_string()) }
        }),
    )
}

/// ほかの道に、ListActors を断る（503）道を足す（候補を引けない castlist）。
pub fn with_actors_down(routes: Router) -> Router {
    routes.route(
        "/api/actors",
        get(|| async {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "ServiceUnavailable",
                "落ちている",
            )
        }),
    )
}
