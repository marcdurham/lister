use chrono::{DateTime, Months, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: Uuid,
    pub text: String,
    pub notes: Option<String>,
    pub is_note: bool,
    #[serde(default)]
    pub is_list: bool,
    pub done: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub due_at: Option<DateTime<Utc>>,
    /// Absolute instant the item becomes visible. Either set directly (fixed mode) or
    /// kept in sync with `due_at` via `show_before_due_amount`/`show_before_due_unit`.
    #[serde(default)]
    pub show_after: Option<DateTime<Utc>>,
    #[serde(default)]
    pub show_before_due_amount: Option<i64>,
    #[serde(default)]
    pub show_before_due_unit: Option<String>,
    /// Absolute instant the item becomes hidden. Either set directly (fixed mode) or
    /// kept in sync with `created_at` via `hide_after_created_amount`/`hide_after_created_unit`.
    #[serde(default)]
    pub hide_after: Option<DateTime<Utc>>,
    #[serde(default)]
    pub hide_after_created_amount: Option<i64>,
    #[serde(default)]
    pub hide_after_created_unit: Option<String>,
    /// When set, marking the item done instead un-marks it and advances `due_at` by this
    /// amount/unit - see `advance_recurrence`.
    #[serde(default)]
    pub recur_amount: Option<i64>,
    #[serde(default)]
    pub recur_unit: Option<String>,
    /// Per-list display setting: show every descendant level (lightly indented) instead
    /// of just direct children.
    #[serde(default)]
    pub show_nested_children: bool,
}

/// A time unit used by both the show-before-due and hide-after-created offsets, and by
/// recurrence intervals. Stored on `Item` as a plain string so the wire/DB format needs
/// no custom type.
pub const TIME_UNITS: &[&str] = &["minutes", "hours", "days", "weeks", "months"];

fn add_units(base: DateTime<Utc>, amount: i64, unit: &str) -> Option<DateTime<Utc>> {
    match unit {
        "minutes" => Some(base + chrono::Duration::minutes(amount)),
        "hours" => Some(base + chrono::Duration::hours(amount)),
        "days" => Some(base + chrono::Duration::days(amount)),
        "weeks" => Some(base + chrono::Duration::weeks(amount)),
        "months" => {
            if amount >= 0 {
                base.checked_add_months(Months::new(amount as u32))
            } else {
                base.checked_sub_months(Months::new((-amount) as u32))
            }
        }
        _ => None,
    }
}

impl Item {
    pub fn new(text: String, is_note: bool) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            text,
            notes: None,
            is_note,
            is_list: false,
            done: false,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            due_at: None,
            show_after: None,
            show_before_due_amount: None,
            show_before_due_unit: None,
            hide_after: None,
            hide_after_created_amount: None,
            hide_after_created_unit: None,
            recur_amount: None,
            recur_unit: None,
            show_nested_children: false,
        }
    }

    /// Whether the item should currently be shown, based on `show_after`/`hide_after`
    /// alone (independent of a membership's own `visible` flag).
    pub fn is_time_visible(&self, now: DateTime<Utc>) -> bool {
        if let Some(show_after) = self.show_after {
            if now < show_after {
                return false;
            }
        }
        if let Some(hide_after) = self.hide_after {
            if now >= hide_after {
                return false;
            }
        }
        true
    }

    /// Recomputes `show_after` from `due_at` when in "before due" mode (both
    /// `show_before_due_amount`/`unit` set) - a fixed `show_after` or "always visible"
    /// (neither set) are left untouched.
    pub fn recompute_show_after(&mut self) {
        if let (Some(amount), Some(unit), Some(due)) = (
            self.show_before_due_amount,
            self.show_before_due_unit.as_deref(),
            self.due_at,
        ) {
            self.show_after = add_units(due, -amount, unit);
        }
    }

    /// Recomputes `hide_after` from `created_at` when in "after created" mode (both
    /// `hide_after_created_amount`/`unit` set) - a fixed `hide_after` or "never" (neither
    /// set) are left untouched.
    pub fn recompute_hide_after(&mut self) {
        if let (Some(amount), Some(unit)) = (
            self.hide_after_created_amount,
            self.hide_after_created_unit.as_deref(),
        ) {
            self.hide_after = add_units(self.created_at, amount, unit);
        }
    }

    /// If this item recurs, un-marks it as done and advances `due_at` (from its previous
    /// due date, or from `now` if it had none) by the recurrence interval, recomputing
    /// `show_after` to match. Returns `false` (leaving the item untouched) if it doesn't
    /// recur or the interval is invalid.
    pub fn advance_recurrence(&mut self, now: DateTime<Utc>) -> bool {
        let (Some(amount), Some(unit)) = (self.recur_amount, self.recur_unit.clone()) else {
            return false;
        };
        let base = self.due_at.unwrap_or(now);
        let Some(next) = add_units(base, amount, &unit) else {
            return false;
        };
        self.due_at = Some(next);
        self.done = false;
        self.recompute_show_after();
        true
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

    #[test]
    fn is_time_visible_respects_show_and_hide() {
        let now = Utc::now();
        let mut item = Item::new("t".into(), false);
        assert!(item.is_time_visible(now));

        item.show_after = Some(now + Duration::minutes(5));
        assert!(!item.is_time_visible(now));
        assert!(item.is_time_visible(now + Duration::minutes(6)));

        item.show_after = None;
        item.hide_after = Some(now + Duration::minutes(5));
        assert!(item.is_time_visible(now));
        assert!(!item.is_time_visible(now + Duration::minutes(6)));
    }

    #[test]
    fn recompute_show_after_derives_from_due_date() {
        let mut item = Item::new("t".into(), false);
        let due = Utc::now() + Duration::days(3);
        item.due_at = Some(due);
        item.show_before_due_amount = Some(2);
        item.show_before_due_unit = Some("days".into());
        item.recompute_show_after();
        assert_eq!(item.show_after, Some(due - Duration::days(2)));

        // Moving the due date recomputes show_after to match.
        let new_due = due + Duration::days(1);
        item.due_at = Some(new_due);
        item.recompute_show_after();
        assert_eq!(item.show_after, Some(new_due - Duration::days(2)));
    }

    #[test]
    fn recompute_show_after_leaves_fixed_mode_untouched() {
        let mut item = Item::new("t".into(), false);
        let fixed = Utc::now() + Duration::hours(1);
        item.show_after = Some(fixed);
        item.due_at = Some(Utc::now() + Duration::days(1));
        item.recompute_show_after();
        assert_eq!(item.show_after, Some(fixed));
    }

    #[test]
    fn recompute_hide_after_derives_from_created_at() {
        let mut item = Item::new("t".into(), false);
        item.hide_after_created_amount = Some(30);
        item.hide_after_created_unit = Some("minutes".into());
        item.recompute_hide_after();
        assert_eq!(item.hide_after, Some(item.created_at + Duration::minutes(30)));
    }

    #[test]
    fn advance_recurrence_unmarks_done_and_moves_due_date() {
        let mut item = Item::new("t".into(), false);
        let due = Utc::now() + Duration::days(1);
        item.due_at = Some(due);
        item.done = true;
        item.recur_amount = Some(1);
        item.recur_unit = Some("weeks".into());

        assert!(item.advance_recurrence(Utc::now()));
        assert!(!item.done);
        assert_eq!(item.due_at, Some(due + Duration::weeks(1)));
    }

    #[test]
    fn advance_recurrence_uses_now_when_never_had_a_due_date() {
        let mut item = Item::new("t".into(), false);
        item.recur_amount = Some(2);
        item.recur_unit = Some("days".into());
        let now = Utc::now();

        assert!(item.advance_recurrence(now));
        assert_eq!(item.due_at, Some(now + Duration::days(2)));
    }

    #[test]
    fn advance_recurrence_noop_without_recurrence_set() {
        let mut item = Item::new("t".into(), false);
        assert!(!item.advance_recurrence(Utc::now()));
        assert_eq!(item.due_at, None);
    }
}
