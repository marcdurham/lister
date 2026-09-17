use chrono::{DateTime, Utc};
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use shared::{Item, Membership};
use std::collections::HashMap;
use uuid::Uuid;

const ITEMS_KEY: &str = "lister.items";
const MEMBERSHIPS_KEY: &str = "lister.memberships";
const CURSOR_KEY: &str = "lister.cursor";
const HAS_LOGGED_IN_KEY: &str = "lister.has_logged_in";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemRecord {
    pub item: Item,
    pub dirty: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MembershipRecord {
    pub membership: Membership,
    pub dirty: bool,
}

pub fn load_items() -> HashMap<Uuid, ItemRecord> {
    LocalStorage::get(ITEMS_KEY).unwrap_or_default()
}

fn save_items(items: &HashMap<Uuid, ItemRecord>) {
    let _ = LocalStorage::set(ITEMS_KEY, items);
}

pub fn load_memberships() -> HashMap<Uuid, MembershipRecord> {
    LocalStorage::get(MEMBERSHIPS_KEY).unwrap_or_default()
}

fn save_memberships(memberships: &HashMap<Uuid, MembershipRecord>) {
    let _ = LocalStorage::set(MEMBERSHIPS_KEY, memberships);
}

pub fn get_cursor() -> Option<DateTime<Utc>> {
    LocalStorage::get(CURSOR_KEY).ok()
}

pub fn set_cursor(cursor: DateTime<Utc>) {
    let _ = LocalStorage::set(CURSOR_KEY, cursor);
}

/// Whether this browser has ever completed a login/register on this account before -
/// recorded alongside the cached items so a guest who's never signed in gets a distinct
/// "cached in browser" status instead of a misleading online/offline reading.
pub fn has_logged_in() -> bool {
    LocalStorage::get(HAS_LOGGED_IN_KEY).unwrap_or(false)
}

pub fn mark_logged_in() {
    let _ = LocalStorage::set(HAS_LOGGED_IN_KEY, true);
}

/// Insert/update a single item locally, marking it dirty so the next sync push picks it up.
pub fn put_item(item: Item) {
    let mut items = load_items();
    items.insert(item.id, ItemRecord { item, dirty: true });
    save_items(&items);
}

/// Insert/update a single membership locally, marking it dirty.
pub fn put_membership(membership: Membership) {
    let mut memberships = load_memberships();
    memberships.insert(
        membership.id,
        MembershipRecord {
            membership,
            dirty: true,
        },
    );
    save_memberships(&memberships);
}

pub fn dirty_items() -> Vec<Item> {
    load_items()
        .into_values()
        .filter(|r| r.dirty)
        .map(|r| r.item)
        .collect()
}

pub fn dirty_memberships() -> Vec<Membership> {
    load_memberships()
        .into_values()
        .filter(|r| r.dirty)
        .map(|r| r.membership)
        .collect()
}

/// Clear the dirty flag for rows that were just pushed successfully.
pub fn mark_synced(item_ids: &[Uuid], membership_ids: &[Uuid]) {
    let mut items = load_items();
    for id in item_ids {
        if let Some(record) = items.get_mut(id) {
            record.dirty = false;
        }
    }
    save_items(&items);

    let mut memberships = load_memberships();
    for id in membership_ids {
        if let Some(record) = memberships.get_mut(id) {
            record.dirty = false;
        }
    }
    save_memberships(&memberships);
}

/// Wipe all locally cached rows - used when switching accounts on a shared device so
/// one user's data never lingers where the next login could see it.
pub fn clear_all() {
    LocalStorage::delete(ITEMS_KEY);
    LocalStorage::delete(MEMBERSHIPS_KEY);
    LocalStorage::delete(CURSOR_KEY);
}

/// Wipe all locally cached rows and replace them with an imported set, marking every row
/// dirty so the next sync push mirrors the import to the server.
pub fn replace_all(items: Vec<Item>, memberships: Vec<Membership>) {
    clear_all();
    let items: HashMap<Uuid, ItemRecord> = items
        .into_iter()
        .map(|item| (item.id, ItemRecord { item, dirty: true }))
        .collect();
    save_items(&items);

    let memberships: HashMap<Uuid, MembershipRecord> = memberships
        .into_iter()
        .map(|membership| {
            (
                membership.id,
                MembershipRecord {
                    membership,
                    dirty: true,
                },
            )
        })
        .collect();
    save_memberships(&memberships);
}

pub fn unsynced_count() -> usize {
    load_items()
        .into_values()
        .filter(|r| r.dirty)
        .count()
        + load_memberships()
            .into_values()
            .filter(|r| r.dirty)
            .count()
}

/// Merge server-pulled rows into local storage. A row that's still locally dirty (an
/// unsynced local edit) is never clobbered by an incoming pull - it wins until it syncs.
pub fn apply_pulled(items: Vec<Item>, memberships: Vec<Membership>) {
    let mut local_items = load_items();
    for item in items {
        let is_locally_dirty = local_items.get(&item.id).is_some_and(|r| r.dirty);
        if !is_locally_dirty {
            local_items.insert(item.id, ItemRecord { item, dirty: false });
        }
    }
    save_items(&local_items);

    let mut local_memberships = load_memberships();
    for membership in memberships {
        let is_locally_dirty = local_memberships
            .get(&membership.id)
            .is_some_and(|r| r.dirty);
        if !is_locally_dirty {
            local_memberships.insert(
                membership.id,
                MembershipRecord {
                    membership,
                    dirty: false,
                },
            );
        }
    }
    save_memberships(&local_memberships);
}
