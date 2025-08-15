use crate::repository;
use anyhow::Context;
use num_traits::ToPrimitive;
use sqlx::FromRow;
use teloxide::types::UserId;

#[derive(FromRow)]
struct PersonalStatsEntity {
    chats: Option<i64>,
    max_length: Option<i32>,
    total_length: Option<i64>,
    given_cm: Option<i64>,
    received_cm: Option<i64>,
}

pub struct PersonalStats {
    pub chats: u64,
    pub max_length: i32,
    pub total_length: i64,
    pub given_cm: i64,
    pub received_cm: i64,
}

impl From<PersonalStatsEntity> for PersonalStats {
    fn from(value: PersonalStatsEntity) -> Self {
        Self {
            chats: value
                .chats
                .map(|x| {
                    x.to_u64()
                        .expect("chats count, fetched from the database, must fit into u64")
                })
                .unwrap_or_default(),
            max_length: value.max_length.unwrap_or_default(),
            total_length: value.total_length.unwrap_or_default(),
            given_cm: value.given_cm.unwrap_or_default(),
            received_cm: value.received_cm.unwrap_or_default(),
        }
    }
}

repository!(
    PersonalStatsRepo,
    pub async fn get(&self, user_id: UserId) -> anyhow::Result<PersonalStats> {
        sqlx::query_as::<_, PersonalStatsEntity>(
            r#"SELECT count(chat_id) AS chats,
                          max(length) AS max_length,
                          sum(length) AS total_length,
                          (SELECT COALESCE(sum(amount), 0) FROM transfers WHERE from_uid = $1) AS given_cm,
                          (SELECT COALESCE(sum(amount), 0) FROM transfers WHERE to_uid = $1) AS received_cm
                   FROM Dicks WHERE uid = $1"#,
        )
        .bind(user_id.0 as i64)
        .fetch_one(&self.pool)
        .await
        .map(PersonalStats::from)
        .context(format!("couldn't get the personal stats of {user_id}"))
    }
);
