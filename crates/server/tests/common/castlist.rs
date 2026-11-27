//! 偽の castlist の答え（アクター 1 つ・辿った先・図）。形は castlist の API（restJson1。欄は camelCase）の答えと同じ。
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
