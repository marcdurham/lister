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
    /// `include_hidden` bypasses both a membership's own `visible` flag and an item's
    /// due-based show/hide window (the same "Show hidden items" toggle covers both).
    pub fn children(&self, parent: Option<Uuid>, include_hidden: bool) -> Vec<(Item, Membership)> {
        let now = Utc::now();
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
                if !include_hidden && !item.is_time_visible(now) {
                    return None;
                }
                Some((item.clone(), m.clone()))
            })
            .collect();
        rows.sort_by(|a, b| a.1.position.partial_cmp(&b.1.position).unwrap());
        rows
    }

    /// Like `children`, but also includes every deeper descendant level, each tagged with
    /// its depth relative to `parent` (1 for a direct child, 2 for a grandchild, etc.) -
    /// used when a list has "show nested children" enabled.
    pub fn children_recursive(
        &self,
        parent: Option<Uuid>,
        include_hidden: bool,
    ) -> Vec<(Item, Membership, usize)> {
        let mut out = Vec::new();
        self.push_children_recursive(parent, include_hidden, 1, &mut out);
        out
    }

    fn push_children_recursive(
        &self,
        parent: Option<Uuid>,
        include_hidden: bool,
        depth: usize,
        out: &mut Vec<(Item, Membership, usize)>,
    ) {
        for (item, membership) in self.children(parent, include_hidden) {
            let item_id = item.id;
            out.push((item.clone(), membership, depth));
            self.push_children_recursive(Some(item_id), include_hidden, depth + 1, out);
        }
    }

    /// Every live item anywhere in the hierarchy whose text contains `query` (already
    /// lowercased and non-empty), each paired with one of its live memberships - used so
    /// a search from the root list finds items nested inside any list, not just top-level
    /// ones. Depth is always 1, marking these as non-drag-and-drop rows the same way a
    /// nested-children row is: they aren't siblings of one another in any real list, so
    /// reordering or dropping onto them wouldn't mean anything.
    pub fn search_all(&self, query: &str, include_hidden: bool) -> Vec<(Item, Membership, usize)> {
        let now = Utc::now();
        let mut rows: Vec<(Item, Membership, usize)> = self
            .items
            .values()
            .filter(|item| item.deleted_at.is_none())
            .filter(|item| item.text.to_lowercase().contains(query))
            .filter(|item| include_hidden || item.is_time_visible(now))
            .filter_map(|item| {
                let membership = self.memberships.values().find(|m| {
                    m.item_id == item.id
                        && m.deleted_at.is_none()
                        && (include_hidden || m.visible)
                })?;
                Some((item.clone(), membership.clone(), 1))
            })
            .collect();
        rows.sort_by(|a, b| a.0.text.to_lowercase().cmp(&b.0.text.to_lowercase()));
        rows
    }

    /// Any live membership id for this item - used to open the item editor for an item
    /// reached by its id alone (e.g. the current list's own title), where a specific
    /// membership isn't already in hand.
    pub fn any_membership_id(&self, item_id: Uuid) -> Option<Uuid> {
        self.memberships
            .values()
            .find(|m| m.item_id == item_id && m.deleted_at.is_none())
            .map(|m| m.id)
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

    /// Render `root` (or the whole top level, when `root` is `None`) as a markdown list of
    /// just the item titles, with children indented under their parents.
    pub fn copy_as_markdown(&self, root: Option<Uuid>) -> String {
        let mut out = String::new();
        self.write_markdown_children(root, 0, &mut out);
        out
    }

    fn write_markdown_children(&self, parent: Option<Uuid>, depth: usize, out: &mut String) {
        for (item, _) in self.children(parent, true) {
            out.push_str(&"  ".repeat(depth));
            out.push_str("- ");
            // Mirrors the row's own list detection: an item with children renders (and
            // exports) as a list even if `is_list` was never explicitly set on it.
            let is_task = !item.is_note && !item.is_list && self.direct_child_count(item.id) == 0;
            if is_task {
                out.push_str(if item.done { "DONE: " } else { "TODO: " });
            }
            out.push_str(&item.text);
            out.push('\n');
            self.write_markdown_children(Some(item.id), depth + 1, out);
        }
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

/// Pending edits to an item's due date, show/hide windows, and recurrence, applied in
/// one shot from the item editor. Each "mode" is represented by which optional fields are
/// set: for show, either both `show_before_due_amount`/`unit` (before-due mode) or just
/// `show_at_fixed` (fixed mode) or neither (always visible); for hide, the equivalent
/// with `hide_after_created_amount`/`unit` / `hide_at_fixed` / neither.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScheduleUpdate {
    pub due_at: Option<chrono::DateTime<Utc>>,
    pub show_before_due_amount: Option<i64>,
    pub show_before_due_unit: Option<String>,
    pub show_at_fixed: Option<chrono::DateTime<Utc>>,
    pub hide_after_created_amount: Option<i64>,
    pub hide_after_created_unit: Option<String>,
    pub hide_at_fixed: Option<chrono::DateTime<Utc>>,
    pub recur_amount: Option<i64>,
    pub recur_unit: Option<String>,
}

pub enum Action {
    Reload,
    /// No-op state change used purely to force a re-render, so items whose due-based
    /// show/hide window has just opened or closed appear/disappear on their own instead
    /// of waiting for the next real edit or a page refresh - see `main::visibility_tick_loop`.
    Tick,
    AddItem {
        text: String,
        is_note: bool,
        is_list: bool,
        parent: Option<Uuid>,
    },
    ToggleDone(Uuid),
    UpdateNotes {
        item_id: Uuid,
        notes: String,
    },
    ToggleVisible(Uuid),
    UpdateSchedule {
        item_id: Uuid,
        update: ScheduleUpdate,
    },
    SetShowNestedChildren {
        item_id: Uuid,
        value: bool,
    },
    RemoveMembership(Uuid),
    /// Move `membership_id` so it sits immediately before `before_id` in the same list.
    MoveBefore {
        membership_id: Uuid,
        before_id: Uuid,
    },
    /// Move `membership_id` to the end of its current list.
    MoveToEnd {
        membership_id: Uuid,
    },
    /// Move `membership_id` to become a child of `target_item_id`, turning the target
    /// into a list if it isn't already one.
    MoveInto {
        membership_id: Uuid,
        target_item_id: Uuid,
    },
    /// Move `membership_id` out of every ancestor, making it a top-level list.
    MoveToRoot {
        membership_id: Uuid,
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
    /// Adds a parsed markdown import's items/memberships, reparenting whichever ones
    /// have no parent (normally just the file's wrapper item) onto `parent` (or leaving
    /// them at the top level when `parent` is `None`), appended after whatever children
    /// it already has.
    ImportMarkdown {
        items: Vec<Item>,
        memberships: Vec<Membership>,
        parent: Option<Uuid>,
    },
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
            Action::Tick => Rc::new((*self).clone()),
            Action::AddItem {
                text,
                is_note,
                is_list,
                parent,
            } => {
                let mut item = Item::new(text, is_note);
                item.is_list = is_list;
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
                    let now = Utc::now();
                    // A recurring task never stays done: marking it done instead advances
                    // its due date and pops back up as a fresh, unmarked instance.
                    if !item.done && item.recur_amount.is_some() {
                        item.advance_recurrence(now);
                    } else {
                        item.done = !item.done;
                    }
                    item.updated_at = now;
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
            Action::UpdateSchedule { item_id, update } => {
                let mut next = (*self).clone();
                if let Some(item) = next.items.get_mut(&item_id) {
                    item.due_at = update.due_at;
                    item.show_before_due_amount = update.show_before_due_amount;
                    item.show_before_due_unit = update.show_before_due_unit;
                    item.show_after = update.show_at_fixed;
                    item.hide_after_created_amount = update.hide_after_created_amount;
                    item.hide_after_created_unit = update.hide_after_created_unit;
                    item.hide_after = update.hide_at_fixed;
                    item.recur_amount = update.recur_amount;
                    item.recur_unit = update.recur_unit;
                    item.recompute_show_after();
                    item.recompute_hide_after();
                    item.updated_at = Utc::now();
                    store::put_item(item.clone());
                }
                Rc::new(next)
            }
            Action::SetShowNestedChildren { item_id, value } => {
                let mut next = (*self).clone();
                if let Some(item) = next.items.get_mut(&item_id) {
                    item.show_nested_children = value;
                    item.updated_at = Utc::now();
                    store::put_item(item.clone());
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
            Action::MoveToEnd { membership_id } => {
                let mut next = (*self).clone();
                let Some(parent) = next.memberships.get(&membership_id).map(|m| m.parent_id)
                else {
                    return Rc::new(next);
                };
                let next_pos = self
                    .children(parent, true)
                    .into_iter()
                    .filter(|(_, m)| m.id != membership_id)
                    .last()
                    .map(|(_, m)| m.position + 1.0)
                    .unwrap_or(1.0);
                if let Some(m) = next.memberships.get_mut(&membership_id) {
                    m.position = next_pos;
                    m.updated_at = Utc::now();
                    store::put_membership(m.clone());
                }
                Rc::new(next)
            }
            Action::MoveInto {
                membership_id,
                target_item_id,
            } => {
                let mut next = (*self).clone();
                let Some(dragged_item_id) = next.memberships.get(&membership_id).map(|m| m.item_id)
                else {
                    return Rc::new(next);
                };
                // Refuse to move an item into itself or one of its own descendants.
                if dragged_item_id == target_item_id
                    || self.descendant_ids(dragged_item_id).contains(&target_item_id)
                {
                    return Rc::new(next);
                }
                let next_pos = self
                    .children(Some(target_item_id), true)
                    .last()
                    .map(|(_, m)| m.position + 1.0)
                    .unwrap_or(1.0);
                if let Some(m) = next.memberships.get_mut(&membership_id) {
                    m.parent_id = Some(target_item_id);
                    m.position = next_pos;
                    m.updated_at = Utc::now();
                    store::put_membership(m.clone());
                }
                if let Some(target) = next.items.get_mut(&target_item_id) {
                    if !target.is_list {
                        target.is_list = true;
                        target.updated_at = Utc::now();
                        store::put_item(target.clone());
                    }
                }
                Rc::new(next)
            }
            Action::MoveToRoot { membership_id } => {
                let mut next = (*self).clone();
                let next_pos = self
                    .children(None, true)
                    .into_iter()
                    .filter(|(_, m)| m.id != membership_id)
                    .last()
                    .map(|(_, m)| m.position + 1.0)
                    .unwrap_or(1.0);
                if let Some(m) = next.memberships.get_mut(&membership_id) {
                    m.parent_id = None;
                    m.position = next_pos;
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
            Action::ImportMarkdown {
                items,
                memberships,
                parent,
            } => {
                let mut next = (*self).clone();
                // Offset the parsed roots' positions so they land after whatever
                // children `parent` already has, instead of interleaving with them.
                let offset = next
                    .children(parent, true)
                    .last()
                    .map(|(_, m)| m.position)
                    .unwrap_or(0.0);
                for item in &items {
                    store::put_item(item.clone());
                }
                for membership in memberships {
                    let mut membership = membership;
                    if membership.parent_id.is_none() {
                        membership.parent_id = parent;
                        membership.position += offset;
                    }
                    store::put_membership(membership.clone());
                    next.memberships.insert(membership.id, membership);
                }
                for item in items {
                    next.items.insert(item.id, item);
                }
                if let Some(parent_id) = parent {
                    if let Some(parent_item) = next.items.get_mut(&parent_id) {
                        if !parent_item.is_note && !parent_item.is_list {
                            parent_item.is_list = true;
                            parent_item.updated_at = Utc::now();
                            store::put_item(parent_item.clone());
                        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn item(text: &str, is_note: bool, is_list: bool, done: bool) -> Item {
        let mut i = Item::new(text.to_string(), is_note);
        i.is_list = is_list;
        i.done = done;
        i
    }

    fn state_with(rows: Vec<Item>) -> AppState {
        let mut items = HashMap::new();
        let mut memberships = HashMap::new();
        for (position, item) in rows.into_iter().enumerate() {
            let membership = Membership::new(item.id, None, position as f64);
            memberships.insert(membership.id, membership);
            items.insert(item.id, item);
        }
        AppState {
            items,
            memberships,
            online: true,
            syncing: false,
        }
    }

    #[test]
    fn copy_as_markdown_prefixes_tasks_with_todo_or_done() {
        let state = state_with(vec![
            item("Buy milk", false, false, false),
            item("Pay rent", false, false, true),
            item("Just a note", true, false, false),
            item("A sublist", false, true, false),
        ]);
        assert_eq!(
            state.copy_as_markdown(None),
            "- TODO: Buy milk\n- DONE: Pay rent\n- Just a note\n- A sublist\n"
        );
    }

    /// A parent with children is exported as a list (no TODO:/DONE: prefix) even if
    /// `is_list` was never explicitly set on it - matching how a row with children
    /// always renders as a list regardless of that flag.
    #[test]
    fn copy_as_markdown_treats_items_with_children_as_lists() {
        let parent = item("Errands", false, false, false);
        let parent_id = parent.id;
        let child = item("Buy milk", false, false, false);
        let mut items = HashMap::new();
        let mut memberships = HashMap::new();
        memberships.insert(
            Uuid::new_v4(),
            Membership::new(parent.id, None, 1.0),
        );
        memberships.insert(
            Uuid::new_v4(),
            Membership::new(child.id, Some(parent_id), 1.0),
        );
        items.insert(parent.id, parent);
        items.insert(child.id, child);
        let state = AppState {
            items,
            memberships,
            online: true,
            syncing: false,
        };
        assert_eq!(
            state.copy_as_markdown(None),
            "- Errands\n  - TODO: Buy milk\n"
        );
    }

    /// The markdown import parser must accept the exact format `copy_as_markdown`
    /// produces - explicit `TODO:`/`DONE:` prefixes, nested with two-space indents -
    /// and reconstruct the same task/done states and parent/child structure.
    #[test]
    fn markdown_import_round_trips_the_copy_format() {
        let list = item("Errands", false, true, false);
        let list_id = list.id;
        let todo = item("Buy milk", false, false, false);
        let done = item("Pay rent", false, false, true);
        let top_task = item("Walk the dog", false, false, true);

        let mut items = HashMap::new();
        let mut memberships = HashMap::new();
        memberships.insert(Uuid::new_v4(), Membership::new(list.id, None, 1.0));
        memberships.insert(Uuid::new_v4(), Membership::new(todo.id, Some(list_id), 1.0));
        memberships.insert(Uuid::new_v4(), Membership::new(done.id, Some(list_id), 2.0));
        memberships.insert(Uuid::new_v4(), Membership::new(top_task.id, None, 2.0));
        items.insert(list.id, list);
        items.insert(todo.id, todo);
        items.insert(done.id, done);
        items.insert(top_task.id, top_task);
        let state = AppState {
            items,
            memberships,
            online: true,
            syncing: false,
        };

        let markdown = state.copy_as_markdown(None);
        assert_eq!(
            markdown,
            "- Errands\n  - TODO: Buy milk\n  - DONE: Pay rent\n- DONE: Walk the dog\n"
        );

        let import = crate::markdown::parse_markdown(&markdown);
        let find = |text: &str| import.items.iter().find(|i| i.text == text).unwrap();
        let parent_of = |item_id: Uuid| {
            import
                .memberships
                .iter()
                .find(|m| m.item_id == item_id)
                .and_then(|m| m.parent_id)
        };

        let errands = find("Errands");
        let milk = find("Buy milk");
        let rent = find("Pay rent");
        let dog = find("Walk the dog");

        assert!(!milk.done);
        assert!(rent.done);
        assert!(dog.done);
        assert_eq!(parent_of(milk.id), Some(errands.id));
        assert_eq!(parent_of(rent.id), Some(errands.id));
        assert_eq!(parent_of(dog.id), None);
    }
}
