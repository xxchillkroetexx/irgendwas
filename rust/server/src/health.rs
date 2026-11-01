//! `GET /health`: 200 if the database answers, 503 otherwise. For container healthchecks.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::MySqlPool;

pub async fn health(State(pool): State<MySqlPool>) -> (StatusCode, Json<Value>) {
    match app::server::db::ping(&pool).await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "status": "ok", "database": "ok" })),
        ),
        Err(err) => {
            tracing::error!(error = %err, "health check: database unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "error", "database": "unreachable" })),
            )
        }
    }
}
