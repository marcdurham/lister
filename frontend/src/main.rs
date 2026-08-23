mod model;
mod storage;

use model::{Item, ListState};
use web_sys::{HtmlInputElement, KeyboardEvent};
use yew::prelude::*;

#[function_component(App)]
fn app() -> Html {
    let state = use_state(|| storage::load().unwrap_or_default());
    let draft = use_state(String::new);

    let on_input = {
        let draft = draft.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            draft.set(input.value());
        })
    };

    let submit = {
        let state = state.clone();
        let draft = draft.clone();
        move || {
            let text = (*draft).trim().to_string();
            if text.is_empty() {
                return;
            }
            let mut next: ListState = (*state).clone();
            next.items.push(Item::new(text));
            storage::save(&next);
            state.set(next);
            draft.set(String::new());
        }
    };

    let add_item = {
        let submit = submit.clone();
        Callback::from(move |_: MouseEvent| submit())
    };

    let on_keypress = {
        let submit = submit.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Enter" {
                submit();
            }
        })
    };

    let toggle_item = {
        let state = state.clone();
        Callback::from(move |id: uuid::Uuid| {
            let mut next: ListState = (*state).clone();
            if let Some(item) = next.items.iter_mut().find(|i| i.id == id) {
                item.done = !item.done;
            }
            storage::save(&next);
            state.set(next);
        })
    };

    let remove_item = {
        let state = state.clone();
        Callback::from(move |id: uuid::Uuid| {
            let mut next: ListState = (*state).clone();
            next.items.retain(|i| i.id != id);
            storage::save(&next);
            state.set(next);
        })
    };

    html! {
        <div class="app">
            <header>
                <h1>{ "Lister" }</h1>
                <p class="subtitle">{ "A tiny offline-first list keeper" }</p>
            </header>
            <div class="composer">
                <input
                    type="text"
                    placeholder="Add an item..."
                    value={(*draft).clone()}
                    oninput={on_input}
                    onkeypress={on_keypress}
                />
                <button onclick={add_item}>{ "Add" }</button>
            </div>
            <ul class="items">
                { for state.items.iter().map(|item| {
                    let id = item.id;
                    let on_toggle = toggle_item.clone();
                    let on_remove = remove_item.clone();
                    html! {
                        <li class={if item.done { "item done" } else { "item" }} key={id.to_string()}>
                            <span onclick={move |_| on_toggle.emit(id)}>{ &item.text }</span>
                            <button class="remove" onclick={move |_| on_remove.emit(id)}>{ "x" }</button>
                        </li>
                    }
                }) }
            </ul>
        </div>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
