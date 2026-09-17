use crate::model::{Item, Membership};
use crate::store;
use chrono::Utc;
use std::collections::HashMap;
use std::rc::Rc;
use uuid::Uuid;
use yew::prelude::*;

#[derive(Clone, PartialEq)]
pub struct AppState {
    pub items: HashMap<Uuid, Item>,
    pub memberships: HashMap<Uuid, Membership>,
    pub online: bool,
    pub syncing: bool,
}

impl AppState {
    pub fn load() -> Self {
        let items = store::load_items()
            .into_values()
            .map(|r| (r.item.id, r.item))
            .collect();
        let memberships = store::load_memberships()
            .into_values()
            .map(|r| (r.membership.id, r.membership))
            .collect();
        Self {
            items,
            memberships,
            online: crate::sync::is_online(),
            syncing: false,
        }
    }

    /// Children of `parent` (or top-level lists when `parent` is None), sorted by position.
    pub fn children(&self, parent: Option<Uuid>, include_hidden: bool) -> Vec<(Item, Membership)> {
        let mut rows: Vec<(Item, Membership)> = self
            .memberships
            .values()
            .filter(|m| m.parent_id == parent && m.deleted_at.is_none())
            .filter(|m| include_hidden || m.visible)
            .filter_map(|m| {
                let item = self.items.get(&m.item_id)?;
                if item.deleted_at.is_some() {
                    return None;
                }
                Some((item.clone(), m.clone()))
            })
            .collect();
        rows.sort_by(|a, b| a.1.position.partial_cmp(&b.1.position).unwrap());
        rows
    }

    /// Count of live (non-deleted) direct children, regardless of visibility.
    pub fn direct_child_count(&self, item_id: Uuid) -> usize {
        self.memberships
            .values()
            .filter(|m| m.parent_id == Some(item_id) && m.deleted_at.is_none())
            .filter(|m| {
                self.items
                    .get(&m.item_id)
                    .is_some_and(|i| i.deleted_at.is_none())
            })
            .count()
    }

    /// How many active (non-deleted) memberships an item currently has.
    fn active_membership_count(&self, item_id: Uuid) -> usize {
        self.memberships
            .values()
            .filter(|m| m.item_id == item_id && m.deleted_at.is_none())
            .count()
    }

    fn direct_children_ids(&self, item_id: Uuid) -> Vec<Uuid> {
        self.memberships
            .values()
            .filter(|m| m.parent_id == Some(item_id) && m.deleted_at.is_none())
            .map(|m| m.item_id)
            .collect()
    }

    /// Descendants that would be swept up if `item_id` is removed: children (recursively)
    /// whose *only* parent is somewhere in this branch, i.e. not shared with another list.
    pub fn only_child_descendant_ids(&self, item_id: Uuid) -> Vec<Uuid> {
        let mut result = Vec::new();
        let mut stack: Vec<Uuid> = self
            .direct_children_ids(item_id)
            .into_iter()
            .filter(|cid| self.active_membership_count(*cid) == 1)
            .collect();
        while let Some(id) = stack.pop() {
            result.push(id);
            let more: Vec<Uuid> = self
                .direct_children_ids(id)
                .into_iter()
                .filter(|cid| self.active_membership_count(*cid) == 1)
                .collect();
            stack.extend(more);
        }
        result
    }

    /// All descendants of `item_id` (regardless of whether they're shared with another
    /// list) - used to keep the list manager from letting an item become its own ancestor.
    pub fn descendant_ids(&self, item_id: Uuid) -> std::collections::HashSet<Uuid> {
        let mut result = std::collections::HashSet::new();
        let mut stack = self.direct_children_ids(item_id);
        while let Some(id) = stack.pop() {
            if result.insert(id) {
                stack.extend(self.direct_children_ids(id));
            }
        }
        result
    }

    /// Items currently in the trash, most-recently-removed first.
    pub fn trashed_items(&self) -> Vec<Item> {
        let mut items: Vec<Item> = self
            .items
            .values()
            .filter(|i| i.deleted_at.is_some())
            .cloned()
            .collect();
        items.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
        items
    }
}

pub enum Action {
    Reload,
    AddItem {
        text: String,
        is_note: bool,
        parent: Option<Uuid>,
    },
    ToggleDone(Uuid),
    UpdateNotes {
        item_id: Uuid,
        notes: String,
    },
    ToggleVisible(Uuid),
    RemoveMembership(Uuid),
    /// Move `membership_id` so it sits immediately before `before_id` in the same list.
    MoveBefore {
        membership_id: Uuid,
        before_id: Uuid,
    },
    AddToList {
        item_id: Uuid,
        parent: Option<Uuid>,
    },
    RemoveItem {
        item_id: Uuid,
        remove_children: bool,
    },
    RestoreItem(Uuid),
    SetItemKind {
        item_id: Uuid,
        is_note: bool,
        is_list: bool,
    },
    UpdateText {
        item_id: Uuid,
        text: String,
    },
    SetOnline(bool),
    SetSyncing(bool),
    ImportGoogleTasks(Vec<crate::google::ImportedTaskList>),
    ImportJson {
        items: Vec<Item>,
        memberships: Vec<Membership>,
    },
}

impl Reducible for AppState {
    type Action = Action;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            Action::Reload => Rc::new(AppState::load()),
            Action::AddItem {
                text,
                is_note,
                parent,
            } => {
                let item = Item::new(text, is_note);
                // New items go to the top of the list, not the bottom.
                let next_pos = self
                    .children(parent, true)
                    .first()
                    .map(|(_, m)| m.position - 1.0)
                    .unwrap_or(1.0);
                let membership = Membership::new(item.id, parent, next_pos);
                store::put_item(item.clone());
                store::put_membership(membership.clone());

                let mut next = (*self).clone();
                next.items.insert(item.id, item);
                next.memberships.insert(membership.id, membership);
                Rc::new(next)
            }
            Action::ToggleDone(item_id) => {
                let mut next = (*self).clone();
                if let Some(item) = next.items.get_mut(&item_id) {
                    item.done = !item.done;
                    item.updated_at = Utc::now();
                    store::put_item(item.clone());
                }
                Rc::new(next)
            }
            Action::UpdateNotes { item_id, notes } => {
                let mut next = (*self).clone();
                if let Some(item) = next.items.get_mut(&item_id) {
                    item.notes = if notes.trim().is_empty() {
                        None
                    } else {
                        Some(notes)
                    };
                    item.updated_at = Utc::now();
                    store::put_item(item.clone());
                }
                Rc::new(next)
            }
            Action::ToggleVisible(membership_id) => {
                let mut next = (*self).clone();
                if let Some(m) = next.memberships.get_mut(&membership_id) {
                    m.visible = !m.visible;
                    m.updated_at = Utc::now();
                    store::put_membership(m.clone());
                }
                Rc::new(next)
            }
            Action::RemoveMembership(membership_id) => {
                let mut next = (*self).clone();
                if let Some(m) = next.memberships.get_mut(&membership_id) {
                    let now = Utc::now();
                    m.deleted_at = Some(now);
                    m.updated_at = now;
                    store::put_membership(m.clone());
                }
                Rc::new(next)
            }
            Action::MoveBefore {
                membership_id,
                before_id,
            } => {
                let mut next = (*self).clone();
                if membership_id == before_id {
                    return Rc::new(next);
                }
                let Some(parent) = next.memberships.get(&before_id).map(|m| m.parent_id) else {
                    return Rc::new(next);
                };
                let mut siblings: Vec<&Membership> = next
                    .memberships
                    .values()
                    .filter(|m| {
                        m.parent_id == parent && m.deleted_at.is_none() && m.id != membership_id
                    })
                    .collect();
                siblings.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap());
                let Some(before_idx) = siblings.iter().position(|m| m.id == before_id) else {
                    return Rc::new(next);
                };
                let before_pos = siblings[before_idx].position;
                let new_position = match before_idx.checked_sub(1) {
                    Some(prev_idx) => (siblings[prev_idx].position + before_pos) / 2.0,
                    None => before_pos - 1.0,
                };
                if let Some(m) = next.memberships.get_mut(&membership_id) {
                    m.position = new_position;
                    m.updated_at = Utc::now();
                    store::put_membership(m.clone());
                }
                Rc::new(next)
            }
            Action::AddToList { item_id, parent } => {
                let already_there = self
                    .memberships
                    .values()
                    .any(|m| m.item_id == item_id && m.parent_id == parent && m.deleted_at.is_none());
                if already_there {
                    return self;
                }
                let next_pos = self
                    .children(parent, true)
                    .last()
                    .map(|(_, m)| m.position + 1.0)
                    .unwrap_or(1.0);
                let membership = Membership::new(item_id, parent, next_pos);
                store::put_membership(membership.clone());

                let mut next = (*self).clone();
                next.memberships.insert(membership.id, membership);
                Rc::new(next)
            }
            Action::RemoveItem {
                item_id,
                remove_children,
            } => {
                let now = Utc::now();
                let mut next = (*self).clone();
                let mut to_remove = vec![item_id];
                if remove_children {
                    to_remove.extend(self.only_child_descendant_ids(item_id));
                }
                for id in &to_remove {
                    if let Some(item) = next.items.get_mut(id) {
                        item.deleted_at = Some(now);
                        item.updated_at = now;
                        store::put_item(item.clone());
                    }
                    let membership_ids: Vec<Uuid> = next
                        .memberships
                        .values()
                        .filter(|m| m.item_id == *id && m.deleted_at.is_none())
                        .map(|m| m.id)
                        .collect();
                    for mid in membership_ids {
                        if let Some(m) = next.memberships.get_mut(&mid) {
                            m.deleted_at = Some(now);
                            m.updated_at = now;
                            store::put_membership(m.clone());
                        }
                    }
                }
                Rc::new(next)
            }
            Action::RestoreItem(item_id) => {
                let mut next = (*self).clone();
                let trashed_at = next.items.get(&item_id).and_then(|i| i.deleted_at);
                if let Some(item) = next.items.get_mut(&item_id) {
                    item.deleted_at = None;
                    item.updated_at = Utc::now();
                    store::put_item(item.clone());
                }
                if let Some(trashed_at) = trashed_at {
                    let ids: Vec<Uuid> = next
                        .memberships
                        .values()
                        .filter(|m| m.item_id == item_id && m.deleted_at == Some(trashed_at))
                        .map(|m| m.id)
                        .collect();
                    for mid in ids {
                        if let Some(m) = next.memberships.get_mut(&mid) {
                            m.deleted_at = None;
                            m.updated_at = Utc::now();
                            store::put_membership(m.clone());
                        }
                    }
                }
                Rc::new(next)
            }
            Action::SetItemKind {
                item_id,
                is_note,
                is_list,
            } => {
                let mut next = (*self).clone();
                if let Some(item) = next.items.get_mut(&item_id) {
                    item.is_note = is_note;
                    item.is_list = is_list;
                    if is_note || is_list {
                        item.done = false;
                    }
                    item.updated_at = Utc::now();
                    store::put_item(item.clone());
                }
                Rc::new(next)
            }
            Action::UpdateText { item_id, text } => {
                let mut next = (*self).clone();
                if let Some(item) = next.items.get_mut(&item_id) {
                    item.text = text;
                    item.updated_at = Utc::now();
                    store::put_item(item.clone());
                }
                Rc::new(next)
            }
            Action::SetOnline(online) => {
                let mut next = (*self).clone();
                next.online = online;
                Rc::new(next)
            }
            Action::SetSyncing(syncing) => {
                let mut next = (*self).clone();
                next.syncing = syncing;
                Rc::new(next)
            }
            Action::ImportGoogleTasks(lists) => {
                let mut next = (*self).clone();
                for list in lists {
                    let list_item = Item::new(list.title, false);
                    let list_pos = next
                        .children(None, true)
                        .last()
                        .map(|(_, m)| m.position + 1.0)
                        .unwrap_or(1.0);
                    let list_membership = Membership::new(list_item.id, None, list_pos);
                    store::put_item(list_item.clone());
                    store::put_membership(list_membership.clone());
                    next.items.insert(list_item.id, list_item.clone());
                    next.memberships
                        .insert(list_membership.id, list_membership);

                    for (pos, task) in list.tasks.into_iter().enumerate() {
                        let mut item = Item::new(task.title, false);
                        item.notes = task.notes;
                        item.done = task.done;
                        let membership =
                            Membership::new(item.id, Some(list_item.id), (pos + 1) as f64);
                        store::put_item(item.clone());
                        store::put_membership(membership.clone());
                        next.items.insert(item.id, item);
                        next.memberships.insert(membership.id, membership);
                    }
                }
                Rc::new(next)
            }
            Action::ImportJson { items, memberships } => {
                store::replace_all(items.clone(), memberships.clone());
                Rc::new(AppState {
                    items: items.into_iter().map(|i| (i.id, i)).collect(),
                    memberships: memberships.into_iter().map(|m| (m.id, m)).collect(),
                    online: self.online,
                    syncing: self.syncing,
                })
            }
        }
    }
}
