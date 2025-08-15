use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct AdjustRequest {
    pub chat_id: i64,
    pub user_id: u64,
    pub delta: i32,
    pub reason: String,
    pub locale: Option<String>,
    // pub source: Option<String>,
    #[serde(alias = "quiet")]
    pub silent: Option<bool>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AdjustResponse {
    Applied { new_length: i32 },
    Skipped { reason: String },
}

impl AdjustResponse {
    pub fn new_applied(new_length: i32) -> Self {
        Self::Applied { new_length }
    }

    pub fn new_skipped(reason: String) -> Self {
        Self::Skipped { reason }
    }
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GetUserResponse {
    Ok {
        chat_id: i64,
        user_id: u64,
        name: String,
        length: i32,
        pos_in_top: Option<i64>,
    },
    Skipped { reason: String },
}

impl GetUserResponse {
    pub fn new_ok(chat_id: i64, user_id: u64, name: String, length: i32, pos_in_top: Option<i64>) -> Self {
        Self::Ok { chat_id, user_id, name, length, pos_in_top }
    }

    pub fn new_skipped(reason: String) -> Self {
        Self::Skipped { reason }
    }
}

#[derive(Serialize)]
pub struct GetTopResponse {
    pub users: Vec<GetUserResponse>,
}
