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

    #[test]
    fn item_serde_roundtrip() {
        let original = Item::new("round trip".into(), true);
        let json = serde_json::to_string(&original).unwrap();
        let decoded: Item = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, original.id);
        assert_eq!(decoded.text, "round trip");
        assert!(decoded.is_note);
        assert!(!decoded.done);
        assert_eq!(decoded.deleted_at, None);
    }

    #[test]
    fn membership_serde_roundtrip() {
        let parent = Uuid::new_v4();
        let original = Membership::new(Uuid::new_v4(), Some(parent), 2.5);
        let json = serde_json::to_string(&original).unwrap();
        let decoded: Membership = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, original.id);
        assert_eq!(decoded.item_id, original.item_id);
        assert_eq!(decoded.parent_id, Some(parent));
        assert!((decoded.position - 2.5).abs() < f64::EPSILON);
        assert!(decoded.visible);
    }

    #[test]
    fn item_timestamps_are_unique_across_instances() {
        // Two items created back-to-back should have distinct ids.
        let a = Item::new("a".into(), false);
        let b = Item::new("b".into(), false);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn sync_response_roundtrip() {
        let item = Item::new("r".into(), false);
        let resp = SyncResponse {
            items: vec![item.clone()],
            memberships: vec![],
            cursor: Utc::now(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: SyncResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.items.len(), 1);
        assert_eq!(decoded.items[0].id, item.id);
    }
}
