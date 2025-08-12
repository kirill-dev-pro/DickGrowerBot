use crate::domain::LanguageCode;
use crate::handlers::{reply_html, HandlerResult};
use crate::reply_html;
use crate::repo;
use rust_i18n::t;
use teloxide::macros::BotCommands;
use teloxide::requests::Requester;
use teloxide::types::{Message, UserId};
use teloxide::Bot;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum AdminCommands {
    #[command(description = "deluser")]
    Deluser,
}

pub async fn admin_cmd_handler(
    bot: Bot,
    msg: Message,
    _cmd: AdminCommands,
    repos: repo::Repositories,
) -> HandlerResult {
    let lang_code = LanguageCode::from_maybe_user(msg.from.as_ref());

    let is_admin = {
        let admin_ids = bot
            .get_chat_administrators(msg.chat.id)
            .await?
            .into_iter()
            .map(|m| m.user.id)
            .collect::<Vec<_>>();
        let from_id = msg
            .from
            .as_ref()
            .map(|u| u.id)
            .ok_or_else(|| anyhow::anyhow!("not from a user"))?;
        admin_ids.into_iter().any(|id| id == from_id)
    };
    if !is_admin {
        let answer = t!("commands.deluser.errors.not_admin", locale = &lang_code).to_string();
        reply_html!(bot, msg, answer);
        return Ok(());
    }

    let target_uid: Option<UserId> = if let Some(reply) = msg.reply_to_message() {
        reply.from.as_ref().map(|u| u.id)
    } else {
        msg.text().and_then(|t| {
            let mut parts = t.split_whitespace();
            let _cmd = parts.next()?;
            let id_str = parts.next()?;
            id_str.parse::<u64>().ok().map(UserId)
        })
    };

    let target_uid = match target_uid {
        Some(uid) => uid,
        None => {
            let answer = t!("commands.deluser.errors.usage", locale = &lang_code).to_string();
            reply_html!(bot, msg, answer);
            return Ok(());
        }
    };

    let user = repos.users.get(target_uid).await?;
    if user.is_none() {
        let answer = t!(
            "commands.deluser.errors.not_found",
            locale = &lang_code,
            uid = target_uid.0
        )
        .to_string();
        reply_html!(bot, msg, answer);
        return Ok(());
    }
    let user_name = user.map(|u| u.name.value_ref().to_string()).unwrap_or("unknown".to_string());

    let affected = repos.users.delete_everything(target_uid).await?;
    let answer = if affected > 0 {
        t!(
            "commands.deluser.success",
            locale = &lang_code,
            name = user_name
        )
        .to_string()
    } else {
        t!(
            "commands.deluser.errors.not_found",
            locale = &lang_code,
            uid = target_uid.0
        )
        .to_string()
    };
    reply_html!(bot, msg, answer);
    Ok(())
}
