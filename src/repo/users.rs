use anyhow::Context;
use chrono::{DateTime, Utc};
use teloxide::types::UserId;

use crate::domain::{Ratio, Username};
use crate::repo::ChatIdKind;
use crate::repository;

#[derive(sqlx::FromRow, Debug)]
pub struct User {
    pub uid: i64,
    pub name: Username,
    pub created_at: DateTime<Utc>,
}

repository!(
    Users,
    pub async fn create_or_update(&self, user_id: UserId, name: &str) -> anyhow::Result<User> {
        let uid = user_id.0 as i64;
        sqlx::query_as!(
            User,
            "INSERT INTO Users(uid, name) VALUES ($1, $2)
                ON CONFLICT (uid) DO UPDATE SET name = $2
                RETURNING uid, name, created_at",
            uid,
            name
        )
        .fetch_one(&self.pool)
        .await
        .context(format!("couldn't upsert a user with id = {user_id}"))
    },
    pub async fn get_chat_members(&self, chat_id: &ChatIdKind) -> anyhow::Result<Vec<User>> {
        sqlx::query_as!(
            User,
            "SELECT u.uid, name, created_at FROM Users u
                JOIN Dicks d USING (uid)
                JOIN Chats c ON d.chat_id = c.id
                WHERE c.chat_id = $1::bigint OR c.chat_instance = $1::text",
            chat_id.value() as String
        )
        .fetch_all(&self.pool)
        .await
        .context(format!(
            "couldn't get users of the chat with id = {chat_id}"
        ))
    },
    pub async fn get_random_active_member(
        &self,
        chat_id: &ChatIdKind,
    ) -> anyhow::Result<Option<User>> {
        sqlx::query_as!(
            User,
            "SELECT u.uid, name, u.created_at FROM Users u
                JOIN Dicks d USING (uid)
                JOIN Chats c ON d.chat_id = c.id
                WHERE (c.chat_id = $1::bigint OR c.chat_instance = $1::text)
                    AND updated_at > current_timestamp - interval '1 week'
                ORDER BY random() LIMIT 1",
            chat_id.value() as String
        )
        .fetch_optional(&self.pool)
        .await
        .context(format!(
            "couldn't get a random active user of the chat with id = {chat_id}"
        ))
    },
    pub async fn get_random_active_poor_member(
        &self,
        chat_id: &ChatIdKind,
        rich_exclusion_ratio: Ratio,
    ) -> anyhow::Result<Option<User>> {
        sqlx::query_as!(User,
            "WITH ranked_users AS (
                SELECT u.uid, name, u.created_at, PERCENT_RANK() OVER (ORDER BY length) AS percentile_rank
                    FROM Users u
                    JOIN Dicks d USING (uid)
                    JOIN Chats c ON d.chat_id = c.id
                    WHERE (c.chat_id = $1::bigint OR c.chat_instance = $1::text)
                        AND updated_at > current_timestamp - interval '1 week'
            )
            SELECT uid, name, created_at
            FROM ranked_users
            WHERE percentile_rank <= $2
            ORDER BY random() LIMIT 1",
                chat_id.value() as String, 1.0 - rich_exclusion_ratio.to_value())
            .fetch_optional(&self.pool)
            .await
            .context(format!("couldn't get a random active poor user of the chat with id = {chat_id}"))
    },
    pub async fn get_random_active_member_with_poor_in_priority(
        &self,
        chat_id: &ChatIdKind,
    ) -> anyhow::Result<Option<User>> {
        sqlx::query_as!(User,
            "WITH user_weights AS (
                SELECT u.uid, u.name, u.created_at, d.length,
                       1.0 / (1.0 + EXP(d.length / 6.0)) AS weight  -- Sigmoid-like transformation
                FROM Users u
                  JOIN Dicks d USING (uid)
                  JOIN Chats c ON d.chat_id = c.id
                WHERE (c.chat_id = $1::bigint OR c.chat_instance = $1::text)
                  AND d.updated_at > current_timestamp - interval '1 week'
            ),
                 cumulative_weights AS (
                     SELECT uid, name, created_at, weight,
                            SUM(weight) OVER (ORDER BY uid) AS cumulative_weight, -- Cumulative weight
                            SUM(weight) OVER () AS total_weight
                     FROM user_weights
                 ),
                 random_value AS (
                     SELECT RANDOM() * (SELECT total_weight FROM cumulative_weights LIMIT 1) AS rand_value  -- Generate one random value
                 )
            SELECT uid, name, created_at
            FROM cumulative_weights, random_value
            WHERE cumulative_weight >= random_value.rand_value
            ORDER BY cumulative_weight
            LIMIT 1;  -- Select the first user whose cumulative weight exceeds the random value",
                chat_id.value() as String)
            .fetch_optional(&self.pool)
            .await
            .context(format!("couldn't get a random active user of the chat with id = {chat_id}"))
    },
    pub async fn get(&self, user_id: UserId) -> anyhow::Result<Option<User>> {
        sqlx::query_as!(
            User,
            "SELECT uid, name, created_at FROM Users WHERE uid = $1",
            user_id.0 as i64
        )
        .fetch_optional(&self.pool)
        .await
        .context(format!("couldn't get a user with id = {user_id}"))
    },
    pub async fn delete_everything(&self, user_id: UserId) -> anyhow::Result<u64> {
        let mut tx = self.pool.begin().await?;

        sqlx::query!("DELETE FROM transfers WHERE from_uid = $1 OR to_uid = $1", user_id.0 as i64)
            .execute(&mut *tx)
            .await
            .context("couldn't delete transfers for user")?;

        sqlx::query!("DELETE FROM Promo_Code_Activations WHERE uid = $1", user_id.0 as i64)
            .execute(&mut *tx)
            .await
            .context("couldn't delete promo code activations for user")?;

        sqlx::query!("DELETE FROM Loans WHERE uid = $1", user_id.0 as i64)
            .execute(&mut *tx)
            .await
            .context("couldn't delete loans for user")?;

        sqlx::query!("DELETE FROM Dick_of_Day WHERE winner_uid = $1", user_id.0 as i64)
            .execute(&mut *tx)
            .await
            .context("couldn't delete Dick_of_Day records for user")?;

        sqlx::query!("DELETE FROM Battle_Stats WHERE uid = $1", user_id.0 as i64)
            .execute(&mut *tx)
            .await
            .context("couldn't delete battle stats for user")?;

        sqlx::query!("DELETE FROM Imports WHERE uid = $1", user_id.0 as i64)
            .execute(&mut *tx)
            .await
            .context("couldn't delete imports for user")?;

        sqlx::query!("DELETE FROM Dicks WHERE uid = $1", user_id.0 as i64)
            .execute(&mut *tx)
            .await
            .context("couldn't delete dicks for user")?;

        let res = sqlx::query!("DELETE FROM Users WHERE uid = $1", user_id.0 as i64)
            .execute(&mut *tx)
            .await
            .context(format!("couldn't delete a user with id = {user_id}"))?;

        tx.commit().await?;
        Ok(res.rows_affected())
    },
    // pub async fn get_by_username(&self, username: &Username) -> anyhow::Result<Option<User>> {
    //     sqlx::query_as!(
    //         User,
    //         "SELECT uid, name, created_at FROM Users WHERE name = $1",
    //         username.value_ref() as String
    //     )
    //     .fetch_optional(&self.pool)
    //     .await
    //     .context(format!("couldn't get a user with username = {username}"))
    // },
    #[cfg(test)]
    pub async fn get_all(&self) -> anyhow::Result<Vec<User>> {
        sqlx::query_as!(User, "SELECT * FROM Users")
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
    }
);
