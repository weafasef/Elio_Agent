//! REST API 路由

use axum::{extract::State, routing::get, Json, Router};
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

pub fn create_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/health", get(health_check))
}

async fn health_check(State(_state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
