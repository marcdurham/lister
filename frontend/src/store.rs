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
const THEME_KEY: &str = "lister.theme";
const REMOTE_MODE_KEY: &str = "lister.remote_mode";
const LAST_SYNC_KEY: &str = "lister.last_sync";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemRecord {
    pub item: Item,
    pub dirty: bool,
    /// When this item was last confirmed to match the server (pushed to it, or pulled
    /// from it). `None` for an item that has never been uploaded - or one cached before
    /// this field existed, which is told apart from a never-uploaded item by `dirty`.
    #[serde(default)]
    pub pushed_at: Option<DateTime<Utc>>,
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

/// Light/dark appearance preference from the settings menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    /// Follow the OS/browser setting (the default).
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

    pub fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::System => "System",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
        }
    }
}

/// Stored as a bare string (not gloo's JSON encoding) because the inline script in
/// index.html reads the same key to apply the theme before the app has loaded.
pub fn load_theme() -> ThemeMode {
    match LocalStorage::raw().get_item(THEME_KEY).ok().flatten().as_deref() {
        Some("light") => ThemeMode::Light,
        Some("dark") => ThemeMode::Dark,
        _ => ThemeMode::System,
    }
}

pub fn save_theme(mode: ThemeMode) {
    let _ = LocalStorage::raw().set_item(THEME_KEY, mode.as_str());
    apply_theme();
}

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    /// Defined inline in index.html: re-resolves the saved preference into the
    /// `data-theme` attribute the stylesheet keys off.
    #[wasm_bindgen(js_namespace = window, js_name = listerApplyTheme)]
    fn apply_theme();
}

/// Insert/update a single item locally, marking it dirty so the next sync push picks it up.
pub fn put_item(item: Item) {
    let mut items = load_items();
    let pushed_at = items.get(&item.id).and_then(|r| r.pushed_at);
    items.insert(
        item.id,
        ItemRecord {
            item,
            dirty: true,
            pushed_at,
        },
    );
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
    let now = Utc::now();
    let mut items = load_items();
    for id in item_ids {
        if let Some(record) = items.get_mut(id) {
            record.dirty = false;
            record.pushed_at = Some(now);
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
    LocalStorage::delete(LAST_SYNC_KEY);
}

/// Wipe all locally cached rows and replace them with an imported set, marking every row
/// dirty so the next sync push mirrors the import to the server.
pub fn replace_all(items: Vec<Item>, memberships: Vec<Membership>) {
    clear_all();
    let items: HashMap<Uuid, ItemRecord> = items
        .into_iter()
        .map(|item| {
            (
                item.id,
                ItemRecord {
                    item,
                    dirty: true,
                    pushed_at: None,
                },
            )
        })
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
    let now = Utc::now();
    let mut local_items = load_items();
    for item in items {
        let is_locally_dirty = local_items.get(&item.id).is_some_and(|r| r.dirty);
        if !is_locally_dirty {
            local_items.insert(
                item.id,
                ItemRecord {
                    item,
                    dirty: false,
                    pushed_at: Some(now),
                },
            );
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

/// Replace one local item with the server's (newer) copy, discarding any unsynced local
/// edit to it, and merge in whichever of its memberships aren't themselves locally dirty.
pub fn overwrite_item_from_server(item: Item, memberships: Vec<Membership>) {
    let mut local_items = load_items();
    local_items.insert(
        item.id,
        ItemRecord {
            item,
            dirty: false,
            pushed_at: Some(Utc::now()),
        },
    );
    save_items(&local_items);
    apply_pulled(Vec::new(), memberships);
}

/// What a single item needs, for the remote-mode icon and the item editor's status.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemSync {
    /// The item, or one of its memberships, has a local change the server hasn't seen.
    pub dirty: bool,
    pub pushed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncStatus {
    /// Only in this browser - never uploaded.
    LocalOnly,
    /// In this browser and on the server, with local changes not yet uploaded.
    LocalChanges,
    /// In this browser and on the server, and identical.
    Synced,
}

impl ItemSync {
    pub fn status(&self) -> SyncStatus {
        match (self.dirty, self.pushed_at) {
            (true, None) => SyncStatus::LocalOnly,
            (true, Some(_)) => SyncStatus::LocalChanges,
            // Clean rows only ever come from a push or a pull, so they exist remotely
            // even when no timestamp was recorded (cached before `pushed_at` existed).
            (false, _) => SyncStatus::Synced,
        }
    }
}

/// Sync status of every cached item, read from local storage in one pass.
pub fn load_item_sync() -> HashMap<Uuid, ItemSync> {
    let dirty_membership_items: std::collections::HashSet<Uuid> = load_memberships()
        .into_values()
        .filter(|r| r.dirty)
        .map(|r| r.membership.item_id)
        .collect();
    load_items()
        .into_iter()
        .map(|(id, r)| {
            (
                id,
                ItemSync {
                    dirty: r.dirty || dirty_membership_items.contains(&id),
                    pushed_at: r.pushed_at,
                },
            )
        })
        .collect()
}

/// Every membership (live or deleted) that places `item_id` in a list.
pub fn memberships_of_item(item_id: Uuid) -> Vec<Membership> {
    load_memberships()
        .into_values()
        .map(|r| r.membership)
        .filter(|m| m.item_id == item_id)
        .collect()
}

/// Whether the item has a local change not yet uploaded.
pub fn is_item_dirty(item_id: Uuid) -> bool {
    load_items().get(&item_id).is_some_and(|r| r.dirty)
}

/// Items (and their memberships) that have to reach the server together with `item_id`:
/// the item itself, plus every ancestor list that's still locally dirty - a membership
/// can't be stored server-side before the list it points at exists there.
pub fn push_bundle(item_id: Uuid) -> (Vec<Item>, Vec<Membership>) {
    let records = load_items();
    let mut items = Vec::new();
    let mut memberships = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut queue = vec![item_id];
    while let Some(id) = queue.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(record) = records.get(&id) else {
            continue;
        };
        if id != item_id && !record.dirty {
            continue;
        }
        items.push(record.item.clone());
        for m in memberships_of_item(id) {
            if let Some(parent) = m.parent_id {
                queue.push(parent);
            }
            memberships.push(m);
        }
    }
    (items, memberships)
}

/// Remote mode shows each item's sync status and the remote controls.
pub fn load_remote_mode() -> bool {
    LocalStorage::get(REMOTE_MODE_KEY).unwrap_or(false)
}

pub fn save_remote_mode(on: bool) {
    let _ = LocalStorage::set(REMOTE_MODE_KEY, on);
}

/// When a full sync with the server last completed.
pub fn last_sync() -> Option<DateTime<Utc>> {
    LocalStorage::get(LAST_SYNC_KEY).ok()
}

pub fn set_last_sync(at: DateTime<Utc>) {
    let _ = LocalStorage::set(LAST_SYNC_KEY, at);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_sync_status_tells_local_only_from_changed_from_synced() {
        let at = Some(Utc::now());
        assert_eq!(ItemSync { dirty: true, pushed_at: None }.status(), SyncStatus::LocalOnly);
        assert_eq!(ItemSync { dirty: true, pushed_at: at }.status(), SyncStatus::LocalChanges);
        assert_eq!(ItemSync { dirty: false, pushed_at: at }.status(), SyncStatus::Synced);
        // Cached before `pushed_at` existed: clean, so it came from the server.
        assert_eq!(ItemSync { dirty: false, pushed_at: None }.status(), SyncStatus::Synced);
    }
}
