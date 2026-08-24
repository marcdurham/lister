use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: Uuid,
    pub text: String,
    pub notes: Option<String>,
    pub is_note: bool,
    pub done: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Item {
    pub fn new(text: String, is_note: bool) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            text,
            notes: None,
            is_note,
            done: false,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Membership {
    pub id: Uuid,
    pub item_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub position: f64,
    pub visible: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Membership {
    pub fn new(item_id: Uuid, parent_id: Option<Uuid>, position: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            item_id,
            parent_id,
            position,
            visible: true,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SyncRequest {
    pub since: Option<DateTime<Utc>>,
    pub items: Vec<Item>,
    pub memberships: Vec<Membership>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncResponse {
    pub items: Vec<Item>,
    pub memberships: Vec<Membership>,
    pub cursor: DateTime<Utc>,
}
