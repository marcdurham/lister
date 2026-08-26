mod auth;
mod components;
mod model;
mod state;
mod store;
mod sync;

use components::{AuthScreen, Breadcrumbs, Composer, ItemEditor, ItemRow, TrashView};
use gloo_events::EventListener;
use gloo_timers::future::TimeoutFuture;
use state::{Action, AppState};
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

const SYNC_INTERVAL_MS: u32 = 30_000;

#[derive(Clone, PartialEq)]
enum SessionState {
    Loading,
    LoggedOut,
    LoggedIn(auth::User),
}

#[function_component(App)]
fn app() -> Html {
    let state = use_reducer(AppState::load);
    let path = use_state(Vec::<Uuid>::new);
    let show_hidden = use_state(|| false);
    let editing = use_state(|| None::<Uuid>);
    let show_trash = use_state(|| false);
    let session = use_state(|| SessionState::Loading);

    // Resolve the current session once on mount.
    {
        let session = session.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match auth::me().await {
                    Some(user) => session.set(SessionState::LoggedIn(user)),
                    None => session.set(SessionState::LoggedOut),
                }
            });
            || ()
        });
    }

    let on_authed = {
        let session = session.clone();
        let state = state.clone();
        Callback::from(move |user: auth::User| {
            store::clear_all();
            state.dispatch(Action::Reload);
            session.set(SessionState::LoggedIn(user));
            let state = state.clone();
            spawn_local(async move {
                sync::sync_once().await;
                state.dispatch(Action::Reload);
            });
        })
    };

    let logout = {
        let session = session.clone();
        let state = state.clone();
        Callback::from(move |_: MouseEvent| {
            let session = session.clone();
            let state = state.clone();
            spawn_local(async move {
                auth::logout().await;
                store::clear_all();
                state.dispatch(Action::Reload);
                session.set(SessionState::LoggedOut);
            });
        })
    };

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

    let on_edit = {
        let editing = editing.clone();
        Callback::from(move |id: Uuid| editing.set(Some(id)))
    };
    let close_editor = {
        let editing = editing.clone();
        Callback::from(move |_: ()| editing.set(None))
    };

    let toggle_trash = {
        let show_trash = show_trash.clone();
        Callback::from(move |_: MouseEvent| show_trash.set(!*show_trash))
    };
    let close_trash = {
        let show_trash = show_trash.clone();
        Callback::from(move |_: ()| show_trash.set(false))
    };

    let trash_count = state.trashed_items().len();
    let unsynced_count = if state.online { 0 } else { store::unsynced_count() };

    let current_parent = path.last().copied();
    let rows = state.children(current_parent, *show_hidden);
    let ids: Vec<Uuid> = rows.iter().map(|(_, m)| m.id).collect();

    let user = match &*session {
        SessionState::LoggedIn(user) => user.clone(),
        SessionState::Loading => {
            return html! { <div class="app auth-screen"><p>{ "Loading..." }</p></div> };
        }
        SessionState::LoggedOut => {
            return html! { <AuthScreen on_authed={on_authed} /> };
        }
    };

    html! {
        <div class="app">
            <header>
                <h1>{ "Lister" }</h1>
                <div class="header-actions">
                    <button class="trash-btn" onclick={toggle_trash}>
                        { "Trash" }
                        if trash_count > 0 {
                            <span class="child-count">{ trash_count }</span>
                        }
                    </button>
                    <div class={classes!("status", if state.online { "online" } else { "offline" })}>
                        { if state.online { "online" } else { "offline" } }
                        { if !state.online && unsynced_count > 0 {
                            html! { <span class="unsynced-count">{ unsynced_count }</span> }
                        } else {
                            html! {}
                        }}
                        { if state.syncing { " · syncing" } else { "" } }
                    </div>
                    <span class="user-email">{ &user.email }</span>
                    <button class="logout-btn" onclick={logout}>{ "Log out" }</button>
                </div>
            </header>
            if *show_trash {
                <TrashView state={state.clone()} on_close={close_trash} />
            } else {
                <Breadcrumbs state={state.clone()} path={(*path).clone()} on_navigate={on_navigate} />
                <div class="composer-bar">
                    <Composer state={state.clone()} parent={current_parent} />
                </div>
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
                                on_edit={on_edit.clone()}
                            />
                        }
                    }) }
                </ul>
                if rows.is_empty() {
                    <p class="empty">{ "Nothing here yet - add an item above." }</p>
                }
            }
            if let Some(id) = *editing {
                <ItemEditor state={state.clone()} item_id={id} on_close={close_editor} />
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
