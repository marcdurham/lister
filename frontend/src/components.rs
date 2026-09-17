use crate::auth;
use crate::google::{self, ImportedTaskList};
use crate::model::{Item, Membership};
use crate::state::{Action, AppState};
use std::collections::HashSet;
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
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
    pub is_first: bool,
    pub is_last: bool,
    pub prev_id: Option<Uuid>,
    pub next_id: Option<Uuid>,
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

    let item = &props.item;
    let membership = &props.membership;
    let state = props.state.clone();
    let child_count = state.direct_child_count(item.id);

    let toggle_done = {
        let state = state.clone();
        let id = item.id;
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            state.dispatch(Action::ToggleDone(id));
        })
    };

    let toggle_expanded = {
        let expanded = expanded.clone();
        Callback::from(move |_: MouseEvent| expanded.set(!*expanded))
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

    let open = {
        let on_open = props.on_open.clone();
        let id = item.id;
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            on_open.emit(id);
        })
    };

    let edit = {
        let on_edit = props.on_edit.clone();
        let id = item.id;
        Callback::from(move |_: MouseEvent| on_edit.emit(id))
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

    let move_up = props.prev_id.map(|prev_id| {
        let state = state.clone();
        let id = membership.id;
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            state.dispatch(Action::Reorder {
                membership_id: id,
                swap_with: prev_id,
            })
        })
    });

    let move_down = props.next_id.map(|next_id| {
        let state = state.clone();
        let id = membership.id;
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            state.dispatch(Action::Reorder {
                membership_id: id,
                swap_with: next_id,
            })
        })
    });

    let toggle_picker = {
        let picker_open = picker_open.clone();
        Callback::from(move |_: MouseEvent| picker_open.set(!*picker_open))
    };

    let removable_children = state.only_child_descendant_ids(item.id).len();

    let row_class = classes!(
        "item",
        item.done.then_some("done"),
        (!membership.visible).then_some("hidden-row")
    );

    html! {
        <li class={row_class}>
            <div class="item-main" onclick={toggle_expanded}>
                <div class="reorder">
                    <button disabled={props.is_first} onclick={move_up.clone().unwrap_or_default()}>{ "▲" }</button>
                    <button disabled={props.is_last} onclick={move_down.clone().unwrap_or_default()}>{ "▼" }</button>
                </div>
                if !item.is_note {
                    <input type="checkbox" checked={item.done} onclick={toggle_done} />
                } else {
                    { note_icon() }
                }
                <span class="item-text">{ &item.text }</span>
                if child_count > 0 {
                    <span class="child-count">{ child_count }</span>
                }
                <button class={classes!("open-btn", (!state.has_children(item.id)).then_some("subtle"))} onclick={open}>
                    { "Open ▸" }
                </button>
            </div>
            if *expanded {
                <div class="item-actions">
                    <button onclick={edit}>{ "Edit" }</button>
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
    let lists = props.state.top_level_lists();
    let item_id = props.item_id;
    let state = props.state.clone();

    html! {
        <div class="picker">
            { for lists.iter().filter(|l| Some(l.id) != props.current_parent && l.id != item_id).map(|list| {
                let state = state.clone();
                let parent = list.id;
                let onclick = Callback::from(move |_: MouseEvent| {
                    state.dispatch(Action::AddToList { item_id, parent: Some(parent) });
                });
                html! { <button {onclick}>{ &list.text }</button> }
            }) }
            if lists.is_empty() {
                <span class="hint">{ "No other top-level lists yet." }</span>
            }
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

    let convert = {
        let state = state.clone();
        let id = item.id;
        Callback::from(move |_: MouseEvent| state.dispatch(Action::ConvertType(id)))
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
                    <span>{ if item.is_note { "Currently a note" } else { "Currently a task" } }</span>
                    <button onclick={convert}>
                        { if item.is_note { "Convert to task" } else { "Convert to note" } }
                    </button>
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

#[function_component(SettingsMenu)]
pub fn settings_menu() -> Html {
    let open = use_state(|| false);

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
                </div>
            }
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
