use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use log::debug;
use rust_i18n::t;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

use crate::{
    domain::{AdjustRequest, AdjustResponse, GetUserResponse},
    repo::{ChatIdKind, Repositories},
};

use axum::{
    body::Body,
    http::{header, Request as HttpRequest},
    middleware::{self, Next},
    response::Response,
};

#[derive(Clone)]
pub struct ApiState {
    pub repos: Repositories,
    pub bot: Bot,
    pub api_key: String,
}

pub fn router(state: ApiState) -> Router {
    let shared = Arc::new(state);
    Router::new()
        .route("/api/v1/adjust", post(adjust))
        .route("/api/v1/user/:chat_id/:user_id", get(get_user))
        .route_layer(middleware::from_fn_with_state(shared.clone(), auth_bearer))
        .with_state(shared)
}

async fn adjust(
    State(state): State<Arc<ApiState>>,
    Json(payload): Json<AdjustRequest>,
) -> Result<(StatusCode, Json<AdjustResponse>), (StatusCode, String)> {
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
            Json(AdjustResponse::new_skipped(
                "user_not_registered".to_string(),
            )),
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

    if payload.silent != Some(true) {
        let key = if payload.delta >= 0 {
            "api.adjust.increased"
        } else {
            "api.adjust.decreased"
        };
        let text = t!(
            key,
            locale = &lang,
            name = name,
            delta = payload.delta.abs(),
            length = res.new_length,
            reason = payload.reason
        );
        let _ = state
            .bot
            .send_message(chat_id, text)
            .parse_mode(ParseMode::Html)
            .await;
    }

    Ok((
        StatusCode::OK,
        Json(AdjustResponse::new_applied(res.new_length)),
    ))
}

async fn get_user(
    State(state): State<Arc<ApiState>>,
    Path((chat_id, user_id)): Path<(i64, u64)>,
) -> Result<(StatusCode, Json<GetUserResponse>), (StatusCode, String)> {
    let chat_id = teloxide::types::ChatId(chat_id);
    let uid = teloxide::types::UserId(user_id);

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
            Json(GetUserResponse::new_skipped(
                "user_not_registered".to_string(),
            )),
        ));
    }

    let dick = state
        .repos
        .dicks
        .fetch_dick(uid, &ChatIdKind::ID(chat_id))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(d) = dick {
        Ok((
            StatusCode::OK,
            Json(GetUserResponse::new_ok(
                chat_id.0,
                uid.0,
                d.owner_name,
                d.length,
                d.position,
            )),
        ))
    } else {
        Ok((
            StatusCode::OK,
            Json(GetUserResponse::new_skipped(
                "user_not_registered".to_string(),
            )),
        ))
    }
}

// Middleware: check Bearer token against ApiState.api_key
async fn auth_bearer(
    State(state): State<Arc<ApiState>>,
    req: HttpRequest<Body>,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = provided
        .strip_prefix("Bearer ")
        .map(str::trim)
        .unwrap_or("");

    if state.api_key.is_empty() || token != state.api_key {
        return Err((StatusCode::UNAUTHORIZED, "unauthorized".into()));
    }

    Ok(next.run(req).await)
}
