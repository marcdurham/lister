use crate::model::{Item, Membership};
use crate::state::{Action, AppState};
use uuid::Uuid;
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct ComposerProps {
    pub state: UseReducerHandle<AppState>,
    pub parent: Option<Uuid>,
}

#[function_component(Composer)]
pub fn composer(props: &ComposerProps) -> Html {
    let draft = use_state(String::new);
    let is_note = use_state(|| false);

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
                class={if *is_note { "toggle note active" } else { "toggle note" }}
                onclick={toggle_note}
                title="Toggle note vs task"
            >
                { "Note" }
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

    html! {
        <nav class="breadcrumbs">
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
}

#[function_component(ItemRow)]
pub fn item_row(props: &ItemRowProps) -> Html {
    let notes_open = use_state(|| false);
    let picker_open = use_state(|| false);

    let item = &props.item;
    let membership = &props.membership;
    let state = props.state.clone();
    let has_children = state.has_children(item.id);

    let toggle_done = {
        let state = state.clone();
        let id = item.id;
        Callback::from(move |_: MouseEvent| state.dispatch(Action::ToggleDone(id)))
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
        Callback::from(move |_: MouseEvent| on_open.emit(id))
    };

    let toggle_visible = {
        let state = state.clone();
        let id = membership.id;
        Callback::from(move |_: MouseEvent| state.dispatch(Action::ToggleVisible(id)))
    };

    let remove = {
        let state = state.clone();
        let id = membership.id;
        Callback::from(move |_: MouseEvent| state.dispatch(Action::RemoveFromList(id)))
    };

    let move_up = props.prev_id.map(|prev_id| {
        let state = state.clone();
        let id = membership.id;
        Callback::from(move |_: MouseEvent| {
            state.dispatch(Action::Reorder {
                membership_id: id,
                swap_with: prev_id,
            })
        })
    });

    let move_down = props.next_id.map(|next_id| {
        let state = state.clone();
        let id = membership.id;
        Callback::from(move |_: MouseEvent| {
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

    let row_class = classes!(
        "item",
        item.done.then_some("done"),
        (!membership.visible).then_some("hidden-row")
    );

    html! {
        <li class={row_class}>
            <div class="item-main">
                <div class="reorder">
                    <button disabled={props.is_first} onclick={move_up.clone().unwrap_or_default()}>{ "▲" }</button>
                    <button disabled={props.is_last} onclick={move_down.clone().unwrap_or_default()}>{ "▼" }</button>
                </div>
                if !item.is_note {
                    <input type="checkbox" checked={item.done} onclick={toggle_done} />
                } else {
                    <span class="note-badge">{ "note" }</span>
                }
                <span class="item-text" onclick={open.clone()}>{ &item.text }</span>
                <button class={classes!("open-btn", (!has_children).then_some("subtle"))} onclick={open}>
                    { "Open ▸" }
                </button>
            </div>
            <div class="item-actions">
                <button onclick={toggle_notes}>{ if item.notes.is_some() { "Notes*" } else { "Notes" } }</button>
                <button onclick={toggle_visible}>{ if membership.visible { "Hide" } else { "Show" } }</button>
                <button onclick={toggle_picker}>{ "Add to list" }</button>
                <button class="remove" onclick={remove}>{ "Remove" }</button>
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
