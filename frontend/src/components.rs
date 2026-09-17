use crate::auth;
use crate::backup;
use crate::google::{self, ImportedTaskList};
use crate::model::{Item, Membership};
use crate::state::{Action, AppState};
use std::collections::HashSet;
use uuid::Uuid;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{DragEvent, FileReader, HtmlInputElement};
use yew::prelude::*;

fn note_icon() -> Html {
    html! {
        <svg class="note-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
            <rect x="2" y="1.5" width="12" height="13" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.3"/>
            <line x1="4.5" y1="5" x2="11.5" y2="5" stroke="currentColor" stroke-width="1.1"/>
            <line x1="4.5" y1="7.7" x2="11.5" y2="7.7" stroke="currentColor" stroke-width="1.1"/>
            <line x1="4.5" y1="10.4" x2="9" y2="10.4" stroke="currentColor" stroke-width="1.1"/>
        </svg>
    }
}

/// Icon shown for a task that has children - it's really a sub-list now, not a checkable item.
fn list_icon() -> Html {
    html! {
        <svg class="list-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
            <circle cx="2.2" cy="3.2" r="1" fill="currentColor"/>
            <circle cx="2.2" cy="8" r="1" fill="currentColor"/>
            <circle cx="2.2" cy="12.8" r="1" fill="currentColor"/>
            <line x1="5" y1="3.2" x2="14" y2="3.2" stroke="currentColor" stroke-width="1.3"/>
            <line x1="5" y1="8" x2="14" y2="8" stroke="currentColor" stroke-width="1.3"/>
            <line x1="5" y1="12.8" x2="14" y2="12.8" stroke="currentColor" stroke-width="1.3"/>
        </svg>
    }
}

/// Six-dot grip handle used to drag-reorder a row.
fn drag_handle_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="12" height="16" aria-hidden="true">
            <circle cx="5" cy="3" r="1.2" fill="currentColor"/>
            <circle cx="5" cy="8" r="1.2" fill="currentColor"/>
            <circle cx="5" cy="13" r="1.2" fill="currentColor"/>
            <circle cx="11" cy="3" r="1.2" fill="currentColor"/>
            <circle cx="11" cy="8" r="1.2" fill="currentColor"/>
            <circle cx="11" cy="13" r="1.2" fill="currentColor"/>
        </svg>
    }
}

fn edit_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <path
                d="M11.1 1.6a1.6 1.6 0 0 1 2.3 2.3l-7.9 7.9-3 .7.7-3 7.9-7.9z"
                fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" stroke-linecap="round"
            />
        </svg>
    }
}

fn more_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <circle cx="3" cy="8" r="1.3" fill="currentColor"/>
            <circle cx="8" cy="8" r="1.3" fill="currentColor"/>
            <circle cx="13" cy="8" r="1.3" fill="currentColor"/>
        </svg>
    }
}

/// Compact "browse all" icon for switching the add-to-list picker from search to navigation.
fn show_all_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <rect x="1.5" y="1.5" width="5.5" height="5.5" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/>
            <rect x="9" y="1.5" width="5.5" height="5.5" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/>
            <rect x="1.5" y="9" width="5.5" height="5.5" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/>
            <rect x="9" y="9" width="5.5" height="5.5" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/>
        </svg>
    }
}

fn format_ts(ts: &chrono::DateTime<chrono::Utc>) -> String {
    ts.format("%Y-%m-%d %H:%M").to_string()
}

#[derive(Properties, PartialEq)]
pub struct AuthScreenProps {
    pub on_authed: Callback<auth::User>,
}

#[function_component(AuthScreen)]
pub fn auth_screen(props: &AuthScreenProps) -> Html {
    let mode_register = use_state(|| false);
    let email = use_state(String::new);
    let password = use_state(String::new);
    let error = use_state(|| None::<String>);
    let busy = use_state(|| false);

    let on_email = {
        let email = email.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            email.set(input.value());
        })
    };
    let on_password = {
        let password = password.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            password.set(input.value());
        })
    };

    let toggle_mode = {
        let mode_register = mode_register.clone();
        let error = error.clone();
        Callback::from(move |_: MouseEvent| {
            mode_register.set(!*mode_register);
            error.set(None);
        })
    };

    let submit = {
        let email = email.clone();
        let password = password.clone();
        let mode_register = mode_register.clone();
        let error = error.clone();
        let busy = busy.clone();
        let on_authed = props.on_authed.clone();
        Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            if *busy {
                return;
            }
            let email_v = (*email).clone();
            let password_v = (*password).clone();
            let is_register = *mode_register;
            let error = error.clone();
            let busy = busy.clone();
            let on_authed = on_authed.clone();
            busy.set(true);
            spawn_local(async move {
                let result = if is_register {
                    auth::register(&email_v, &password_v).await
                } else {
                    auth::login(&email_v, &password_v).await
                };
                busy.set(false);
                match result {
                    Ok(user) => {
                        error.set(None);
                        on_authed.emit(user);
                    }
                    Err(msg) => error.set(Some(msg)),
                }
            });
        })
    };

    html! {
        <div class="auth-screen">
            <form class="auth-card">
                <h1>{ "Lister" }</h1>
                <h2>{ if *mode_register { "Create account" } else { "Sign in" } }</h2>
                <label class="editor-field">
                    <span>{ "Email" }</span>
                    <input type="email" value={(*email).clone()} oninput={on_email} />
                </label>
                <label class="editor-field">
                    <span>{ "Password" }</span>
                    <input type="password" value={(*password).clone()} oninput={on_password} />
                </label>
                if let Some(msg) = &*error {
                    <p class="auth-error">{ msg }</p>
                }
                <button class="auth-submit" type="submit" onclick={submit} disabled={*busy}>
                    { if *busy { "Please wait..." } else if *mode_register { "Create account" } else { "Sign in" } }
                </button>
                <button class="auth-switch" type="button" onclick={toggle_mode}>
                    { if *mode_register { "Already have an account? Sign in" } else { "Need an account? Create one" } }
                </button>
            </form>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct ComposerProps {
    pub state: UseReducerHandle<AppState>,
    pub parent: Option<Uuid>,
}

#[function_component(Composer)]
pub fn composer(props: &ComposerProps) -> Html {
    let draft = use_state(String::new);
    // Notes are the default item type; the toggle flips it to a task.
    let is_note = use_state(|| true);

    let on_input = {
        let draft = draft.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            draft.set(input.value());
        })
    };

    let submit = {
        let draft = draft.clone();
        let is_note = is_note.clone();
        let state = props.state.clone();
        let parent = props.parent;
        move || {
            let text = draft.trim().to_string();
            if text.is_empty() {
                return;
            }
            state.dispatch(Action::AddItem {
                text,
                is_note: *is_note,
                parent,
            });
            draft.set(String::new());
        }
    };

    let onclick = {
        let submit = submit.clone();
        Callback::from(move |_: MouseEvent| submit())
    };
    let onkeypress = {
        let submit = submit.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Enter" {
                submit();
            }
        })
    };
    let toggle_note = {
        let is_note = is_note.clone();
        Callback::from(move |_: MouseEvent| is_note.set(!*is_note))
    };

    html! {
        <div class="composer">
            <input
                type="text"
                placeholder={if *is_note { "Add a note..." } else { "Add a task..." }}
                value={(*draft).clone()}
                oninput={on_input}
                {onkeypress}
            />
            <button
                class={if *is_note { "toggle task" } else { "toggle task active" }}
                onclick={toggle_note}
                title="Toggle note vs task"
            >
                { if *is_note { "Task" } else { "Note" } }
            </button>
            <button {onclick}>{ "Add" }</button>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct BreadcrumbsProps {
    pub state: UseReducerHandle<AppState>,
    pub path: Vec<Uuid>,
    pub on_navigate: Callback<Vec<Uuid>>,
}

#[function_component(Breadcrumbs)]
pub fn breadcrumbs(props: &BreadcrumbsProps) -> Html {
    let home = {
        let on_navigate = props.on_navigate.clone();
        Callback::from(move |_: MouseEvent| on_navigate.emit(vec![]))
    };

    let up = {
        let on_navigate = props.on_navigate.clone();
        let path = props.path.clone();
        Callback::from(move |_: MouseEvent| {
            let mut next = path.clone();
            next.pop();
            on_navigate.emit(next);
        })
    };

    html! {
        <nav class="breadcrumbs">
            if !props.path.is_empty() {
                <button class="up-btn" onclick={up} title="Up one level">{ "↑ Up" }</button>
            }
            <a onclick={home}>{ "Lists" }</a>
            { for props.path.iter().enumerate().map(|(idx, id)| {
                let name = props.state.items.get(id).map(|i| i.text.clone()).unwrap_or_default();
                let path = props.path.clone();
                let on_navigate = props.on_navigate.clone();
                let target: Vec<Uuid> = path[..=idx].to_vec();
                let onclick = Callback::from(move |_: MouseEvent| on_navigate.emit(target.clone()));
                html! {
                    <>
                        <span class="sep">{ "›" }</span>
                        <a {onclick}>{ name }</a>
                    </>
                }
            }) }
        </nav>
    }
}

#[derive(Properties, PartialEq)]
pub struct ItemRowProps {
    pub state: UseReducerHandle<AppState>,
    pub item: Item,
    pub membership: Membership,
    pub on_open: Callback<Uuid>,
    pub on_edit: Callback<Uuid>,
}

#[function_component(ItemRow)]
pub fn item_row(props: &ItemRowProps) -> Html {
    let expanded = use_state(|| false);
    let notes_open = use_state(|| false);
    let picker_open = use_state(|| false);
    let confirm_remove = use_state(|| false);
    let remove_children = use_state(|| true);
    let drag_over = use_state(|| false);

    let item = &props.item;
    let membership = &props.membership;
    let state = props.state.clone();
    let child_count = state.direct_child_count(item.id);
    let is_list = !item.is_note && (item.is_list || child_count > 0);

    let toggle_done = {
        let state = state.clone();
        let id = item.id;
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            state.dispatch(Action::ToggleDone(id));
        })
    };

    // Tapping the row opens the sub-list if this item is (or acts as) a list, otherwise edits it.
    let open_or_edit = {
        let on_open = props.on_open.clone();
        let on_edit = props.on_edit.clone();
        let id = item.id;
        let navigate_in = is_list;
        Callback::from(move |_: MouseEvent| {
            if navigate_in {
                on_open.emit(id);
            } else {
                on_edit.emit(id);
            }
        })
    };

    let toggle_expanded = {
        let expanded = expanded.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            expanded.set(!*expanded);
        })
    };

    let toggle_notes = {
        let notes_open = notes_open.clone();
        Callback::from(move |_: MouseEvent| notes_open.set(!*notes_open))
    };

    let on_notes_blur = {
        let state = state.clone();
        let id = item.id;
        Callback::from(move |e: FocusEvent| {
            let input: web_sys::HtmlTextAreaElement = e.target_unchecked_into();
            state.dispatch(Action::UpdateNotes {
                item_id: id,
                notes: input.value(),
            });
        })
    };

    let edit = {
        let on_edit = props.on_edit.clone();
        let id = item.id;
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            on_edit.emit(id);
        })
    };

    let toggle_visible = {
        let state = state.clone();
        let id = membership.id;
        Callback::from(move |_: MouseEvent| state.dispatch(Action::ToggleVisible(id)))
    };

    let open_confirm_remove = {
        let confirm_remove = confirm_remove.clone();
        Callback::from(move |_: MouseEvent| confirm_remove.set(true))
    };

    let cancel_remove = {
        let confirm_remove = confirm_remove.clone();
        Callback::from(move |_: MouseEvent| confirm_remove.set(false))
    };

    let toggle_remove_children = {
        let remove_children = remove_children.clone();
        Callback::from(move |_: MouseEvent| remove_children.set(!*remove_children))
    };

    let confirm_remove_click = {
        let state = state.clone();
        let id = item.id;
        let remove_children = remove_children.clone();
        let confirm_remove = confirm_remove.clone();
        Callback::from(move |_: MouseEvent| {
            state.dispatch(Action::RemoveItem {
                item_id: id,
                remove_children: *remove_children,
            });
            confirm_remove.set(false);
        })
    };

    let toggle_picker = {
        let picker_open = picker_open.clone();
        Callback::from(move |_: MouseEvent| picker_open.set(!*picker_open))
    };

    // Drag-to-reorder: the handle starts the drag, and dropping onto another row swaps
    // the two items' positions (the same swap the old up/down buttons performed).
    let on_drag_start = {
        let id = membership.id;
        Callback::from(move |e: DragEvent| {
            if let Some(dt) = e.data_transfer() {
                let _ = dt.set_data("text/plain", &id.to_string());
                dt.set_effect_allowed("move");
            }
        })
    };

    let on_handle_click = Callback::from(|e: MouseEvent| e.stop_propagation());

    let on_drag_over = {
        let drag_over = drag_over.clone();
        Callback::from(move |e: DragEvent| {
            e.prevent_default();
            drag_over.set(true);
        })
    };

    let on_drag_leave = {
        let drag_over = drag_over.clone();
        Callback::from(move |_: DragEvent| drag_over.set(false))
    };

    let on_drop = {
        let state = state.clone();
        let drag_over = drag_over.clone();
        let id = membership.id;
        Callback::from(move |e: DragEvent| {
            e.prevent_default();
            drag_over.set(false);
            let Some(dt) = e.data_transfer() else { return };
            let Ok(dragged) = dt.get_data("text/plain") else {
                return;
            };
            if let Ok(dragged_id) = Uuid::parse_str(&dragged) {
                if dragged_id != id {
                    state.dispatch(Action::Reorder {
                        membership_id: dragged_id,
                        swap_with: id,
                    });
                }
            }
        })
    };

    let removable_children = state.only_child_descendant_ids(item.id).len();

    let notes_preview = item
        .notes
        .as_deref()
        .and_then(|n| n.lines().next())
        .map(|line| line.trim())
        .filter(|line| !line.is_empty());

    let row_class = classes!(
        "item",
        item.done.then_some("done"),
        (!membership.visible).then_some("hidden-row"),
        (*drag_over).then_some("drag-over")
    );

    html! {
        <li
            class={row_class}
            ondragover={on_drag_over}
            ondragleave={on_drag_leave}
            ondrop={on_drop}
        >
            <div class="item-main" onclick={open_or_edit}>
                <span
                    class="drag-handle"
                    draggable="true"
                    ondragstart={on_drag_start}
                    onclick={on_handle_click}
                    title="Drag to reorder"
                >
                    { drag_handle_icon() }
                </span>
                if item.is_note {
                    { note_icon() }
                } else if is_list {
                    { list_icon() }
                } else {
                    <input type="checkbox" checked={item.done} onclick={toggle_done} />
                }
                <div class="item-title-group">
                    <span class="item-text">{ &item.text }</span>
                    if let Some(preview) = notes_preview {
                        <span class="item-notes-preview">{ preview }</span>
                    }
                </div>
                if child_count > 0 {
                    <span class="child-count">{ child_count }</span>
                }
                <button class="more-btn" onclick={toggle_expanded} title="More actions" aria-label="More actions">
                    { more_icon() }
                </button>
                <button class="edit-btn" onclick={edit} title="Edit" aria-label="Edit">
                    { edit_icon() }
                </button>
            </div>
            if *expanded {
                <div class="item-actions">
                    <button onclick={toggle_notes}>{ if item.notes.is_some() { "Notes*" } else { "Notes" } }</button>
                    <button onclick={toggle_visible}>{ if membership.visible { "Hide" } else { "Show" } }</button>
                    <button onclick={toggle_picker}>{ "Add to list" }</button>
                    <button class="remove" onclick={open_confirm_remove}>{ "Remove" }</button>
                </div>
                if *notes_open {
                    <textarea
                        class="notes"
                        placeholder="Details..."
                        onblur={on_notes_blur}
                        value={item.notes.clone().unwrap_or_default()}
                    />
                }
                if *picker_open {
                    <AddToListPicker state={state.clone()} item_id={item.id} current_parent={membership.parent_id} />
                }
                if *confirm_remove {
                    <div class="confirm-remove">
                        <p>
                            { "Remove \u{201c}" }{ &item.text }{ "\u{201d}?" }
                            if removable_children > 0 {
                                { format!(" It has {removable_children} item(s) that only live here.") }
                            }
                        </p>
                        if removable_children > 0 {
                            <label class="confirm-children">
                                <input type="checkbox" checked={*remove_children} onclick={toggle_remove_children} />
                                { format!(" Also remove {removable_children} child item(s)") }
                            </label>
                        }
                        <div class="confirm-actions">
                            <button class="remove" onclick={confirm_remove_click}>{ "Remove" }</button>
                            <button onclick={cancel_remove}>{ "Cancel" }</button>
                        </div>
                    </div>
                }
            }
        </li>
    }
}

#[derive(Properties, PartialEq)]
pub struct AddToListPickerProps {
    pub state: UseReducerHandle<AppState>,
    pub item_id: Uuid,
    pub current_parent: Option<Uuid>,
}

#[function_component(AddToListPicker)]
pub fn add_to_list_picker(props: &AddToListPickerProps) -> Html {
    let query = use_state(String::new);
    let show_all = use_state(|| false);

    let item_id = props.item_id;
    let state = props.state.clone();
    let Some(item) = state.items.get(&item_id).cloned() else {
        return html! {};
    };

    let on_query_input = {
        let query = query.clone();
        let show_all = show_all.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            show_all.set(false);
            query.set(input.value());
        })
    };

    let toggle_show_all = {
        let show_all = show_all.clone();
        let query = query.clone();
        Callback::from(move |_: MouseEvent| {
            query.set(String::new());
            show_all.set(!*show_all);
        })
    };

    let query_lower = query.trim().to_lowercase();
    let candidates: Vec<Item> = if *show_all {
        state.top_level_lists()
    } else if query_lower.is_empty() {
        Vec::new()
    } else {
        let mut matches: Vec<Item> = state
            .items
            .values()
            .filter(|i| i.deleted_at.is_none())
            .filter(|i| i.id != item_id)
            .filter(|i| i.text.to_lowercase().contains(&query_lower))
            .cloned()
            .collect();
        matches.sort_by(|a, b| a.text.to_lowercase().cmp(&b.text.to_lowercase()));
        matches.truncate(20);
        matches
    };

    html! {
        <div class="picker">
            <div class="picker-current">
                <span class="picker-current-label">{ "Add to list:" }</span>
                <span class="picker-current-text">{ &item.text }</span>
            </div>
            <div class="picker-search">
                <input
                    type="text"
                    placeholder="Search for a list..."
                    value={(*query).clone()}
                    oninput={on_query_input}
                />
                <button
                    class={classes!("show-all-btn", (*show_all).then_some("active"))}
                    onclick={toggle_show_all}
                    title="Browse all lists"
                    aria-label="Browse all lists"
                >
                    { show_all_icon() }
                </button>
            </div>
            <div class="picker-results">
                { for candidates.iter().filter(|l| Some(l.id) != props.current_parent).map(|list| {
                    let state = state.clone();
                    let parent = list.id;
                    let onclick = Callback::from(move |_: MouseEvent| {
                        state.dispatch(Action::AddToList { item_id, parent: Some(parent) });
                    });
                    html! { <button {onclick}>{ &list.text }</button> }
                }) }
                if candidates.is_empty() {
                    if *show_all {
                        <span class="hint">{ "No other lists yet." }</span>
                    } else if query_lower.is_empty() {
                        <span class="hint">{ "Type to search, or tap the icon to browse all lists." }</span>
                    } else {
                        <span class="hint">{ "No matches." }</span>
                    }
                }
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct ItemEditorProps {
    pub state: UseReducerHandle<AppState>,
    pub item_id: Uuid,
    pub on_close: Callback<()>,
}

#[function_component(ItemEditor)]
pub fn item_editor(props: &ItemEditorProps) -> Html {
    let Some(item) = props.state.items.get(&props.item_id).cloned() else {
        return html! {};
    };
    let state = props.state.clone();
    let text = use_state(|| item.text.clone());

    let on_text_input = {
        let text = text.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            text.set(input.value());
        })
    };

    let close = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| on_close.emit(()))
    };

    let save = {
        let state = state.clone();
        let id = item.id;
        let text = text.clone();
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| {
            let trimmed = text.trim().to_string();
            if !trimmed.is_empty() {
                state.dispatch(Action::UpdateText {
                    item_id: id,
                    text: trimmed,
                });
            }
            on_close.emit(());
        })
    };

    let convert_to = |is_note: bool, is_list: bool| {
        let state = state.clone();
        let id = item.id;
        Callback::from(move |_: MouseEvent| {
            state.dispatch(Action::SetItemKind {
                item_id: id,
                is_note,
                is_list,
            })
        })
    };

    let kind_label = if item.is_note {
        "note"
    } else if item.is_list {
        "list"
    } else {
        "task"
    };

    html! {
        <div class="editor-overlay">
            <div class="editor">
                <h2>{ "Edit item" }</h2>
                <label class="editor-field">
                    <span>{ "Text" }</span>
                    <input type="text" value={(*text).clone()} oninput={on_text_input} />
                </label>
                <div class="editor-type">
                    <span>{ format!("Currently a {kind_label}") }</span>
                    <div class="editor-type-actions">
                        if !item.is_note && !item.is_list {
                            <button onclick={convert_to(true, false)}>{ "Convert to note" }</button>
                        }
                        if !item.is_list {
                            <button onclick={convert_to(false, true)}>{ "Convert to list" }</button>
                        }
                        if item.is_note || item.is_list {
                            <button onclick={convert_to(false, false)}>{ "Convert to task" }</button>
                        }
                    </div>
                </div>
                <div class="editor-meta">
                    <div>{ "Created: " }{ format_ts(&item.created_at) }</div>
                    <div>{ "Updated: " }{ format_ts(&item.updated_at) }</div>
                </div>
                <div class="editor-actions">
                    <button onclick={save}>{ "Save" }</button>
                    <button onclick={close}>{ "Cancel" }</button>
                </div>
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct TrashViewProps {
    pub state: UseReducerHandle<AppState>,
    pub on_close: Callback<()>,
}

#[function_component(TrashView)]
pub fn trash_view(props: &TrashViewProps) -> Html {
    let items = props.state.trashed_items();
    let state = props.state.clone();
    let close = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| on_close.emit(()))
    };

    html! {
        <div class="trash-view">
            <div class="trash-header">
                <h2>{ "Trash" }</h2>
                <button onclick={close}>{ "Back" }</button>
            </div>
            if items.is_empty() {
                <p class="empty">{ "Trash is empty." }</p>
            }
            <ul class="items">
                { for items.iter().map(|item| {
                    let state = state.clone();
                    let id = item.id;
                    let restore = Callback::from(move |_: MouseEvent| state.dispatch(Action::RestoreItem(id)));
                    html! {
                        <li class="item trash-item" key={item.id.to_string()}>
                            <div class="item-main">
                                if item.is_note { { note_icon() } } else {
                                    <input type="checkbox" checked={item.done} disabled=true />
                                }
                                <span class="item-text">{ &item.text }</span>
                            </div>
                            <div class="item-actions">
                                <button onclick={restore}>{ "Restore" }</button>
                            </div>
                        </li>
                    }
                }) }
            </ul>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct SettingsMenuProps {
    pub state: UseReducerHandle<AppState>,
}

#[function_component(SettingsMenu)]
pub fn settings_menu(props: &SettingsMenuProps) -> Html {
    let open = use_state(|| false);
    let file_input_ref = use_node_ref();

    let toggle = {
        let open = open.clone();
        Callback::from(move |_: MouseEvent| open.set(!*open))
    };

    let import_google = {
        let open = open.clone();
        Callback::from(move |_: MouseEvent| {
            open.set(false);
            google::start_connect();
        })
    };

    let export_json = {
        let open = open.clone();
        let state = props.state.clone();
        Callback::from(move |_: MouseEvent| {
            open.set(false);
            let items: Vec<Item> = state.items.values().cloned().collect();
            let memberships: Vec<Membership> = state.memberships.values().cloned().collect();
            let json = backup::export_json(items, memberships);
            let filename = format!("lister-export-{}.json", chrono::Utc::now().format("%Y-%m-%d"));
            backup::download_json(&filename, &json);
        })
    };

    let choose_import_file = {
        let open = open.clone();
        let file_input_ref = file_input_ref.clone();
        Callback::from(move |_: MouseEvent| {
            open.set(false);
            if let Some(input) = file_input_ref.cast::<HtmlInputElement>() {
                input.click();
            }
        })
    };

    let on_file_change = {
        let state = props.state.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            let Some(file) = input.files().and_then(|list| list.get(0)) else {
                return;
            };
            input.set_value("");

            let state = state.clone();
            let Ok(reader) = FileReader::new() else {
                return;
            };
            let reader_for_load = reader.clone();
            let onload = Closure::once(move |_: Event| {
                let Ok(result) = reader_for_load.result() else {
                    return;
                };
                let Some(text) = result.as_string() else {
                    return;
                };
                match backup::parse_import(&text) {
                    Ok(data) => {
                        let confirmed = web_sys::window()
                            .and_then(|w| {
                                w.confirm_with_message(
                                    "Importing will replace all of your current tasks with the contents of this file. Continue?",
                                )
                                .ok()
                            })
                            .unwrap_or(false);
                        if confirmed {
                            state.dispatch(Action::ImportJson {
                                items: data.items,
                                memberships: data.memberships,
                            });
                        }
                    }
                    Err(_) => {
                        if let Some(window) = web_sys::window() {
                            let _ = window
                                .alert_with_message("That file doesn't look like a valid Lister export.");
                        }
                    }
                }
            });
            reader.set_onload(Some(onload.as_ref().unchecked_ref()));
            onload.forget();
            let _ = reader.read_as_text(&file);
        })
    };

    html! {
        <div class="settings-menu">
            <button class="settings-btn" onclick={toggle} title="Settings" aria-label="Settings">
                { "⚙" }
            </button>
            if *open {
                <div class="settings-dropdown">
                    <button class="settings-item" onclick={import_google}>
                        { "Import from Google Tasks" }
                    </button>
                    <button class="settings-item" onclick={export_json}>
                        { "Export tasks (JSON)" }
                    </button>
                    <button class="settings-item" onclick={choose_import_file}>
                        { "Import tasks (JSON)" }
                    </button>
                </div>
            }
            <input
                type="file"
                accept="application/json"
                ref={file_input_ref}
                onchange={on_file_change}
                style="display: none;"
            />
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct GoogleImportDialogProps {
    pub state: UseReducerHandle<AppState>,
    pub lists: Vec<ImportedTaskList>,
    pub on_close: Callback<()>,
}

#[function_component(GoogleImportDialog)]
pub fn google_import_dialog(props: &GoogleImportDialogProps) -> Html {
    let selected = use_state(|| (0..props.lists.len()).collect::<HashSet<usize>>());

    let close = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| on_close.emit(()))
    };

    let import = {
        let state = props.state.clone();
        let lists = props.lists.clone();
        let selected = selected.clone();
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| {
            let chosen: Vec<ImportedTaskList> = lists
                .iter()
                .enumerate()
                .filter(|(idx, _)| selected.contains(idx))
                .map(|(_, l)| l.clone())
                .collect();
            if !chosen.is_empty() {
                state.dispatch(Action::ImportGoogleTasks(chosen));
            }
            on_close.emit(());
        })
    };

    html! {
        <div class="editor-overlay">
            <div class="editor">
                <h2>{ "Import from Google Tasks" }</h2>
                if props.lists.is_empty() {
                    <p>{ "No Google task lists were found for this account." }</p>
                } else {
                    <ul class="google-import-lists">
                        { for props.lists.iter().enumerate().map(|(idx, list)| {
                            let is_checked = selected.contains(&idx);
                            let selected = selected.clone();
                            let onclick = Callback::from(move |_: MouseEvent| {
                                let mut next = (*selected).clone();
                                if next.contains(&idx) {
                                    next.remove(&idx);
                                } else {
                                    next.insert(idx);
                                }
                                selected.set(next);
                            });
                            html! {
                                <li>
                                    <label>
                                        <input type="checkbox" checked={is_checked} {onclick} />
                                        { format!(" {} ({} tasks)", list.title, list.tasks.len()) }
                                    </label>
                                </li>
                            }
                        }) }
                    </ul>
                }
                <div class="editor-actions">
                    if !props.lists.is_empty() {
                        <button onclick={import}>{ "Import selected" }</button>
                    }
                    <button onclick={close}>{ "Cancel" }</button>
                </div>
            </div>
        </div>
    }
}
