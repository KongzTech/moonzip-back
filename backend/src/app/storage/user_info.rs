use crate::app::storage::misc::StoredPubkey;
use chrono::DateTime;
use serde::Serialize;
use services_common::TZ;

#[derive(Debug, Serialize, sqlx::FromRow, Clone)]
pub struct StoredUserInfo {
    pub wallet_address: StoredPubkey,
    pub username: String,
    pub display_name: Option<String>,
    pub image_url: Option<String>,
    pub nft_address: Option<StoredPubkey>,
    pub last_active: Option<i64>,
    pub created_at: Option<DateTime<TZ>>,
    pub updated_at: Option<DateTime<TZ>>,
}
