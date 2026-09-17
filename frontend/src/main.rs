mod admin;
mod auth;
mod backup;
mod components;
mod google;
mod model;
mod state;
mod store;
mod sync;

use components::{
    AdminPage, AuthScreen, Breadcrumbs, Composer, GoogleImportDialog, ItemEditor, ItemRow,
    ListManager, SettingsMenu, TrashView,
};
use gloo_events::EventListener;
use gloo_timers::future::TimeoutFuture;
use state::{Action, AppState};
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

const SYNC_INTERVAL_MS: u32 = 30_000;

/// Encode a navigation path into the value stored in `history.state`.
fn path_to_js(path: &[Uuid]) -> JsValue {
    let strs: Vec<String> = path.iter().map(|id| id.to_string()).collect();
    JsValue::from_str(&serde_json::to_string(&strs).unwrap_or_default())
}

/// Decode a navigation path back out of `history.state` (or the empty path if absent/invalid).
fn js_to_path(value: JsValue) -> Vec<Uuid> {
    value
        .as_string()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .map(|strs| strs.iter().filter_map(|s| Uuid::parse_str(s).ok()).collect())
        .unwrap_or_default()
}

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
    let editing = use_state(|| None::<(Uuid, Uuid)>);
    let show_trash = use_state(|| false);
    let show_admin = use_state(|| false);
    let managing_lists = use_state(|| None::<Uuid>);
    let show_login = use_state(|| false);
    let session = use_state(|| SessionState::Loading);
    let google_import = use_state(|| None::<Vec<google::ImportedTaskList>>);

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

    // Pick up an in-progress Google Tasks import once we're back from the OAuth redirect.
    {
        let google_import = google_import.clone();
        use_effect_with((), move |_| {
            if google::returned_from_google() {
                google::clear_return_marker();
                spawn_local(async move {
                    let lists = google::fetch_imported_tasks().await;
                    google_import.set(Some(lists));
                });
            }
            || ()
        });
    }

    let on_authed = {
        let session = session.clone();
        let state = state.clone();
        let show_login = show_login.clone();
        Callback::from(move |user: auth::User| {
            store::clear_all();
            state.dispatch(Action::Reload);
            session.set(SessionState::LoggedIn(user));
            show_login.set(false);
            let state = state.clone();
            spawn_local(async move {
                sync::sync_once().await;
                state.dispatch(Action::Reload);
            });
        })
    };

    let open_login = {
        let show_login = show_login.clone();
        Callback::from(move |_: MouseEvent| show_login.set(true))
    };
    let close_login = {
        let show_login = show_login.clone();
        Callback::from(move |_: ()| show_login.set(false))
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

    // Push a browser history entry for every in-app navigation, so the browser's own
    // back button walks back up the hierarchy one level at a time.
    let navigate = {
        let path = path.clone();
        Callback::from(move |new_path: Vec<Uuid>| {
            if let Some(window) = web_sys::window() {
                if let Ok(history) = window.history() {
                    let _ = history.push_state_with_url(&path_to_js(&new_path), "", None);
                }
            }
            path.set(new_path);
        })
    };

    // Sync `path` from the browser's session history on back/forward navigation.
    {
        let path = path.clone();
        use_effect_with((), move |_| {
            if let Some(window) = web_sys::window() {
                if let Ok(history) = window.history() {
                    let _ = history.replace_state_with_url(&path_to_js(&[]), "", None);
                }
            }
            let listener = EventListener::new(&web_sys::window().unwrap(), "popstate", move |e| {
                if let Ok(event) = e.clone().dyn_into::<web_sys::PopStateEvent>() {
                    path.set(js_to_path(event.state()));
                }
            });
            move || drop(listener)
        });
    }

    let on_navigate = navigate.clone();

    let on_open = {
        let path = path.clone();
        let navigate = navigate.clone();
        Callback::from(move |id: Uuid| {
            let mut next = (*path).clone();
            next.push(id);
            navigate.emit(next);
        })
    };

    let on_go_up = {
        let path = path.clone();
        let navigate = navigate.clone();
        Callback::from(move |_: MouseEvent| {
            let mut next = (*path).clone();
            next.pop();
            navigate.emit(next);
        })
    };

    let toggle_hidden = {
        let show_hidden = show_hidden.clone();
        Callback::from(move |_: MouseEvent| show_hidden.set(!*show_hidden))
    };

    let on_edit = {
        let editing = editing.clone();
        Callback::from(move |ids: (Uuid, Uuid)| editing.set(Some(ids)))
    };
    let close_editor = {
        let editing = editing.clone();
        Callback::from(move |_: ()| editing.set(None))
    };

    let on_manage_lists = {
        let editing = editing.clone();
        let managing_lists = managing_lists.clone();
        Callback::from(move |item_id: Uuid| {
            editing.set(None);
            managing_lists.set(Some(item_id));
        })
    };
    let close_list_manager = {
        let managing_lists = managing_lists.clone();
        Callback::from(move |_: ()| managing_lists.set(None))
    };

    let toggle_trash = {
        let show_trash = show_trash.clone();
        Callback::from(move |_: MouseEvent| show_trash.set(!*show_trash))
    };
    let close_trash = {
        let show_trash = show_trash.clone();
        Callback::from(move |_: ()| show_trash.set(false))
    };

    let open_admin = {
        let show_admin = show_admin.clone();
        Callback::from(move |_: ()| show_admin.set(true))
    };
    let close_admin = {
        let show_admin = show_admin.clone();
        Callback::from(move |_: ()| show_admin.set(false))
    };

    let close_google_import = {
        let google_import = google_import.clone();
        Callback::from(move |_: ()| google_import.set(None))
    };

    let trash_count = state.trashed_items().len();
    let unsynced_count = if state.online { 0 } else { store::unsynced_count() };

    let current_parent = path.last().copied();
    let rows = state.children(current_parent, *show_hidden);

    if *session == SessionState::Loading {
        return html! { <div class="app auth-screen"><p>{ "Loading..." }</p></div> };
    }
    let user = match &*session {
        SessionState::LoggedIn(user) => Some(user.clone()),
        _ => None,
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
                    if let Some(user) = &user {
                        <span class="user-email">{ &user.email }</span>
                        <SettingsMenu state={state.clone()} is_admin={user.is_admin} on_open_admin={open_admin} />
                        <button class="logout-btn" onclick={logout}>{ "Log out" }</button>
                    } else {
                        <button class="login-btn" onclick={open_login}>{ "Log in" }</button>
                    }
                </div>
            </header>
            if let Some(lists) = (*google_import).clone() {
                <GoogleImportDialog state={state.clone()} lists={lists} on_close={close_google_import} />
            }
            if *show_login {
                <AuthScreen on_authed={on_authed} on_close={Some(close_login)} />
            } else if let Some(item_id) = *managing_lists {
                <ListManager state={state.clone()} item_id={item_id} on_close={close_list_manager} />
            } else if *show_admin && user.is_some() {
                <AdminPage current_user_id={user.as_ref().unwrap().id.clone()} on_close={close_admin} />
            } else if *show_trash {
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
                    if current_parent.is_some() {
                        <li class="item up-item" onclick={on_go_up}>
                            <div class="item-main">
                                <span class="item-text">{ ".." }</span>
                            </div>
                        </li>
                    }
                    { for rows.iter().map(|(item, membership)| {
                        html! {
                            <ItemRow
                                key={membership.id.to_string()}
                                state={state.clone()}
                                item={item.clone()}
                                membership={membership.clone()}
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
            if let Some((item_id, membership_id)) = *editing {
                <ItemEditor
                    state={state.clone()}
                    item_id={item_id}
                    membership_id={membership_id}
                    on_close={close_editor}
                    on_manage_lists={on_manage_lists}
                />
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
