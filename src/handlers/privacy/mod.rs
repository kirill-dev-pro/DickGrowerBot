use crate::domain::LanguageCode;
use crate::domain::SupportedLanguage::{EN, RU};
use crate::handlers::{reply_html, HandlerResult};
use crate::{metrics, reply_html};
use teloxide::macros::BotCommands;
use teloxide::prelude::Message;
use teloxide::Bot;

static EN_POLICY: &str = include_str!("en.html");
static RU_POLICY: &str = include_str!("ru.html");

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum PrivacyCommands {
    #[command(description = "privacy")]
    Privacy,
}

pub async fn privacy_cmd_handler(bot: Bot, msg: Message) -> HandlerResult {
    metrics::CMD_PRIVACY_COUNTER.inc();
    let lang_code = LanguageCode::from_maybe_user(msg.from.as_ref());
    let policy = match lang_code.to_supported_language() {
        RU => RU_POLICY,
        EN => EN_POLICY,
    };
    reply_html!(bot, msg, policy);
    Ok(())
}
