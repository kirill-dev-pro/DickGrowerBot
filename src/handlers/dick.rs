use std::future::IntoFuture;

use anyhow::{anyhow, Context};
use chrono::{Datelike, Utc};
use futures::future::join;
use futures::TryFutureExt;
use rust_i18n::t;
use teloxide::macros::BotCommands;
use teloxide::requests::Requester;
use teloxide::types::{
    CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, Message, ParseMode, ReplyMarkup,
    User, UserId,
};
use teloxide::Bot;

use page::{InvalidPage, Page};
use rand::rngs::OsRng;
use rand::Rng;

use crate::domain::{LanguageCode, Username};
use crate::handlers::utils::{callbacks, page, Incrementor};
use crate::handlers::{reply_html, utils, HandlerResult};
use crate::repo::{ChatIdPartiality, UID};
use crate::{config, metrics, repo};

const TOMORROW_SQL_CODE: &str = "GD0E1";
const CALLBACK_PREFIX_TOP_PAGE: &str = "top:page:";

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum DickCommands {
    #[command(description = "grow")]
    Grow,
    #[command(description = "top")]
    Top,
    #[command(description = "gift")]
    Gift,
    #[command(description = "fire")]
    Fire,
}

pub async fn dick_cmd_handler(
    bot: Bot,
    msg: Message,
    cmd: DickCommands,
    repos: repo::Repositories,
    incr: Incrementor,
    config: config::AppConfig,
) -> HandlerResult {
    let from = msg
        .from
        .as_ref()
        .ok_or(anyhow!("unexpected absence of a FROM field"))?;
    let chat_id = msg.chat.id.into();
    let from_refs = FromRefs(from, &chat_id);
    match cmd {
        DickCommands::Grow => {
            metrics::CMD_GROW_COUNTER.chat.inc();
            let answer = grow_impl(&repos, incr, from_refs).await?;
            reply_html(bot, &msg, answer)
        }
        DickCommands::Top => {
            metrics::CMD_TOP_COUNTER.chat.inc();
            let top = top_impl(&repos, &config, from_refs, Page::first()).await?;
            let mut request = reply_html(bot, &msg, top.lines);
            if top.has_more_pages && config.features.top_unlimited {
                let keyboard = ReplyMarkup::InlineKeyboard(build_pagination_keyboard(
                    Page::first(),
                    top.has_more_pages,
                ));
                request.reply_markup.replace(keyboard);
            }
            request
        }
        DickCommands::Gift => {
            metrics::CMD_GIFT_COUNTER.chat.inc();
            let answer = gift_impl(&repos, &msg, from_refs, &config).await?;
            reply_html(bot, &msg, answer)
        }
        DickCommands::Fire => {
            metrics::CMD_FIRE_COUNTER.chat.inc();
            let answer = fire_impl(&repos, &msg, from_refs, &config).await?;
            reply_html(bot, &msg, answer)
        }
    }
    .await
    .context(format!("failed for {msg:?}"))?;
    Ok(())
}

pub struct FromRefs<'a>(pub &'a User, pub &'a ChatIdPartiality);

pub(crate) async fn grow_impl(
    repos: &repo::Repositories,
    incr: Incrementor,
    from_refs: FromRefs<'_>,
) -> anyhow::Result<String> {
    let (from, chat_id) = (from_refs.0, from_refs.1);
    let name = utils::get_full_name(from);
    let user = repos.users.create_or_update(from.id, &name).await?;
    let days_since_registration = (Utc::now() - user.created_at).num_days() as u32;
    let increment = incr
        .growth_increment(from.id, chat_id.kind(), days_since_registration)
        .await;
    let grow_result = repos
        .dicks
        .create_or_grow(from.id, chat_id, increment.total)
        .await;
    let lang_code = LanguageCode::from_user(from);

    let main_part = match grow_result {
        Ok(repo::GrowthResult {
            new_length,
            pos_in_top,
        }) => {
            let event_key = if increment.total.is_negative() {
                "shrunk"
            } else {
                "grown"
            };
            let event_template = format!("commands.grow.direction.{event_key}");
            let event = t!(&event_template, locale = &lang_code);
            let answer = t!(
                "commands.grow.result",
                locale = &lang_code,
                event = event,
                incr = increment.total.abs(),
                length = new_length
            );
            let perks_part = increment.perks_part_of_answer(&lang_code);
            if let Some(pos) = pos_in_top {
                let position = t!("commands.grow.position", locale = &lang_code, pos = pos);
                format!("{answer}\n{position}{perks_part}")
            } else {
                format!("{answer}{perks_part}")
            }
        }
        Err(e) => {
            let db_err = e.downcast::<sqlx::Error>()?;
            if let sqlx::Error::Database(e) = db_err {
                e.code()
                    .filter(|c| c == TOMORROW_SQL_CODE)
                    .map(|_| t!("commands.grow.tomorrow", locale = &lang_code).to_string())
                    .ok_or(anyhow!(e))?
            } else {
                Err(db_err)?
            }
        }
    };
    let time_left_part = utils::date::get_time_till_next_day_string(&lang_code);
    Ok(format!("{main_part}{time_left_part}"))
}

pub(crate) async fn gift_impl(
    repos: &repo::Repositories,
    msg: &Message,
    from_refs: FromRefs<'_>,
    config: &config::AppConfig,
) -> anyhow::Result<String> {
    let (from, chat_id) = (from_refs.0, from_refs.1);
    let lang_code = LanguageCode::from_user(from);

    let text = msg.text().unwrap_or("");
    let parts: Vec<&str> = text.split_whitespace().collect();

    let reply_msg = msg.reply_to_message();

    if parts.len() < 2 {
        return Ok(format!(
            "{}",
            t!("commands.gift.error.usage", locale = &lang_code)
        ));
    }

    let amount_str = parts[1];
    let amount: u16 = match amount_str.parse() {
        Ok(amt) if amt > 0 => amt,
        _ => {
            return Ok(format!(
                "{}",
                t!("commands.gift.error.invalid_amount", locale = &lang_code)
            ))
        }
    };

    log::debug!("from: {from:?}, chat_id: {chat_id:?}, amount: {amount}");

    let recipient = match reply_msg.as_ref().and_then(|msg| msg.from.as_ref()) {
        Some(user) => user,
        None => {
            log::warn!("no FROM field in the gift command handler");
            log::debug!("reply_msg: {reply_msg:?}");
            return Ok(format!(
                "{}",
                t!("commands.gift.error.usage", locale = &lang_code)
            ));
        }
    };

    if recipient.id == from.id {
        return Ok(format!(
            "{}",
            t!("commands.gift.error.same_person", locale = &lang_code)
        ));
    }

    if let Some(custom_name) = config.gift_restriction.restrictions.get(&recipient.id.0) {
        return Ok(format!(
            "{}",
            t!(
                "commands.gift.error.restricted_user",
                locale = &lang_code,
                name = custom_name
            )
        ));
    }

    let sender_length = match repos.dicks.fetch_length(from.id, &chat_id.kind()).await {
        Ok(length) => length,
        Err(_) => 0,
    };

    if sender_length < amount as i32 {
        return Ok(format!(
            "{}",
            t!(
                "commands.gift.error.not_enough",
                locale = &lang_code,
                current = sender_length,
                required = amount
            )
        ));
    }

    match repos
        .dicks
        .is_user_has_dick(recipient.id, &chat_id.kind())
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            log::debug!(
                "recipient {} doesn't have a dick in {}",
                recipient.id,
                chat_id
            );
            return Ok(format!(
                "{}",
                t!("commands.gift.error.wrong_person", locale = &lang_code)
            ));
        }
        Err(e) => {
            return Ok(format!(
                "{}",
                t!(
                    "commands.gift.error.unknown",
                    locale = &lang_code,
                    error = e.to_string()
                )
            ))
        }
    };

    let transfer_result = repos
        .dicks
        .move_length(chat_id, from.id, recipient.id, amount)
        .await;

    match transfer_result {
        Ok((sender_result, recipient_result)) => {
            let sender_name = utils::get_full_name(from);
            let recipient_name = utils::get_full_name(&recipient);

            Ok(format!(
                "{}",
                t!(
                    "commands.gift.result",
                    locale = &lang_code,
                    sender = sender_name.to_string(),
                    recipient = recipient_name.to_string(),
                    amount = amount,
                    sender_length = sender_result.new_length,
                    recipient_length = recipient_result.new_length
                )
            ))
        }
        Err(e) => Ok(format!(
            "{}",
            t!(
                "commands.gift.error.unknown",
                locale = &lang_code,
                error = e.to_string()
            )
        )),
    }
}

async fn get_random_chat_users(
    repos: &repo::Repositories,
    chat_id: &repo::ChatIdKind,
    sender_id: UserId,
    count: u16,
) -> anyhow::Result<Vec<repo::Dick>> {
    let total = repos
        .dicks
        .count_chat_members(chat_id, Some(sender_id))
        .await?;

    if total == 0 {
        return Ok(vec![]);
    }

    let count = count.min(total as u16);
    let mut indices = std::collections::HashSet::new();

    while indices.len() < count as usize {
        let idx = OsRng.gen_range(0..total);
        indices.insert(idx);
    }

    let mut users = Vec::with_capacity(count as usize);
    for idx in indices {
        if let Some(user) = repos
            .dicks
            .get_nth_user(chat_id, Some(sender_id), idx as u32)
            .await?
        {
            users.push(user);
        }
    }

    Ok(users)
}

pub(crate) async fn fire_impl(
    repos: &repo::Repositories,
    msg: &Message,
    from_refs: FromRefs<'_>,
    config: &config::AppConfig,
) -> anyhow::Result<String> {
    let (from, chat_id) = (from_refs.0, from_refs.1);
    let lang_code = LanguageCode::from_user(from);

    let text = msg.text().unwrap_or("");
    let parts: Vec<&str> = text.split_whitespace().collect();

    if parts.len() < 2 {
        return Ok(format!(
            "{}",
            t!("commands.fire.error.usage", locale = &lang_code)
        ));
    }

    let amount_str = parts[1];
    let total_amount: u16 = match amount_str.parse() {
        Ok(amt) if amt > 0 => amt,
        _ => {
            return Ok(format!(
                "{}",
                t!("commands.fire.error.invalid_amount", locale = &lang_code)
            ))
        }
    };

    let recipients_count = config.fire_recipients;
    let amount_per_person = total_amount / recipients_count;

    if amount_per_person == 0 {
        return Ok(format!(
            "{}",
            t!("commands.fire.error.too_small", locale = &lang_code)
        ));
    }

    log::debug!("from: {from:?}, chat_id: {chat_id:?}, total_amount: {total_amount}, recipients: {recipients_count}, per_person: {amount_per_person}");

    let sender_length = match repos.dicks.fetch_length(from.id, &chat_id.kind()).await {
        Ok(length) => length,
        Err(_) => 0,
    };

    if sender_length < total_amount as i32 {
        return Ok(format!(
            "{}",
            t!(
                "commands.fire.error.not_enough",
                locale = &lang_code,
                current = sender_length,
                required = total_amount
            )
        ));
    }

    let random_users =
        match get_random_chat_users(repos, &chat_id.kind(), from.id, recipients_count as u16).await
        {
            Ok(users) => users,
            Err(e) => {
                return Ok(format!(
                    "{}",
                    t!(
                        "commands.fire.error.unknown",
                        locale = &lang_code,
                        error = e.to_string()
                    )
                ))
            }
        };

    if random_users.len() < recipients_count as usize {
        return Ok(format!(
            "{}",
            t!(
                "commands.fire.error.not_enough_users",
                locale = &lang_code,
                found = random_users.len(),
                required = recipients_count
            )
        ));
    }

    let mut successful_transfers = Vec::new();
    let mut remaining_amount = total_amount;

    let mut futures = Vec::new();
    for user in &random_users {
        if remaining_amount < amount_per_person {
            break;
        }

        futures.push(repos.dicks.move_length(
            &chat_id,
            from.id,
            user.owner_uid.into(),
            amount_per_person,
        ));
        remaining_amount -= amount_per_person;
    }

    let results = futures::future::join_all(futures).await;

    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok((_, recipient_result)) => {
                successful_transfers.push((&random_users[i], recipient_result.new_length));
            }
            Err(e) => {
                log::warn!(
                    "Failed to transfer to user {:?}: {}",
                    random_users[i].owner_uid,
                    e
                );
                remaining_amount += amount_per_person;
            }
        }
    }

    if successful_transfers.is_empty() {
        return Ok(format!(
            "{}",
            t!("commands.fire.error.no_transfers", locale = &lang_code)
        ));
    }

    let final_sender_length = match repos.dicks.fetch_length(from.id, &chat_id.kind()).await {
        Ok(length) => length,
        Err(_) => sender_length - (total_amount - remaining_amount) as i32,
    };

    let sender_name = utils::get_full_name(from);
    let transferred_amount = total_amount - remaining_amount;

    let mut recipient_lines = Vec::new();
    for (user, new_length) in &successful_transfers {
        recipient_lines.push(t!(
            "commands.fire.line",
            locale = &lang_code,
            name = user.owner_name.clone(),
            length = new_length
        ));
    }
    let recipients_list = recipient_lines.join("\n");

    Ok(format!(
        "{}\n\n{}",
        t!(
            "commands.fire.result",
            locale = &lang_code,
            sender = sender_name.to_string(),
            total_amount = transferred_amount,
            recipients_count = successful_transfers.len(),
            amount_per_person = amount_per_person,
            sender_length = final_sender_length
        ),
        recipients_list
    ))
}

#[derive(Debug)]
pub(crate) struct Top {
    pub lines: String,
    pub(crate) has_more_pages: bool,
}

impl Top {
    fn from(s: impl ToString) -> Self {
        Self {
            lines: s.to_string(),
            has_more_pages: false,
        }
    }

    fn with_more_pages(s: impl ToString) -> Self {
        Self {
            lines: s.to_string(),
            has_more_pages: true,
        }
    }
}

pub(crate) async fn top_impl(
    repos: &repo::Repositories,
    config: &config::AppConfig,
    from_refs: FromRefs<'_>,
    page: Page,
) -> anyhow::Result<Top> {
    let (from, chat_id) = (from_refs.0, from_refs.1.kind());
    let lang_code = LanguageCode::from_user(from);
    let top_limit = config.top_limit as u32;
    let offset = page * top_limit;
    let query_limit = config.top_limit + 1; // fetch +1 row to know whether more rows exist or not
    let dicks = repos.dicks.get_top(&chat_id, offset, query_limit).await?;
    let has_more_pages = dicks.len() as u32 > top_limit;
    let lines = dicks
        .into_iter()
        .take(config.top_limit as usize)
        .enumerate()
        .map(|(i, d)| {
            let escaped_name = Username::new(d.owner_name).escaped();
            let name = if from.id == <UID as Into<UserId>>::into(d.owner_uid) {
                format!("<u>{escaped_name}</u>")
            } else {
                escaped_name
            };
            let can_grow = Utc::now().num_days_from_ce() > d.grown_at.num_days_from_ce();
            let pos = d.position.unwrap_or((i + 1) as i64);
            let mut line = t!(
                "commands.top.line",
                locale = &lang_code,
                n = pos,
                name = name,
                length = d.length
            )
            .to_string();
            if can_grow {
                line.push_str(" [+]")
            };
            line
        })
        .collect::<Vec<String>>();

    let res = if lines.is_empty() {
        Top::from(t!("commands.top.empty", locale = &lang_code))
    } else {
        let title = t!("commands.top.title", locale = &lang_code);
        let ending = t!("commands.top.ending", locale = &lang_code);
        let text = format!("{}\n\n{}\n\n{}", title, lines.join("\n"), ending);
        if has_more_pages {
            Top::with_more_pages(text)
        } else {
            Top::from(text)
        }
    };
    Ok(res)
}

pub fn page_callback_filter(query: CallbackQuery) -> bool {
    query
        .data
        .filter(|d| d.starts_with(CALLBACK_PREFIX_TOP_PAGE))
        .is_some()
}

pub async fn page_callback_handler(
    bot: Bot,
    q: CallbackQuery,
    config: config::AppConfig,
    repos: repo::Repositories,
) -> HandlerResult {
    let edit_msg_req_params = callbacks::get_params_for_message_edit(&q)?;
    if !config.features.top_unlimited {
        return answer_callback_feature_disabled(bot, &q, edit_msg_req_params).await;
    }

    let page = q
        .data
        .as_ref()
        .ok_or(InvalidPage::message("no data"))
        .and_then(|d| {
            d.strip_prefix(CALLBACK_PREFIX_TOP_PAGE)
                .map(str::to_owned)
                .ok_or(InvalidPage::for_value(d, "invalid prefix"))
        })
        .and_then(|r| r.parse().map_err(|e| InvalidPage::for_value(&r, e)))
        .map(Page)
        .map_err(|e| anyhow!(e))?;
    let chat_id_kind = edit_msg_req_params.clone().into();
    let chat_id_partiality = ChatIdPartiality::Specific(chat_id_kind);
    let from_refs = FromRefs(&q.from, &chat_id_partiality);
    let top = top_impl(&repos, &config, from_refs, page).await?;

    let keyboard = build_pagination_keyboard(page, top.has_more_pages);
    let (answer_callback_query_result, edit_message_result) = match &edit_msg_req_params {
        callbacks::EditMessageReqParamsKind::Chat(chat_id, message_id) => {
            let mut edit_message_text_req = bot.edit_message_text(*chat_id, *message_id, top.lines);
            edit_message_text_req.parse_mode.replace(ParseMode::Html);
            edit_message_text_req.reply_markup.replace(keyboard);
            join(
                bot.answer_callback_query(&q.id).into_future(),
                edit_message_text_req.into_future().map_ok(|_| ()),
            )
            .await
        }
        callbacks::EditMessageReqParamsKind::Inline {
            inline_message_id, ..
        } => {
            let mut edit_message_text_inline_req =
                bot.edit_message_text_inline(inline_message_id, top.lines);
            edit_message_text_inline_req
                .parse_mode
                .replace(ParseMode::Html);
            edit_message_text_inline_req.reply_markup.replace(keyboard);
            join(
                bot.answer_callback_query(&q.id).into_future(),
                edit_message_text_inline_req.into_future().map_ok(|_| ()),
            )
            .await
        }
    };
    answer_callback_query_result.context(format!("failed to answer a callback query {q:?}"))?;
    edit_message_result.context(format!(
        "failed to edit the message of {edit_msg_req_params:?}"
    ))?;
    Ok(())
}

pub fn build_pagination_keyboard(page: Page, has_more_pages: bool) -> InlineKeyboardMarkup {
    let mut buttons = Vec::new();
    if page > 0 {
        buttons.push(InlineKeyboardButton::callback(
            "⬅️",
            format!("{CALLBACK_PREFIX_TOP_PAGE}{}", page - 1),
        ))
    }
    if has_more_pages {
        buttons.push(InlineKeyboardButton::callback(
            "➡️",
            format!("{CALLBACK_PREFIX_TOP_PAGE}{}", page + 1),
        ))
    }
    InlineKeyboardMarkup::new(vec![buttons])
}

async fn answer_callback_feature_disabled(
    bot: Bot,
    q: &CallbackQuery,
    edit_msg_req_params: callbacks::EditMessageReqParamsKind,
) -> HandlerResult {
    let lang_code = LanguageCode::from_user(&q.from);

    let mut answer = bot.answer_callback_query(&q.id);
    answer.show_alert.replace(true);
    answer
        .text
        .replace(t!("errors.feature_disabled", locale = &lang_code).to_string());
    answer.await?;

    match edit_msg_req_params {
        callbacks::EditMessageReqParamsKind::Chat(chat_id, message_id) => bot
            .edit_message_reply_markup(chat_id, message_id)
            .await
            .map(|_| ())?,
        callbacks::EditMessageReqParamsKind::Inline {
            inline_message_id, ..
        } => bot
            .edit_message_reply_markup_inline(inline_message_id)
            .await
            .map(|_| ())?,
    };
    Ok(())
}
