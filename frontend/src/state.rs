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

    pub fn has_children(&self, item_id: Uuid) -> bool {
        self.memberships
            .values()
            .any(|m| m.parent_id == Some(item_id) && m.deleted_at.is_none())
    }

    pub fn top_level_lists(&self) -> Vec<Item> {
        self.children(None, false)
            .into_iter()
            .map(|(item, _)| item)
            .collect()
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
    Reorder {
        membership_id: Uuid,
        swap_with: Uuid,
    },
    AddToList {
        item_id: Uuid,
        parent: Option<Uuid>,
    },
    RemoveFromList(Uuid),
    SetOnline(bool),
    SetSyncing(bool),
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
                let next_pos = self
                    .children(parent, true)
                    .last()
                    .map(|(_, m)| m.position + 1.0)
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
            Action::Reorder {
                membership_id,
                swap_with,
            } => {
                let mut next = (*self).clone();
                let a_pos = next.memberships.get(&membership_id).map(|m| m.position);
                let b_pos = next.memberships.get(&swap_with).map(|m| m.position);
                if let (Some(a_pos), Some(b_pos)) = (a_pos, b_pos) {
                    if let Some(m) = next.memberships.get_mut(&membership_id) {
                        m.position = b_pos;
                        m.updated_at = Utc::now();
                        store::put_membership(m.clone());
                    }
                    if let Some(m) = next.memberships.get_mut(&swap_with) {
                        m.position = a_pos;
                        m.updated_at = Utc::now();
                        store::put_membership(m.clone());
                    }
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
            Action::RemoveFromList(membership_id) => {
                let mut next = (*self).clone();
                if let Some(m) = next.memberships.get_mut(&membership_id) {
                    m.deleted_at = Some(Utc::now());
                    m.updated_at = Utc::now();
                    store::put_membership(m.clone());
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
        }
    }
}
