mod components;
mod model;
mod state;
mod store;
mod sync;

use components::{Breadcrumbs, Composer, ItemRow};
use gloo_events::EventListener;
use gloo_timers::future::TimeoutFuture;
use state::{Action, AppState};
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

const SYNC_INTERVAL_MS: u32 = 30_000;

#[function_component(App)]
fn app() -> Html {
    let state = use_reducer(AppState::load);
    let path = use_state(Vec::<Uuid>::new);
    let show_hidden = use_state(|| false);

    // Online/offline listeners + periodic sync loop.
    {
        let state = state.clone();
        use_effect_with((), move |_| {
            let online_listener = {
                let state = state.clone();
                EventListener::new(&web_sys::window().unwrap(), "online", move |_| {
                    state.dispatch(Action::SetOnline(true));
                })
            };
            let offline_listener = {
                let state = state.clone();
                EventListener::new(&web_sys::window().unwrap(), "offline", move |_| {
                    state.dispatch(Action::SetOnline(false));
                })
            };

            spawn_local(sync_loop(state.clone()));

            move || {
                drop(online_listener);
                drop(offline_listener);
            }
        });
    }

    let on_navigate = {
        let path = path.clone();
        Callback::from(move |new_path: Vec<Uuid>| path.set(new_path))
    };

    let on_open = {
        let path = path.clone();
        Callback::from(move |id: Uuid| {
            let mut next = (*path).clone();
            next.push(id);
            path.set(next);
        })
    };

    let toggle_hidden = {
        let show_hidden = show_hidden.clone();
        Callback::from(move |_: MouseEvent| show_hidden.set(!*show_hidden))
    };

    let current_parent = path.last().copied();
    let rows = state.children(current_parent, *show_hidden);
    let ids: Vec<Uuid> = rows.iter().map(|(_, m)| m.id).collect();

    html! {
        <div class="app">
            <header>
                <h1>{ "Lister" }</h1>
                <div class={classes!("status", if state.online { "online" } else { "offline" })}>
                    { if state.online { "online" } else { "offline" } }
                    { if state.syncing { " · syncing" } else { "" } }
                </div>
            </header>
            <Breadcrumbs state={state.clone()} path={(*path).clone()} on_navigate={on_navigate} />
            <Composer state={state.clone()} parent={current_parent} />
            <label class="show-hidden">
                <input type="checkbox" checked={*show_hidden} onclick={toggle_hidden} />
                { " Show hidden items" }
            </label>
            <ul class="items">
                { for rows.iter().enumerate().map(|(idx, (item, membership))| {
                    let prev_id = (idx > 0).then(|| ids[idx - 1]);
                    let next_id = (idx + 1 < ids.len()).then(|| ids[idx + 1]);
                    html! {
                        <ItemRow
                            key={membership.id.to_string()}
                            state={state.clone()}
                            item={item.clone()}
                            membership={membership.clone()}
                            is_first={idx == 0}
                            is_last={idx + 1 == ids.len()}
                            prev_id={prev_id}
                            next_id={next_id}
                            on_open={on_open.clone()}
                        />
                    }
                }) }
            </ul>
            if rows.is_empty() {
                <p class="empty">{ "Nothing here yet - add an item above." }</p>
            }
        </div>
    }
}

async fn sync_loop(state: UseReducerHandle<AppState>) {
    loop {
        if sync::is_online() {
            state.dispatch(Action::SetSyncing(true));
            sync::sync_once().await;
            state.dispatch(Action::SetSyncing(false));
            state.dispatch(Action::Reload);
        }
        TimeoutFuture::new(SYNC_INTERVAL_MS).await;
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
