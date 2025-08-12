use super::ChatIdPartiality;
use crate::repository;
use anyhow::Context;
use teloxide::types::UserId;

#[derive(sqlx::Type, Debug, Clone, Copy)]
#[sqlx(type_name = "text")]
pub enum TransferKind {
    Gift,
    Fire,
}

repository!(
    Transfers,
    with_(chats)_(Chats),
    pub async fn log(
        &self,
        chat_id: &ChatIdPartiality,
        from: UserId,
        to: UserId,
        amount: i32,
        kind: TransferKind,
    ) -> anyhow::Result<()> {
        let internal_chat_id = self.chats.upsert_chat(chat_id).await?;
        let kind_str = match kind { TransferKind::Gift => "gift", TransferKind::Fire => "fire" };
        sqlx::query(r#"INSERT INTO transfers(chat_id, from_uid, to_uid, amount, kind)
               VALUES ($1, $2, $3, $4, $5)"#)
        .bind(internal_chat_id)
        .bind(from.0 as i64)
        .bind(to.0 as i64)
        .bind(amount)
        .bind(kind_str)
        .execute(&self.pool)
        .await
        .context("couldn't insert transfer record")?;
        Ok(())
    }
);
