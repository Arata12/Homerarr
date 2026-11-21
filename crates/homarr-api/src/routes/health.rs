use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use homarr_auth::OptionalAuth;
use homarr_common::HomarrResult;
use homarr_db::repos::{apps, boards, integrations, users};
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn live() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let db_res = state.db.health_check().await;
    let redis_res = state.redis.health_check().await;
    let healthy = db_res.is_ok() && redis_res.is_ok();
    let code = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        Json(json!({
            "status": if healthy { "ready" } else { "degraded" },
            "database": match db_res {
                Ok(ms) => json!({ "ok": true, "latencyMs": ms }),
                Err(e) => json!({ "ok": false, "error": e.to_string() }),
            },
            "redis": match redis_res {
                Ok(ms) => json!({ "ok": true, "latencyMs": ms }),
                Err(e) => json!({ "ok": false, "error": e.to_string() }),
            }
        })),
    )
}

/// Anonymous callers only learn that the instance is up; version, flags and entity counts are
/// reserved for signed-in users.
pub async fn info(
    OptionalAuth(session): OptionalAuth,
    State(state): State<AppState>,
) -> HomarrResult<Json<Value>> {
    if session.is_none() {
        return Ok(Json(anonymous_info()));
    }
    let users_count = users::count(&state.db).await?;
    let boards_count = boards::list_all(&state.db).await?.len();
    let apps_count = apps::list_all(&state.db).await?.len();
    let integrations_count = integrations::list_all(&state.db).await?.len();
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "runtime": "rust",
        "demoMode": state.config.demo_mode,
        "demoReadOnly": state.config.demo_read_only,
        "kubernetesEnabled": state.config.enable_kubernetes,
        "noExternalConnection": state.config.no_external_connection,
        "counts": {
            "users": users_count,
            "boards": boards_count,
            "apps": apps_count,
            "integrations": integrations_count
        }
    })))
}

fn anonymous_info() -> Value {
    json!({ "runtime": "rust" })
}

pub async fn updates(State(state): State<AppState>) -> HomarrResult<Json<Value>> {
    let summary = state.services.external.check_updates().await?;
    Ok(Json(serde_json::to_value(summary).unwrap_or_default()))
}

pub async fn analytics(
    homarr_auth::RequireAdmin(_admin): homarr_auth::RequireAdmin,
    State(state): State<AppState>,
) -> HomarrResult<Json<Value>> {
    let snap = state
        .services
        .external
        .build_analytics_snapshot(&state.db)
        .await?;
    Ok(Json(serde_json::to_value(snap).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_info_hides_instance_details() {
        let v = anonymous_info();
        let obj = v.as_object().unwrap();
        for key in [
            "version",
            "counts",
            "demoMode",
            "kubernetesEnabled",
            "noExternalConnection",
        ] {
            assert!(!obj.contains_key(key), "{key} must not be public");
        }
    }
}
