use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use log::debug;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

use crate::repo::{ChatIdKind, Repositories};

#[derive(Clone)]
pub struct ApiState {
    pub repos: Repositories,
    pub bot: Bot,
    pub api_key: String,
}

#[derive(Deserialize)]
pub struct AdjustRequest {
    pub chat_id: i64,
    pub user_id: u64,
    pub delta: i32,
    pub reason: String,
    pub locale: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum AdjustResponse {
    Applied { new_length: i32 },
    Skipped { reason: String },
    Duplicate,
}

use once_cell::sync::Lazy;
use std::collections::hash_map::Entry;
use std::time::{Duration, Instant};
static IDEMPOTENCY: Lazy<parking_lot::Mutex<std::collections::HashMap<String, Instant>>> =
    Lazy::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

const IDEMPOTENCY_TTL: Duration = Duration::from_secs(60 * 60 * 24);

fn check_and_remember_idempotency(key: &str) -> bool {
    let now = Instant::now();
    let mut map = IDEMPOTENCY.lock();
    if map.len() > 1000 {
        map.retain(|_, t| now.duration_since(*t) < IDEMPOTENCY_TTL);
    }
    match map.entry(key.to_string()) {
        Entry::Occupied(entry) => {
            if now.duration_since(*entry.get()) < IDEMPOTENCY_TTL {
                return false;
            }
            *entry.into_mut() = now;
            true
        }
        Entry::Vacant(v) => {
            v.insert(now);
            true
        }
    }
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/v1/adjust", post(adjust))
        .with_state(Arc::new(state))
}

async fn adjust(
    State(state): State<Arc<ApiState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<AdjustRequest>,
) -> Result<(StatusCode, Json<AdjustResponse>), (StatusCode, String)> {
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = provided
        .strip_prefix("Bearer ")
        .map(str::trim)
        .unwrap_or("");
    if state.api_key.is_empty() || token != state.api_key {
        return Err((StatusCode::UNAUTHORIZED, "unauthorized".into()));
    }

    if let Some(req_id) = headers.get("x-request-id").and_then(|v| v.to_str().ok()) {
        if !check_and_remember_idempotency(req_id) {
            return Ok((StatusCode::OK, Json(AdjustResponse::Duplicate)));
        }
    }

    debug!(
        "adjusting dick {:?} for user {:?}",
        payload.delta, payload.user_id
    );

    let chat_id = teloxide::types::ChatId(payload.chat_id);
    let uid = teloxide::types::UserId(payload.user_id);
    let lang = payload.locale.unwrap_or_else(|| "ru".to_string());

    let has = state
        .repos
        .dicks
        .is_user_has_dick(uid, &ChatIdKind::ID(chat_id))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    debug!("user {:?} has dick: {}", uid, has);

    if !has {
        return Ok((
            StatusCode::OK,
            Json(AdjustResponse::Skipped {
                reason: "user_not_registered".to_string(),
            }),
        ));
    }

    let name = state
        .repos
        .dicks
        .fetch_dick(uid, &ChatIdKind::ID(chat_id))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(|dick| dick.owner_name)
        .unwrap_or(format!("id:{}", uid.0));

    debug!("user {:?} dick name: {}", uid, name);

    let res = state
        .repos
        .dicks
        .grow_no_attempts_check(&ChatIdKind::ID(chat_id), uid, payload.delta)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    debug!("user {:?} dick length: {}", uid, res.new_length);

    let text = if payload.delta >= 0 {
        t!(
            "api.adjust.increased",
            locale = &lang,
            name = name,
            delta = payload.delta,
            length = res.new_length,
            reason = payload.reason
        )
    } else {
        t!(
            "api.adjust.decreased",
            locale = &lang,
            name = name,
            delta = -payload.delta,
            length = res.new_length,
            reason = payload.reason
        )
    };

    let _ = state
        .bot
        .send_message(chat_id, text)
        .parse_mode(ParseMode::Html)
        .await;

    Ok((
        StatusCode::OK,
        Json(AdjustResponse::Applied {
            new_length: res.new_length,
        }),
    ))
}
