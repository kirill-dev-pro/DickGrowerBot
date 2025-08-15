use crate::config::announcements::*;
use crate::config::env::*;
use crate::config::toggles::*;
use crate::domain::Ratio;
use crate::domain::SupportedLanguage::{EN, RU};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GiftRestrictionsConfig {
    pub restricted_users: Vec<RestrictedUser>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RestrictedUser {
    pub user_ids: Vec<u64>,
    pub custom_name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct GiftRestrictionConfig {
    /// Bind user_id -> custom_name
    pub restrictions: HashMap<u64, String>,
}

impl GiftRestrictionConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|content| toml::from_str::<GiftRestrictionsConfig>(&content).ok())
            .map(|config| {
                config
                    .restricted_users
                    .into_iter()
                    .flat_map(|user_group| {
                        user_group
                            .user_ids
                            .into_iter()
                            .map(move |user_id| (user_id, user_group.custom_name.clone()))
                    })
                    .collect()
            })
            .map(|restrictions| Self { restrictions })
            .unwrap_or_else(|| {
                log::warn!("Failed to load gift restrictions config");
                Self {
                    restrictions: HashMap::new(),
                }
            })
    }
}

#[derive(Clone)]
#[cfg_attr(test, derive(Default))]
pub struct AppConfig {
    pub features: FeatureToggles,
    pub top_limit: u16,
    pub loan_payout_ratio: f32,
    pub dod_rich_exclusion_ratio: Option<Ratio>,
    pub pvp_default_bet: u16,
    pub fire_recipients: u16,
    pub announcements: AnnouncementsConfig,
    pub command_toggles: CachedEnvToggles,
    pub gift_restriction: GiftRestrictionConfig,
}

#[derive(Clone)]
pub struct DatabaseConfig {
    pub url: Url,
    pub max_connections: u32,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let top_limit = get_env_value_or_default("TOP_LIMIT", 10);
        let loan_payout_ratio = get_env_value_or_default("LOAN_PAYOUT_COEF", 0.0);
        let dod_selection_mode = get_optional_env_value("DOD_SELECTION_MODE");
        let dod_rich_exclusion_ratio = get_optional_env_ratio("DOD_RICH_EXCLUSION_RATIO");
        let chats_merging = get_env_value_or_default("CHATS_MERGING_ENABLED", false);
        let top_unlimited = get_env_value_or_default("TOP_UNLIMITED_ENABLED", false);
        let multiple_loans = get_env_value_or_default("MULTIPLE_LOANS_ENABLED", false);
        let pvp_default_bet = get_env_value_or_default("PVP_DEFAULT_BET", 1);
        let fire_recipients = get_env_value_or_default("FIRE_RECIPIENTS", 5);
        let check_acceptor_length = get_env_value_or_default("PVP_CHECK_ACCEPTOR_LENGTH", false);
        let callback_locks = get_env_value_or_default("PVP_CALLBACK_LOCKS_ENABLED", true);
        let show_stats = get_env_value_or_default("PVP_STATS_SHOW", true);
        let show_stats_notice = get_env_value_or_default("PVP_STATS_SHOW_NOTICE", true);
        let announcement_max_shows = get_optional_env_value("ANNOUNCEMENT_MAX_SHOWS");
        let announcement_en = get_optional_env_value("ANNOUNCEMENT_EN");
        let announcement_ru = get_optional_env_value("ANNOUNCEMENT_RU");
        let gift_restriction_file: String = get_optional_env_value("GIFT_RESTRICTIONS_FILE");

        let gift_restriction = if gift_restriction_file.is_empty() {
            log::warn!("GIFT_RESTRICTIONS_FILE is empty, using default gift restrictions");
            GiftRestrictionConfig::default()
        } else {
            log::info!("Loading gift restrictions from file: {}", gift_restriction_file);
            GiftRestrictionConfig::load_from_file(gift_restriction_file)
        };

        Self {
            features: FeatureToggles {
                chats_merging,
                top_unlimited,
                multiple_loans,
                dod_selection_mode,
                pvp: BattlesFeatureToggles {
                    check_acceptor_length,
                    callback_locks,
                    show_stats,
                    show_stats_notice,
                },
            },
            top_limit,
            loan_payout_ratio,
            dod_rich_exclusion_ratio,
            pvp_default_bet,
            fire_recipients,
            announcements: AnnouncementsConfig {
                max_shows: announcement_max_shows,
                announcements: [(EN, announcement_en), (RU, announcement_ru)]
                    .map(|(lc, text)| (lc, Announcement::new(text)))
                    .into_iter()
                    .filter_map(|(lc, mb_ann)| mb_ann.map(|ann| (lc, ann)))
                    .collect(),
            },
            command_toggles: Default::default(),
            gift_restriction,
        }
    }
}

impl DatabaseConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            url: get_env_mandatory_value("DATABASE_URL")?,
            max_connections: get_env_value_or_default("DATABASE_MAX_CONNECTIONS", 10),
        })
    }
}
