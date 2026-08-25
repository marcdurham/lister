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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn item_new_sets_defaults() {
        let item = Item::new("hello".into(), false);
        assert_eq!(item.text, "hello");
        assert!(!item.is_note);
        assert!(!item.done);
        assert_eq!(item.notes, None);
        assert!(item.deleted_at.is_none());
        // id is a fresh v4 uuid (non-zero)
        assert_ne!(item.id, Uuid::nil());
        // timestamps are set and within 2 seconds of now
        let now = Utc::now();
        let window = Duration::seconds(2);
        assert!(item.created_at >= now - window && item.created_at <= now + window);
        assert!(item.updated_at >= now - window && item.updated_at <= now + window);
    }

    #[test]
    fn item_new_sets_is_note_flag() {
        let note = Item::new("note".into(), true);
        assert!(note.is_note);
        assert!(!note.done);
        assert_eq!(note.notes, None);
    }

    #[test]
    fn membership_new_sets_defaults() {
        let parent = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let m = Membership::new(item_id, Some(parent), 3.14);
        assert_eq!(m.item_id, item_id);
        assert_eq!(m.parent_id, Some(parent));
        assert!((m.position - 3.14).abs() < f64::EPSILON);
        assert!(m.visible);
        assert!(m.deleted_at.is_none());
        assert_ne!(m.id, Uuid::nil());
    }

    #[test]
    fn membership_new_nil_parent() {
        let item_id = Uuid::new_v4();
        let m = Membership::new(item_id, None, 0.0);
        assert_eq!(m.parent_id, None);
        assert!((m.position - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn sync_request_defaults() {
        let req = SyncRequest::default();
        assert!(req.since.is_none());
        assert!(req.items.is_empty());
        assert!(req.memberships.is_empty());
    }
}
