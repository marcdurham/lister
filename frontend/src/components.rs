use crate::admin;
use crate::auth;
use crate::backup;
use crate::google::{self, ImportedTaskList};
use crate::markdown;
use crate::model::{Item, Membership};
use crate::state::{Action, AppState};
use gloo_timers::callback::{Interval, Timeout};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use uuid::Uuid;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{FileReader, HtmlInputElement, PointerEvent};
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

pub(crate) fn trash_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
            <path d="M2.5 4.3h11" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" fill="none"/>
            <path d="M6 4.3V2.9a0.9 0.9 0 0 1 0.9-0.9h2.2a0.9 0.9 0 0 1 0.9 0.9v1.4" stroke="currentColor" stroke-width="1.3" fill="none"/>
            <path d="M4.3 4.3l0.6 8.8a1 1 0 0 0 1 0.9h4.2a1 1 0 0 0 1-0.9l0.6-8.8" stroke="currentColor" stroke-width="1.3" fill="none" stroke-linejoin="round"/>
            <line x1="6.5" y1="6.7" x2="6.8" y2="11.5" stroke="currentColor" stroke-width="1"/>
            <line x1="9.5" y1="6.7" x2="9.2" y2="11.5" stroke="currentColor" stroke-width="1"/>
        </svg>
    }
}

/// Small calendar glyph shown on a row/list header when the item has a due, show, or
/// hide date set.
fn calendar_icon() -> Html {
    html! {
        <svg class="calendar-icon" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <rect x="2" y="3" width="12" height="11" rx="1.3" fill="none" stroke="currentColor" stroke-width="1.2"/>
            <line x1="2" y1="6.3" x2="14" y2="6.3" stroke="currentColor" stroke-width="1.2"/>
            <line x1="5" y1="1.5" x2="5" y2="4.3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            <line x1="11" y1="1.5" x2="11" y2="4.3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
        </svg>
    }
}

fn menu_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
            <line x1="2" y1="4" x2="14" y2="4" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
            <line x1="2" y1="8" x2="14" y2="8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
            <line x1="2" y1="12" x2="14" y2="12" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
        </svg>
    }
}

fn edit_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <circle cx="8" cy="3" r="1.3" fill="currentColor"/>
            <circle cx="8" cy="8" r="1.3" fill="currentColor"/>
            <circle cx="8" cy="13" r="1.3" fill="currentColor"/>
        </svg>
    }
}

/// Larger right-pointing chevron shown in place of the edit button while a drag is in
/// progress - dropping another item onto it nests the dragged item under this one.
fn chevron_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="20" height="20" aria-hidden="true">
            <polyline points="5,2 12,8 5,14" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
    }
}

fn search_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
            <circle cx="6.8" cy="6.8" r="4.3" fill="none" stroke="currentColor" stroke-width="1.4"/>
            <line x1="10" y1="10" x2="14" y2="14" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
        </svg>
    }
}

fn plus_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
            <line x1="8" y1="2" x2="8" y2="14" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
            <line x1="2" y1="8" x2="14" y2="8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
        </svg>
    }
}

/// Person glyph used on the combined session button; filled when someone is logged in.
fn person_icon(filled: bool) -> Html {
    if filled {
        html! {
            <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
                <circle cx="8" cy="5" r="3" fill="currentColor"/>
                <path d="M2 14c0-3.3 2.7-6 6-6s6 2.7 6 6" fill="currentColor"/>
            </svg>
        }
    } else {
        html! {
            <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
                <circle cx="8" cy="5" r="3" fill="none" stroke="currentColor" stroke-width="1.3"/>
                <path d="M2 14c0-3.3 2.7-6 6-6s6 2.7 6 6" fill="none" stroke="currentColor" stroke-width="1.3"/>
            </svg>
        }
    }
}

/// Floppy-disk glyph shown on the session button when this browser has never logged in,
/// in place of the online/offline person icon - the connection state doesn't mean much
/// for a guest who only has locally cached data.
fn disk_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
            <path
                d="M2.5 2h9l2 2v9a0.5 0.5 0 0 1-0.5 0.5h-10.5a0.5 0.5 0 0 1-0.5-0.5v-10.5a0.5 0.5 0 0 1 0.5-0.5z"
                fill="none" stroke="currentColor" stroke-width="1.2"
            />
            <rect x="4.3" y="2" width="5.4" height="3.4" fill="none" stroke="currentColor" stroke-width="1.1"/>
            <rect x="4" y="8.3" width="8" height="5.2" fill="currentColor"/>
        </svg>
    }
}

/// Small clipboard glyph for the compact copy button next to the item editor's Text field.
fn copy_icon() -> Html {
    html! {
        <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
            <rect x="5.5" y="5.5" width="8" height="9" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/>
            <path d="M3.5 10.5v-7a1 1 0 0 1 1-1h6" fill="none" stroke="currentColor" stroke-width="1.2"/>
        </svg>
    }
}

fn format_ts(ts: &chrono::DateTime<chrono::Utc>) -> String {
    ts.format("%Y-%m-%d %H:%M").to_string()
}

/// Formats a `DateTime<Utc>` for a `<input type="datetime-local">` value (local time,
/// no timezone/seconds), and parses one back. Both directions go through the JS `Date`
/// object so the conversion follows the browser's own local timezone.
fn to_datetime_local_value(dt: chrono::DateTime<chrono::Utc>) -> String {
    let js_date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(dt.timestamp_millis() as f64));
    let pad = |n: i32| format!("{n:02}");
    format!(
        "{}-{}-{}T{}:{}",
        js_date.get_full_year(),
        pad(js_date.get_month() as i32 + 1),
        pad(js_date.get_date() as i32),
        pad(js_date.get_hours() as i32),
        pad(js_date.get_minutes() as i32),
    )
}

fn from_datetime_local_value(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if value.trim().is_empty() {
        return None;
    }
    let js_date = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(value));
    let millis = js_date.get_time();
    if millis.is_nan() {
        return None;
    }
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis as i64)
}

/// Closes `open` on the next mousedown/touchstart that lands outside `node_ref`'s
/// element - lets any dropdown menu dismiss itself when the user clicks elsewhere,
/// instead of staying open until something explicitly closes it.
#[hook]
fn use_click_outside(node_ref: NodeRef, open: UseStateHandle<bool>) {
    use_effect_with(*open, move |is_open| {
        if !*is_open {
            return Box::new(|| ()) as Box<dyn FnOnce()>;
        }
        let node_ref = node_ref.clone();
        let open = open.clone();
        let listener = gloo_events::EventListener::new(
            &web_sys::window().unwrap(),
            "mousedown",
            move |e: &web_sys::Event| {
                let Some(target) = e.target() else { return };
                let Ok(target_node) = target.dyn_into::<web_sys::Node>() else { return };
                let inside = node_ref
                    .get()
                    .map(|el| el.contains(Some(&target_node)))
                    .unwrap_or(false);
                if !inside {
                    open.set(false);
                }
            },
        );
        Box::new(move || drop(listener)) as Box<dyn FnOnce()>
    });
}

#[derive(Properties, PartialEq)]
pub struct AuthScreenProps {
    pub on_authed: Callback<auth::User>,
    /// Present when the app is already usable offline as a guest, so the screen is a
    /// dismissible panel rather than the only thing on screen.
    pub on_close: Option<Callback<()>>,
}

#[function_component(AuthScreen)]
pub fn auth_screen(props: &AuthScreenProps) -> Html {
    let mode_register = use_state(|| false);
    let email = use_state(String::new);
    let password = use_state(String::new);
    let error = use_state(|| None::<String>);
    let info = use_state(|| None::<String>);
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
        let info = info.clone();
        Callback::from(move |_: MouseEvent| {
            mode_register.set(!*mode_register);
            error.set(None);
            info.set(None);
        })
    };

    let submit = {
        let email = email.clone();
        let password = password.clone();
        let mode_register = mode_register.clone();
        let error = error.clone();
        let info = info.clone();
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
            let info = info.clone();
            let busy = busy.clone();
            let on_authed = on_authed.clone();
            busy.set(true);
            spawn_local(async move {
                error.set(None);
                info.set(None);
                if is_register {
                    match auth::register(&email_v, &password_v).await {
                        Ok(auth::RegisterOutcome::LoggedIn(user)) => on_authed.emit(user),
                        Ok(auth::RegisterOutcome::Pending) => info.set(Some(
                            "Account created. An administrator needs to approve it before you can sign in.".to_string(),
                        )),
                        Err(msg) => error.set(Some(msg)),
                    }
                } else {
                    match auth::login(&email_v, &password_v).await {
                        Ok(user) => on_authed.emit(user),
                        Err(msg) => error.set(Some(msg)),
                    }
                }
                busy.set(false);
            });
        })
    };

    let close = props.on_close.clone().map(|on_close| {
        Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            on_close.emit(());
        })
    });

    html! {
        <div class="auth-screen">
            <form class="auth-card">
                if let Some(close) = close {
                    <button class="auth-back" type="button" onclick={close}>{ "Back" }</button>
                }
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
                if let Some(msg) = &*info {
                    <p class="auth-info">{ msg }</p>
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

#[derive(Clone, Copy, PartialEq)]
enum ItemKind {
    Note,
    Task,
    List,
}

impl ItemKind {
    fn as_str(self) -> &'static str {
        match self {
            ItemKind::Note => "note",
            ItemKind::Task => "task",
            ItemKind::List => "list",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "task" => ItemKind::Task,
            "list" => ItemKind::List,
            _ => ItemKind::Note,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ShowMode {
    Always,
    BeforeDue,
    Fixed,
}

impl ShowMode {
    fn as_str(self) -> &'static str {
        match self {
            ShowMode::Always => "always",
            ShowMode::BeforeDue => "before_due",
            ShowMode::Fixed => "fixed",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "before_due" => ShowMode::BeforeDue,
            "fixed" => ShowMode::Fixed,
            _ => ShowMode::Always,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum HideMode {
    Never,
    AfterCreated,
    Fixed,
}

impl HideMode {
    fn as_str(self) -> &'static str {
        match self {
            HideMode::Never => "never",
            HideMode::AfterCreated => "after_created",
            HideMode::Fixed => "fixed",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "after_created" => HideMode::AfterCreated,
            "fixed" => HideMode::Fixed,
            _ => HideMode::Never,
        }
    }
}

/// `<select>` of the time units usable for a show/hide offset or a recurrence interval.
fn time_unit_select(value: &str, onchange: Callback<Event>) -> Html {
    html! {
        <select class="unit-select" {onchange} value={value.to_string()}>
            { for shared::TIME_UNITS.iter().map(|unit| {
                html! { <option value={*unit} selected={value == *unit}>{ *unit }</option> }
            }) }
        </select>
    }
}

#[derive(Properties, PartialEq)]
pub struct SessionButtonProps {
    pub user: Option<auth::User>,
    pub online: bool,
    /// True once this browser has completed a login/register at least once. While false,
    /// the button shows a neutral "cached in browser" state instead of online/offline,
    /// since network status isn't meaningful for a guest who's never signed in.
    pub has_logged_in: bool,
    pub on_login: Callback<()>,
    pub on_logout: Callback<()>,
}

/// Combined login/online control: a single icon button whose ring shows online (green)
/// vs offline (yellow), and whose glyph shows whether anyone is logged in. Clicking it
/// opens a small menu with the status text, the account email, and a login/logout action.
/// Before the first-ever login on this browser, the ring is blue, the glyph is a disk
/// (this device's cached copy, not a live connection), and the status line reads "Cached
/// in browser" - logging in is still offered from the same menu.
#[function_component(SessionButton)]
pub fn session_button(props: &SessionButtonProps) -> Html {
    let open = use_state(|| false);
    let root_ref = use_node_ref();
    use_click_outside(root_ref.clone(), open.clone());
    let never_logged_in = !props.has_logged_in && props.user.is_none();

    let toggle = {
        let open = open.clone();
        Callback::from(move |_: MouseEvent| open.set(!*open))
    };

    let login = {
        let open = open.clone();
        let on_login = props.on_login.clone();
        Callback::from(move |_: MouseEvent| {
            open.set(false);
            on_login.emit(());
        })
    };

    let logout = {
        let open = open.clone();
        let on_logout = props.on_logout.clone();
        Callback::from(move |_: MouseEvent| {
            open.set(false);
            on_logout.emit(());
        })
    };

    let btn_class = classes!(
        "session-btn",
        if never_logged_in {
            "cached"
        } else if props.online {
            "online"
        } else {
            "offline"
        }
    );

    let title = if never_logged_in {
        "Cached in browser"
    } else if props.online {
        "Online"
    } else {
        "Offline"
    };

    html! {
        <div class="session-menu" ref={root_ref}>
            <button
                class={btn_class}
                onclick={toggle}
                {title}
                aria-label="Account and connection status"
            >
                if never_logged_in {
                    { disk_icon() }
                } else {
                    { person_icon(props.user.is_some()) }
                }
            </button>
            if *open {
                <div class="settings-dropdown session-dropdown">
                    <div class="session-status-line">
                        <span class={classes!("status-dot", if never_logged_in { "cached" } else if props.online { "online" } else { "offline" })}></span>
                        { title }
                    </div>
                    if let Some(user) = &props.user {
                        <div class="session-email">{ &user.email }</div>
                        <button class="settings-item" onclick={logout}>{ "Log out" }</button>
                    } else {
                        <button class="settings-item" onclick={login}>{ "Log in" }</button>
                    }
                </div>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct ComposerProps {
    pub state: UseReducerHandle<AppState>,
    pub parent: Option<Uuid>,
    /// Live search query for the current list; `None` while the composer is in add mode.
    pub search: UseStateHandle<Option<String>>,
}

#[function_component(Composer)]
pub fn composer(props: &ComposerProps) -> Html {
    let draft = use_state(String::new);
    // Notes are the default item type.
    let kind = use_state(|| ItemKind::Note);
    let search = props.search.clone();
    let searching = search.is_some();

    let on_input = {
        let draft = draft.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            draft.set(input.value());
        })
    };

    let on_search_input = {
        let search = search.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            search.set(Some(input.value()));
        })
    };

    let submit = {
        let draft = draft.clone();
        let kind = kind.clone();
        let state = props.state.clone();
        let parent = props.parent;
        move || {
            let text = draft.trim().to_string();
            if text.is_empty() {
                return;
            }
            state.dispatch(Action::AddItem {
                text,
                is_note: *kind == ItemKind::Note,
                is_list: *kind == ItemKind::List,
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
    let on_kind_change = {
        let kind = kind.clone();
        Callback::from(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            kind.set(ItemKind::from_str(&select.value()));
        })
    };

    let enter_search = {
        let search = search.clone();
        Callback::from(move |_: MouseEvent| search.set(Some(String::new())))
    };
    let exit_search = {
        let search = search.clone();
        Callback::from(move |_: MouseEvent| search.set(None))
    };

    if searching {
        let query = (*search).clone().unwrap_or_default();
        html! {
            <div class="composer composer-search">
                <button class="composer-mode-btn" onclick={exit_search} title="Back to add">
                    { plus_icon() }
                </button>
                <input
                    type="text"
                    placeholder="Search this list..."
                    value={query}
                    oninput={on_search_input}
                />
            </div>
        }
    } else {
        html! {
            <div class="composer">
                <button class="composer-mode-btn" onclick={enter_search} title="Search this list">
                    { search_icon() }
                </button>
                <input
                    type="text"
                    placeholder={match *kind {
                        ItemKind::Note => "Add a note...",
                        ItemKind::Task => "Add a task...",
                        ItemKind::List => "Add a list...",
                    }}
                    value={(*draft).clone()}
                    oninput={on_input}
                    {onkeypress}
                />
                <select class="composer-kind" onchange={on_kind_change} value={kind.as_str()}>
                    <option value="note" selected={*kind == ItemKind::Note}>{ "Note" }</option>
                    <option value="task" selected={*kind == ItemKind::Task}>{ "Task" }</option>
                    <option value="list" selected={*kind == ItemKind::List}>{ "List" }</option>
                </select>
                <button {onclick}>{ "Add" }</button>
            </div>
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct ListHeaderProps {
    pub item: Item,
    pub child_count: usize,
    pub on_edit: Callback<()>,
}

/// Shown above the composer bar while viewing a list: the list's own title and notes,
/// each clamped to two lines with a click anywhere on the text expanding it in place,
/// plus an edit button (identical to a row's) that opens the full item editor.
#[function_component(ListHeader)]
pub fn list_header(props: &ListHeaderProps) -> Html {
    let text_expanded = use_state(|| false);
    let notes_expanded = use_state(|| false);

    let toggle_text = {
        let text_expanded = text_expanded.clone();
        Callback::from(move |_: MouseEvent| text_expanded.set(!*text_expanded))
    };
    let toggle_notes = {
        let notes_expanded = notes_expanded.clone();
        Callback::from(move |_: MouseEvent| notes_expanded.set(!*notes_expanded))
    };
    let edit = {
        let on_edit = props.on_edit.clone();
        Callback::from(move |_: MouseEvent| on_edit.emit(()))
    };

    let notes = props
        .item
        .notes
        .as_deref()
        .map(|n| n.trim())
        .filter(|n| !n.is_empty());

    let has_dates = props.item.due_at.is_some()
        || props.item.show_after.is_some()
        || props.item.hide_after.is_some();

    html! {
        <div class="list-header">
            <div class="list-header-row">
                <span
                    class={classes!("list-header-text", (*text_expanded).then_some("expanded"))}
                    onclick={toggle_text}
                >
                    { &props.item.text }
                </span>
                if props.child_count > 0 {
                    <span class="child-count list-header-count">{ props.child_count }</span>
                }
                if has_dates {
                    <span class="dates-icon" title="Has scheduled dates">
                        { calendar_icon() }
                    </span>
                }
                <button class="edit-btn" onclick={edit} title="Edit" aria-label="Edit">
                    { edit_icon() }
                </button>
            </div>
            if let Some(notes) = notes {
                <p
                    class={classes!("list-header-notes", (*notes_expanded).then_some("expanded"))}
                    onclick={toggle_notes}
                >
                    { notes }
                </p>
            }
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

/// What's currently under the pointer while dragging an item: another row (to reorder
/// before), that row's chevron (to nest inside it), or the gap after the last row (to
/// move to the end of the list).
#[derive(Clone, Copy, PartialEq)]
pub enum DragHoverTarget {
    Reorder(Uuid),
    Nest(Uuid),
    End,
}

/// Distance (px) from the top/bottom of the viewport within which an active drag
/// auto-scrolls the page.
const AUTOSCROLL_EDGE: f64 = 70.0;
/// Fastest the page auto-scrolls (px per tick) right at the very edge of the viewport.
const AUTOSCROLL_MAX_SPEED: f64 = 16.0;

/// How fast (and which direction) to auto-scroll for a drag pointer currently at
/// viewport-relative `y`: negative near the top, positive near the bottom, zero
/// everywhere else.
fn autoscroll_speed(y: f64) -> f64 {
    let Some(height) = web_sys::window()
        .and_then(|w| w.inner_height().ok())
        .and_then(|v| v.as_f64())
    else {
        return 0.0;
    };
    if y < AUTOSCROLL_EDGE {
        -((AUTOSCROLL_EDGE - y).max(0.0) / AUTOSCROLL_EDGE) * AUTOSCROLL_MAX_SPEED
    } else if y > height - AUTOSCROLL_EDGE {
        ((y - (height - AUTOSCROLL_EDGE)).max(0.0) / AUTOSCROLL_EDGE) * AUTOSCROLL_MAX_SPEED
    } else {
        0.0
    }
}

/// Minimum on-screen movement (px) before a mouse press turns into a drag.
const MOUSE_DRAG_THRESHOLD: f64 = 6.0;
/// How long a touch/pen must be held still before it arms into a drag.
const TOUCH_HOLD_MS: u32 = 280;
/// Movement (px) during the hold window that means "this is a scroll, not a hold".
const TOUCH_CANCEL_THRESHOLD: f64 = 10.0;

#[derive(Clone)]
struct DragTracker {
    pointer_id: i32,
    start_x: f64,
    start_y: f64,
    /// Y of the last pointermove seen, used to compute a manual scroll delta.
    last_y: f64,
    is_touch: bool,
    /// The hold elapsed and this is now a real drag.
    active: bool,
    /// The touch moved before the hold elapsed, so we're manually scrolling the page
    /// instead (native touch scrolling is disabled on the row so the hold can be timed).
    scrolling: bool,
}

/// Find whatever row (or nest chevron) is visually under the given viewport point.
fn hover_target_at(x: f64, y: f64, dragged_membership_id: Uuid) -> Option<DragHoverTarget> {
    let doc = web_sys::window()?.document()?;
    let el = doc.element_from_point(x as f32, y as f32)?;
    if let Ok(Some(nest)) = el.closest(".nest-target") {
        let item_id = nest.get_attribute("data-drop-item")?;
        return Uuid::parse_str(&item_id).ok().map(DragHoverTarget::Nest);
    }
    if let Ok(Some(_)) = el.closest(".drop-gap") {
        return Some(DragHoverTarget::End);
    }
    if let Ok(Some(row)) = el.closest("[data-membership-id]") {
        let membership_id = row.get_attribute("data-membership-id")?;
        let membership_id = Uuid::parse_str(&membership_id).ok()?;
        if membership_id != dragged_membership_id {
            return Some(DragHoverTarget::Reorder(membership_id));
        }
    }
    None
}

/// Reorders `rows` for live drag preview: pulls the dragged row (and, if it's a depth-0
/// parent whose deeper descendants are also being shown, its subtree) out of its current
/// slot and reinserts it just before `before_membership_id`, or at the very end when
/// `None`. Purely a display-order computation - the real move only happens on drop.
pub fn reorder_preview_rows(
    rows: &[(Item, Membership, usize)],
    dragged_membership_id: Uuid,
    before_membership_id: Option<Uuid>,
) -> Vec<(Item, Membership, usize)> {
    if before_membership_id == Some(dragged_membership_id) {
        return rows.to_vec();
    }
    let Some(drag_idx) = rows.iter().position(|(_, m, _)| m.id == dragged_membership_id) else {
        return rows.to_vec();
    };
    let drag_depth = rows[drag_idx].2;
    let mut end = drag_idx + 1;
    while end < rows.len() && rows[end].2 > drag_depth {
        end += 1;
    }
    let mut result = rows.to_vec();
    let block: Vec<_> = result.drain(drag_idx..end).collect();
    let insert_at = match before_membership_id {
        Some(before_id) => result
            .iter()
            .position(|(_, m, _)| m.id == before_id)
            .unwrap_or(result.len()),
        None => result.len(),
    };
    result.splice(insert_at..insert_at, block);
    result
}

#[derive(Properties, PartialEq)]
pub struct ItemRowProps {
    pub state: UseReducerHandle<AppState>,
    pub item: Item,
    pub membership: Membership,
    pub on_open: Callback<Uuid>,
    pub on_edit: Callback<(Uuid, Uuid)>,
    /// Membership id of the item currently being dragged, shared across all rows so each
    /// one can switch its far-right control to a "drop to nest" chevron.
    pub dragging: Option<Uuid>,
    /// Current nest drop target, shared across all rows so the hovered chevron can
    /// highlight - reorder targets don't need this since the list is redrawn with the
    /// dragged row already in its prospective position instead.
    pub hover_nest_item: Option<Uuid>,
    pub on_drag_start: Callback<Uuid>,
    pub on_drag_hover: Callback<Option<DragHoverTarget>>,
    pub on_drag_end: Callback<()>,
    /// Nesting depth relative to the list currently being viewed: 0 for a direct child
    /// (the normal case), or more when the list has "show nested children" on and this
    /// row is a deeper descendant. Deeper rows are indented and, since dropping onto them
    /// for reorder/nest would silently misplace an item relative to its real parent,
    /// aren't drag sources or drop targets.
    #[prop_or(0)]
    pub depth: usize,
}

#[function_component(ItemRow)]
pub fn item_row(props: &ItemRowProps) -> Html {
    // A plain `use_state` handle snapshots its value per-render, so a `Timeout` closure
    // created in one render (as the touch hold-to-arm timer is) would only ever see the
    // value from that render, never a later update - `use_mut_ref` gives real shared
    // mutable state instead, which every closure (however long-lived) reads live.
    let tracker: Rc<RefCell<Option<DragTracker>>> = use_mut_ref(|| None);
    let hold_timer: Rc<RefCell<Option<Timeout>>> = use_mut_ref(|| None);
    // Runs while an active drag's pointer sits near the top/bottom of the viewport, to
    // keep scrolling the page so the item can be dragged further than one screenful.
    let autoscroll_timer: Rc<RefCell<Option<Interval>>> = use_mut_ref(|| None);
    let autoscroll_y: Rc<RefCell<f64>> = use_mut_ref(|| 0.0);
    // Set right before a real (moved) drag ends, so the "click" event the browser still
    // fires right after pointerup doesn't also open/edit the row that was just dropped.
    let suppress_click: Rc<RefCell<bool>> = use_mut_ref(|| false);

    let item = &props.item;
    let membership = &props.membership;
    let state = props.state.clone();
    let child_count = state.direct_child_count(item.id);
    let is_list = !item.is_note && (item.is_list || child_count > 0);
    let is_dragging_this = props.dragging == Some(membership.id);
    let drag_active = props.dragging.is_some();
    let has_dates = item.due_at.is_some() || item.show_after.is_some() || item.hide_after.is_some();

    let toggle_done = {
        let state = state.clone();
        let id = item.id;
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            state.dispatch(Action::ToggleDone(id));
        })
    };

    // Tapping the row opens the sub-list if this item is (or acts as) a list, otherwise
    // edits it - unless the tap is actually the tail end of a drag (the browser still
    // fires a click right after pointerup), in which case it's swallowed instead.
    let open_or_edit = {
        let on_open = props.on_open.clone();
        let on_edit = props.on_edit.clone();
        let suppress_click = suppress_click.clone();
        let id = item.id;
        let membership_id = membership.id;
        let navigate_in = is_list;
        Callback::from(move |_: MouseEvent| {
            if *suppress_click.borrow() {
                *suppress_click.borrow_mut() = false;
                return;
            }
            if navigate_in {
                on_open.emit(id);
            } else {
                on_edit.emit((id, membership_id));
            }
        })
    };

    let edit = {
        let on_edit = props.on_edit.clone();
        let id = item.id;
        let membership_id = membership.id;
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            on_edit.emit((id, membership_id));
        })
    };

    // Drag-to-reorder/nest: press anywhere on the row (other than the checkbox/buttons)
    // and hold. A mouse starts dragging as soon as it moves past a small threshold. A
    // touch/pen needs a brief hold first - native touch scrolling is disabled on the row
    // (see `.item-main { touch-action: none }`) so we can time that hold, and if the
    // finger moves before it elapses we scroll the page manually instead, so scrolling a
    // list that starts on an item still works.
    let interactive_drag = props.depth == 0;
    let on_pointer_down = {
        let tracker = tracker.clone();
        let hold_timer = hold_timer.clone();
        let on_drag_start = props.on_drag_start.clone();
        let membership_id = membership.id;
        Callback::from(move |e: PointerEvent| {
            if !interactive_drag || e.button() != 0 {
                return;
            }
            let is_touch = e.pointer_type() == "touch" || e.pointer_type() == "pen";
            let pointer_id = e.pointer_id();
            let start_x = e.client_x() as f64;
            let start_y = e.client_y() as f64;
            *tracker.borrow_mut() = Some(DragTracker {
                pointer_id,
                start_x,
                start_y,
                last_y: start_y,
                is_touch,
                active: false,
                scrolling: false,
            });

            if let Some(target) = e.target() {
                if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                    if let Ok(Some(item_main)) = el.closest(".item-main") {
                        let _ = item_main.set_pointer_capture(pointer_id);
                    }
                }
            }

            if is_touch {
                let tracker2 = tracker.clone();
                let on_drag_start2 = on_drag_start.clone();
                let timeout = Timeout::new(TOUCH_HOLD_MS, move || {
                    let mut armed = false;
                    if let Some(t) = tracker2.borrow_mut().as_mut() {
                        if t.pointer_id == pointer_id && !t.active && !t.scrolling {
                            t.active = true;
                            armed = true;
                        }
                    }
                    if armed {
                        on_drag_start2.emit(membership_id);
                    }
                });
                *hold_timer.borrow_mut() = Some(timeout);
            }
        })
    };

    let on_pointer_move = {
        let tracker = tracker.clone();
        let hold_timer = hold_timer.clone();
        let autoscroll_timer = autoscroll_timer.clone();
        let autoscroll_y = autoscroll_y.clone();
        let on_drag_start = props.on_drag_start.clone();
        let on_drag_hover = props.on_drag_hover.clone();
        let membership_id = membership.id;
        Callback::from(move |e: PointerEvent| {
            let Some(mut t) = tracker.borrow().clone() else {
                return;
            };
            if t.pointer_id != e.pointer_id() {
                return;
            }
            let x = e.client_x() as f64;
            let y = e.client_y() as f64;

            if t.active {
                e.prevent_default();
                on_drag_hover.emit(hover_target_at(x, y, membership_id));
                *autoscroll_y.borrow_mut() = y;
                if autoscroll_speed(y) != 0.0 {
                    if autoscroll_timer.borrow().is_none() {
                        let autoscroll_y = autoscroll_y.clone();
                        *autoscroll_timer.borrow_mut() = Some(Interval::new(16, move || {
                            let speed = autoscroll_speed(*autoscroll_y.borrow());
                            if speed != 0.0 {
                                if let Some(win) = web_sys::window() {
                                    win.scroll_by_with_x_and_y(0.0, speed);
                                }
                            }
                        }));
                    }
                } else {
                    *autoscroll_timer.borrow_mut() = None;
                }
                return;
            }

            if t.scrolling {
                if let Some(win) = web_sys::window() {
                    win.scroll_by_with_x_and_y(0.0, t.last_y - y);
                }
                t.last_y = y;
                *tracker.borrow_mut() = Some(t);
                return;
            }

            let dx = x - t.start_x;
            let dy = y - t.start_y;
            if t.is_touch {
                // Moved before the hold armed: hand off to a manual scroll instead.
                if dx.abs() > TOUCH_CANCEL_THRESHOLD || dy.abs() > TOUCH_CANCEL_THRESHOLD {
                    *hold_timer.borrow_mut() = None;
                    t.scrolling = true;
                    if let Some(win) = web_sys::window() {
                        win.scroll_by_with_x_and_y(0.0, -dy);
                    }
                    t.last_y = y;
                    *tracker.borrow_mut() = Some(t);
                }
                return;
            }

            if dx.abs() <= MOUSE_DRAG_THRESHOLD && dy.abs() <= MOUSE_DRAG_THRESHOLD {
                return;
            }
            t.active = true;
            *tracker.borrow_mut() = Some(t.clone());
            on_drag_start.emit(membership_id);
            e.prevent_default();
            on_drag_hover.emit(hover_target_at(x, y, membership_id));
        })
    };

    let on_pointer_up = {
        let tracker = tracker.clone();
        let hold_timer = hold_timer.clone();
        let autoscroll_timer = autoscroll_timer.clone();
        let suppress_click = suppress_click.clone();
        let state = state.clone();
        let on_drag_hover = props.on_drag_hover.clone();
        let on_drag_end = props.on_drag_end.clone();
        let membership_id = membership.id;
        Callback::from(move |e: PointerEvent| {
            let Some(t) = tracker.borrow().clone() else {
                return;
            };
            if t.pointer_id != e.pointer_id() {
                return;
            }
            *hold_timer.borrow_mut() = None;
            *autoscroll_timer.borrow_mut() = None;
            *tracker.borrow_mut() = None;
            if t.active {
                *suppress_click.borrow_mut() = true;
                let target = hover_target_at(
                    e.client_x() as f64,
                    e.client_y() as f64,
                    membership_id,
                );
                match target {
                    Some(DragHoverTarget::Reorder(before_id)) => {
                        state.dispatch(Action::MoveBefore {
                            membership_id,
                            before_id,
                        });
                    }
                    Some(DragHoverTarget::Nest(target_item_id)) => {
                        state.dispatch(Action::MoveInto {
                            membership_id,
                            target_item_id,
                        });
                    }
                    Some(DragHoverTarget::End) => {
                        state.dispatch(Action::MoveToEnd { membership_id });
                    }
                    None => {}
                }
                on_drag_hover.emit(None);
                on_drag_end.emit(());
            }
        })
    };

    let on_pointer_cancel = {
        let tracker = tracker.clone();
        let hold_timer = hold_timer.clone();
        let autoscroll_timer = autoscroll_timer.clone();
        let suppress_click = suppress_click.clone();
        let on_drag_hover = props.on_drag_hover.clone();
        let on_drag_end = props.on_drag_end.clone();
        Callback::from(move |_: PointerEvent| {
            *hold_timer.borrow_mut() = None;
            *autoscroll_timer.borrow_mut() = None;
            let was_active = tracker.borrow().as_ref().map(|t| t.active).unwrap_or(false);
            *tracker.borrow_mut() = None;
            if was_active {
                *suppress_click.borrow_mut() = true;
                on_drag_hover.emit(None);
                on_drag_end.emit(());
            }
        })
    };

    let stop_pointer_down = Callback::from(|e: PointerEvent| e.stop_propagation());

    let notes_preview = item
        .notes
        .as_deref()
        .and_then(|n| n.lines().next())
        .map(|line| line.trim())
        .filter(|line| !line.is_empty());

    let is_nest_hover = props.hover_nest_item == Some(item.id);

    let row_class = classes!(
        "item",
        item.done.then_some("done"),
        (!membership.visible).then_some("hidden-row"),
        is_dragging_this.then_some("dragging"),
        (props.depth > 0).then_some("nested-row")
    );
    // A nested row's own true parent differs from the list currently being viewed, so it
    // isn't a valid drag source or drop target (see `interactive_drag` above) - dropping
    // onto one would silently misplace an item relative to its real parent.
    let membership_attr = interactive_drag.then(|| membership.id.to_string());
    let indent_style = (props.depth > 0).then(|| format!("padding-left: {}ch;", props.depth));

    html! {
        <li class={row_class} data-membership-id={membership_attr}>
            <div
                class="item-main"
                style={indent_style}
                onclick={open_or_edit}
                onpointerdown={on_pointer_down}
                onpointermove={on_pointer_move}
                onpointerup={on_pointer_up}
                onpointercancel={on_pointer_cancel}
            >
                if item.is_note {
                    { note_icon() }
                } else if is_list {
                    { list_icon() }
                } else {
                    <input
                        type="checkbox"
                        checked={item.done}
                        onclick={toggle_done}
                        onpointerdown={stop_pointer_down.clone()}
                    />
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
                if has_dates {
                    <span class="dates-icon" title="Has scheduled dates">
                        { calendar_icon() }
                    </span>
                }
                if drag_active && !is_dragging_this && interactive_drag {
                    <span
                        class={classes!("nest-target", is_nest_hover.then_some("drag-over"))}
                        title="Drop to move inside"
                        data-drop-item={item.id.to_string()}
                        onpointerdown={stop_pointer_down}
                    >
                        { chevron_icon() }
                    </span>
                } else {
                    <button
                        class="edit-btn"
                        onclick={edit}
                        onpointerdown={stop_pointer_down}
                        title="Edit"
                        aria-label="Edit"
                    >
                        { edit_icon() }
                    </button>
                }
            </div>
        </li>
    }
}

#[derive(Properties, PartialEq)]
pub struct ItemEditorProps {
    pub state: UseReducerHandle<AppState>,
    pub item_id: Uuid,
    pub membership_id: Uuid,
    pub on_close: Callback<()>,
    pub on_manage_lists: Callback<Uuid>,
}

#[function_component(ItemEditor)]
pub fn item_editor(props: &ItemEditorProps) -> Html {
    let Some(item) = props.state.items.get(&props.item_id).cloned() else {
        return html! {};
    };
    let state = props.state.clone();
    let membership = state.memberships.get(&props.membership_id).cloned();

    let text = use_state(|| item.text.clone());
    let notes = use_state(|| item.notes.clone().unwrap_or_default());
    // Once true, the notes field stays large for the rest of this editing session.
    let large_notes = use_state(|| item.is_note || item.notes.is_some());
    let confirm_remove = use_state(|| false);
    let remove_children = use_state(|| true);
    // Collapsed by default - the due date/show/hide/repeat section is the least commonly
    // touched part of the form and takes up a lot of room when expanded.
    let show_dates = use_state(|| false);

    let due_input = use_state(|| item.due_at.map(to_datetime_local_value).unwrap_or_default());

    let show_mode = use_state(|| {
        if item.show_before_due_amount.is_some() && item.show_before_due_unit.is_some() {
            ShowMode::BeforeDue
        } else if item.show_after.is_some() {
            ShowMode::Fixed
        } else {
            ShowMode::Always
        }
    });
    let show_amount = use_state(|| item.show_before_due_amount.unwrap_or(1).to_string());
    let show_unit = use_state(|| {
        item.show_before_due_unit
            .clone()
            .unwrap_or_else(|| "days".to_string())
    });
    let show_fixed_input = use_state(|| {
        (*show_mode == ShowMode::Fixed)
            .then(|| item.show_after)
            .flatten()
            .map(to_datetime_local_value)
            .unwrap_or_default()
    });

    let hide_mode = use_state(|| {
        if item.hide_after_created_amount.is_some() && item.hide_after_created_unit.is_some() {
            HideMode::AfterCreated
        } else if item.hide_after.is_some() {
            HideMode::Fixed
        } else {
            HideMode::Never
        }
    });
    let hide_amount = use_state(|| item.hide_after_created_amount.unwrap_or(1).to_string());
    let hide_unit = use_state(|| {
        item.hide_after_created_unit
            .clone()
            .unwrap_or_else(|| "days".to_string())
    });
    let hide_fixed_input = use_state(|| {
        (*hide_mode == HideMode::Fixed)
            .then(|| item.hide_after)
            .flatten()
            .map(to_datetime_local_value)
            .unwrap_or_default()
    });

    let recur_enabled = use_state(|| item.recur_amount.is_some() && item.recur_unit.is_some());
    let recur_amount = use_state(|| item.recur_amount.unwrap_or(1).to_string());
    let recur_unit = use_state(|| item.recur_unit.clone().unwrap_or_else(|| "days".to_string()));

    let on_text_input = {
        let text = text.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            text.set(input.value());
        })
    };

    let on_notes_input = {
        let notes = notes.clone();
        let large_notes = large_notes.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlTextAreaElement = e.target_unchecked_into();
            let value = input.value();
            if !value.trim().is_empty() {
                large_notes.set(true);
            }
            notes.set(value);
        })
    };

    let on_due_input = {
        let due_input = due_input.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            due_input.set(input.value());
        })
    };
    let on_show_mode_change = {
        let show_mode = show_mode.clone();
        Callback::from(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            show_mode.set(ShowMode::from_str(&select.value()));
        })
    };
    let on_show_amount_input = {
        let show_amount = show_amount.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            show_amount.set(input.value());
        })
    };
    let on_show_unit_change = {
        let show_unit = show_unit.clone();
        Callback::from(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            show_unit.set(select.value());
        })
    };
    let on_show_fixed_input = {
        let show_fixed_input = show_fixed_input.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            show_fixed_input.set(input.value());
        })
    };
    let on_hide_mode_change = {
        let hide_mode = hide_mode.clone();
        Callback::from(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            hide_mode.set(HideMode::from_str(&select.value()));
        })
    };
    let on_hide_amount_input = {
        let hide_amount = hide_amount.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            hide_amount.set(input.value());
        })
    };
    let on_hide_unit_change = {
        let hide_unit = hide_unit.clone();
        Callback::from(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            hide_unit.set(select.value());
        })
    };
    let on_hide_fixed_input = {
        let hide_fixed_input = hide_fixed_input.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            hide_fixed_input.set(input.value());
        })
    };
    let on_recur_enabled_click = {
        let recur_enabled = recur_enabled.clone();
        Callback::from(move |_: MouseEvent| recur_enabled.set(!*recur_enabled))
    };
    let on_recur_amount_input = {
        let recur_amount = recur_amount.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            recur_amount.set(input.value());
        })
    };
    let on_recur_unit_change = {
        let recur_unit = recur_unit.clone();
        Callback::from(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            recur_unit.set(select.value());
        })
    };

    let toggle_nested_children = {
        let state = state.clone();
        let id = item.id;
        let current = item.show_nested_children;
        Callback::from(move |_: MouseEvent| {
            state.dispatch(Action::SetShowNestedChildren {
                item_id: id,
                value: !current,
            })
        })
    };

    let close = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| on_close.emit(()))
    };

    let copy_text = {
        let text = text.clone();
        Callback::from(move |_: MouseEvent| {
            if let Some(window) = web_sys::window() {
                let _ = window.navigator().clipboard().write_text(&text);
            }
        })
    };

    let save = {
        let state = state.clone();
        let id = item.id;
        let text = text.clone();
        let notes = notes.clone();
        let due_input = due_input.clone();
        let show_mode = show_mode.clone();
        let show_amount = show_amount.clone();
        let show_unit = show_unit.clone();
        let show_fixed_input = show_fixed_input.clone();
        let hide_mode = hide_mode.clone();
        let hide_amount = hide_amount.clone();
        let hide_unit = hide_unit.clone();
        let hide_fixed_input = hide_fixed_input.clone();
        let recur_enabled = recur_enabled.clone();
        let recur_amount = recur_amount.clone();
        let recur_unit = recur_unit.clone();
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| {
            let trimmed = text.trim().to_string();
            if !trimmed.is_empty() {
                state.dispatch(Action::UpdateText {
                    item_id: id,
                    text: trimmed,
                });
            }
            state.dispatch(Action::UpdateNotes {
                item_id: id,
                notes: (*notes).clone(),
            });

            let due_at = from_datetime_local_value(&due_input);
            let mut update = crate::state::ScheduleUpdate {
                due_at,
                ..Default::default()
            };
            match *show_mode {
                ShowMode::Always => {}
                ShowMode::BeforeDue => {
                    if let Ok(amount) = show_amount.parse::<i64>() {
                        update.show_before_due_amount = Some(amount);
                        update.show_before_due_unit = Some((*show_unit).clone());
                    }
                }
                ShowMode::Fixed => {
                    update.show_at_fixed = from_datetime_local_value(&show_fixed_input);
                }
            }
            match *hide_mode {
                HideMode::Never => {}
                HideMode::AfterCreated => {
                    if let Ok(amount) = hide_amount.parse::<i64>() {
                        update.hide_after_created_amount = Some(amount);
                        update.hide_after_created_unit = Some((*hide_unit).clone());
                    }
                }
                HideMode::Fixed => {
                    update.hide_at_fixed = from_datetime_local_value(&hide_fixed_input);
                }
            }
            if *recur_enabled {
                if let Ok(amount) = recur_amount.parse::<i64>() {
                    update.recur_amount = Some(amount);
                    update.recur_unit = Some((*recur_unit).clone());
                }
            }
            state.dispatch(Action::UpdateSchedule {
                item_id: id,
                update,
            });

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

    let membership_visible = membership.as_ref().map(|m| m.visible).unwrap_or(true);

    let toggle_visible = membership.as_ref().map(|m| {
        let state = state.clone();
        let membership_id = m.id;
        Callback::from(move |_: MouseEvent| state.dispatch(Action::ToggleVisible(membership_id)))
    });

    let toggle_dates = {
        let show_dates = show_dates.clone();
        Callback::from(move |_: MouseEvent| show_dates.set(!*show_dates))
    };

    let manage_lists = {
        let on_manage_lists = props.on_manage_lists.clone();
        let id = item.id;
        Callback::from(move |_: MouseEvent| on_manage_lists.emit(id))
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
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| {
            state.dispatch(Action::RemoveItem {
                item_id: id,
                remove_children: *remove_children,
            });
            confirm_remove.set(false);
            on_close.emit(());
        })
    };

    let removable_children = state.only_child_descendant_ids(item.id).len();

    html! {
        <div class="editor-overlay">
            <div class={classes!("editor", (*large_notes).then_some("editor-large"))}>
                <h2>{ "Edit item" }</h2>
                <label class="editor-field">
                    <span class="editor-field-label">
                        { "Text" }
                        <button
                            type="button"
                            class="copy-btn"
                            onclick={copy_text}
                            title="Copy text"
                            aria-label="Copy text"
                        >
                            { copy_icon() }
                        </button>
                    </span>
                    <input type="text" value={(*text).clone()} oninput={on_text_input} />
                </label>
                <label class="editor-field editor-notes-field">
                    <span>{ "Notes" }</span>
                    <textarea
                        class={classes!("notes", (*large_notes).then_some("notes-large"))}
                        placeholder="Details..."
                        oninput={on_notes_input}
                        value={(*notes).clone()}
                    />
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
                if *show_dates {
                <div class="editor-schedule">
                    <label class="editor-field">
                        <span>{ "Due date" }</span>
                        <input type="datetime-local" value={(*due_input).clone()} oninput={on_due_input} />
                    </label>
                    <div class="editor-schedule-row">
                        <span>{ "Show" }</span>
                        <select class="mode-select" onchange={on_show_mode_change} value={show_mode.as_str()}>
                            <option value="always" selected={*show_mode == ShowMode::Always}>{ "Always" }</option>
                            <option value="before_due" selected={*show_mode == ShowMode::BeforeDue}>{ "Before due date" }</option>
                            <option value="fixed" selected={*show_mode == ShowMode::Fixed}>{ "At date/time" }</option>
                        </select>
                        if *show_mode == ShowMode::BeforeDue {
                            <input
                                type="number"
                                min="0"
                                class="amount-input"
                                value={(*show_amount).clone()}
                                oninput={on_show_amount_input}
                            />
                            { time_unit_select(&show_unit, on_show_unit_change) }
                            <span class="editor-schedule-hint">{ "before due" }</span>
                        } else if *show_mode == ShowMode::Fixed {
                            <input
                                type="datetime-local"
                                value={(*show_fixed_input).clone()}
                                oninput={on_show_fixed_input}
                            />
                        }
                    </div>
                    <div class="editor-schedule-row">
                        <span>{ "Hide" }</span>
                        <select class="mode-select" onchange={on_hide_mode_change} value={hide_mode.as_str()}>
                            <option value="never" selected={*hide_mode == HideMode::Never}>{ "Never" }</option>
                            <option value="after_created" selected={*hide_mode == HideMode::AfterCreated}>{ "After created" }</option>
                            <option value="fixed" selected={*hide_mode == HideMode::Fixed}>{ "At date/time" }</option>
                        </select>
                        if *hide_mode == HideMode::AfterCreated {
                            <input
                                type="number"
                                min="0"
                                class="amount-input"
                                value={(*hide_amount).clone()}
                                oninput={on_hide_amount_input}
                            />
                            { time_unit_select(&hide_unit, on_hide_unit_change) }
                        } else if *hide_mode == HideMode::Fixed {
                            <input
                                type="datetime-local"
                                value={(*hide_fixed_input).clone()}
                                oninput={on_hide_fixed_input}
                            />
                        }
                    </div>
                    <div class="editor-schedule-row">
                        <label class="editor-schedule-recur-toggle">
                            <input type="checkbox" checked={*recur_enabled} onclick={on_recur_enabled_click} />
                            { " Repeats every" }
                        </label>
                        if *recur_enabled {
                            <input
                                type="number"
                                min="1"
                                class="amount-input"
                                value={(*recur_amount).clone()}
                                oninput={on_recur_amount_input}
                            />
                            { time_unit_select(&recur_unit, on_recur_unit_change) }
                        }
                    </div>
                    if item.is_list || state.direct_child_count(item.id) > 0 {
                        <label class="editor-schedule-recur-toggle">
                            <input
                                type="checkbox"
                                checked={item.show_nested_children}
                                onclick={toggle_nested_children}
                            />
                            { " Show multiple levels of children in this list" }
                        </label>
                    }
                </div>
                }
                <div class="editor-secondary-actions">
                    <button type="button" onclick={toggle_dates}>{ "Dates" }</button>
                    if let Some(toggle_visible) = toggle_visible {
                        <button onclick={toggle_visible}>
                            { if membership_visible { "Hide" } else { "Show" } }
                        </button>
                    }
                    <button onclick={manage_lists}>{ "Lists" }</button>
                    <button class="remove" onclick={open_confirm_remove}>{ "Remove" }</button>
                </div>
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
pub struct ListManagerProps {
    pub state: UseReducerHandle<AppState>,
    pub item_id: Uuid,
    pub on_close: Callback<()>,
}

/// Full-screen manager for which lists an item belongs to. Lets the item have several
/// parents at once (or none) by browsing the same hierarchy as the main view, or by
/// searching, and checking/unchecking candidate parents before saving.
#[function_component(ListManager)]
pub fn list_manager(props: &ListManagerProps) -> Html {
    let state = props.state.clone();
    let item_id = props.item_id;
    let Some(item) = state.items.get(&item_id).cloned() else {
        return html! {};
    };

    let path = use_state(Vec::<Uuid>::new);
    let query = use_state(String::new);
    let pending = use_state(|| {
        state
            .memberships
            .values()
            .filter(|m| m.item_id == item_id && m.deleted_at.is_none())
            .filter_map(|m| m.parent_id)
            .collect::<HashSet<Uuid>>()
    });

    // Never allow the item to become its own ancestor.
    let mut excluded = state.descendant_ids(item_id);
    excluded.insert(item_id);

    let on_query_input = {
        let query = query.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            query.set(input.value());
        })
    };

    let on_navigate = {
        let path = path.clone();
        Callback::from(move |new_path: Vec<Uuid>| path.set(new_path))
    };

    let cancel = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| on_close.emit(()))
    };

    let save = {
        let state = state.clone();
        let pending = pending.clone();
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| {
            let original: HashSet<Uuid> = state
                .memberships
                .values()
                .filter(|m| m.item_id == item_id && m.deleted_at.is_none())
                .filter_map(|m| m.parent_id)
                .collect();
            for parent in pending.iter() {
                if !original.contains(parent) {
                    state.dispatch(Action::AddToList {
                        item_id,
                        parent: Some(*parent),
                    });
                }
            }
            for parent in original.iter() {
                if !pending.contains(parent) {
                    let existing = state.memberships.values().find(|m| {
                        m.item_id == item_id
                            && m.parent_id == Some(*parent)
                            && m.deleted_at.is_none()
                    });
                    if let Some(m) = existing {
                        state.dispatch(Action::RemoveMembership(m.id));
                    }
                }
            }
            on_close.emit(());
        })
    };

    let query_lower = query.trim().to_lowercase();
    let current_parent = path.last().copied();

    let rows: Vec<Item> = if query_lower.is_empty() {
        state
            .children(current_parent, true)
            .into_iter()
            .map(|(row_item, _)| row_item)
            .filter(|i| !excluded.contains(&i.id))
            .collect()
    } else {
        let mut matches: Vec<Item> = state
            .items
            .values()
            .filter(|i| i.deleted_at.is_none())
            .filter(|i| !excluded.contains(&i.id))
            .filter(|i| i.text.to_lowercase().contains(&query_lower))
            .cloned()
            .collect();
        matches.sort_by(|a, b| a.text.to_lowercase().cmp(&b.text.to_lowercase()));
        matches.truncate(50);
        matches
    };

    html! {
        <div class="trash-view list-manager">
            <div class="trash-header">
                <h2>{ "Manage lists" }</h2>
                <button onclick={cancel.clone()}>{ "Back" }</button>
            </div>
            <div class="list-manager-current">
                if item.is_note { { note_icon() } } else if item.is_list || state.direct_child_count(item_id) > 0 {
                    { list_icon() }
                } else {
                    <input type="checkbox" checked={item.done} disabled=true />
                }
                <span class="item-text">{ &item.text }</span>
            </div>
            <input
                type="text"
                class="list-manager-search"
                placeholder="Search for a list..."
                value={(*query).clone()}
                oninput={on_query_input}
            />
            if query_lower.is_empty() {
                <Breadcrumbs state={state.clone()} path={(*path).clone()} on_navigate={on_navigate} />
            }
            <ul class="items list-manager-list">
                { for rows.iter().map(|row_item| {
                    let row_id = row_item.id;
                    let checked = pending.contains(&row_id);
                    let child_count = state.direct_child_count(row_id);
                    let has_children = child_count > 0;

                    let toggle = {
                        let pending = pending.clone();
                        Callback::from(move |e: MouseEvent| {
                            e.stop_propagation();
                            let mut next = (*pending).clone();
                            if next.contains(&row_id) {
                                next.remove(&row_id);
                            } else {
                                next.insert(row_id);
                            }
                            pending.set(next);
                        })
                    };

                    let open = {
                        let path = path.clone();
                        let query = query.clone();
                        Callback::from(move |_: MouseEvent| {
                            if !has_children {
                                return;
                            }
                            let mut next = (*path).clone();
                            next.push(row_id);
                            path.set(next);
                            query.set(String::new());
                        })
                    };

                    html! {
                        <li class={classes!("item", (!has_children).then_some("subtle"))} key={row_id.to_string()}>
                            <div class="item-main" onclick={open}>
                                <input type="checkbox" checked={checked} onclick={toggle} />
                                if row_item.is_note {
                                    { note_icon() }
                                } else if row_item.is_list || has_children {
                                    { list_icon() }
                                }
                                <span class="item-text">{ &row_item.text }</span>
                                if child_count > 0 {
                                    <span class="child-count">{ child_count }</span>
                                }
                            </div>
                        </li>
                    }
                }) }
            </ul>
            if rows.is_empty() {
                <p class="empty">
                    { if query_lower.is_empty() { "Nothing here." } else { "No matches." } }
                </p>
            }
            <div class="editor-actions list-manager-actions">
                <button onclick={save}>{ "Save" }</button>
                <button onclick={cancel}>{ "Cancel" }</button>
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

async fn refresh_admin_users(
    users: UseStateHandle<Option<Vec<admin::AdminUser>>>,
    error: UseStateHandle<Option<String>>,
) {
    match admin::list_users().await {
        Ok(list) => users.set(Some(list)),
        Err(msg) => error.set(Some(msg)),
    }
}

#[derive(Properties, PartialEq)]
pub struct AdminPageProps {
    pub current_user_id: String,
    pub on_close: Callback<()>,
}

#[function_component(AdminPage)]
pub fn admin_page(props: &AdminPageProps) -> Html {
    let users = use_state(|| None::<Vec<admin::AdminUser>>);
    let error = use_state(|| None::<String>);
    let status = use_state(|| None::<String>);

    {
        let users = users.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            spawn_local(refresh_admin_users(users, error));
            || ()
        });
    }

    let close = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| on_close.emit(()))
    };

    let current_user_id = props.current_user_id.clone();

    html! {
        <div class="trash-view admin-page">
            <div class="trash-header">
                <h2>{ "Admin" }</h2>
                <button onclick={close}>{ "Back" }</button>
            </div>
            if let Some(msg) = &*error {
                <p class="auth-error">{ msg }</p>
            }
            if let Some(msg) = &*status {
                <p class="auth-info">{ msg }</p>
            }
            {
                match &*users {
                    None => html! { <p class="empty">{ "Loading..." }</p> },
                    Some(list) if list.is_empty() => html! { <p class="empty">{ "No accounts yet." }</p> },
                    Some(list) => html! {
                        <ul class="admin-user-list">
                            { for list.iter().map(|u| {
                                let is_self = u.id.to_string() == current_user_id;

                                let approve = {
                                    let id = u.id;
                                    let email = u.email.clone();
                                    let users = users.clone();
                                    let error = error.clone();
                                    let status = status.clone();
                                    Callback::from(move |_: MouseEvent| {
                                        let users = users.clone();
                                        let error = error.clone();
                                        let status = status.clone();
                                        let email = email.clone();
                                        spawn_local(async move {
                                            error.set(None);
                                            if let Err(msg) = admin::set_status(id, "approved").await {
                                                error.set(Some(msg));
                                            } else {
                                                status.set(Some(format!("Approved {email}.")));
                                                refresh_admin_users(users, error).await;
                                            }
                                        });
                                    })
                                };

                                let disable = {
                                    let id = u.id;
                                    let email = u.email.clone();
                                    let users = users.clone();
                                    let error = error.clone();
                                    let status = status.clone();
                                    Callback::from(move |_: MouseEvent| {
                                        let users = users.clone();
                                        let error = error.clone();
                                        let status = status.clone();
                                        let email = email.clone();
                                        spawn_local(async move {
                                            error.set(None);
                                            if let Err(msg) = admin::set_status(id, "disabled").await {
                                                error.set(Some(msg));
                                            } else {
                                                status.set(Some(format!("Disabled {email}.")));
                                                refresh_admin_users(users, error).await;
                                            }
                                        });
                                    })
                                };

                                let enable = {
                                    let id = u.id;
                                    let email = u.email.clone();
                                    let users = users.clone();
                                    let error = error.clone();
                                    let status = status.clone();
                                    Callback::from(move |_: MouseEvent| {
                                        let users = users.clone();
                                        let error = error.clone();
                                        let status = status.clone();
                                        let email = email.clone();
                                        spawn_local(async move {
                                            error.set(None);
                                            if let Err(msg) = admin::set_status(id, "approved").await {
                                                error.set(Some(msg));
                                            } else {
                                                status.set(Some(format!("Re-enabled {email}.")));
                                                refresh_admin_users(users, error).await;
                                            }
                                        });
                                    })
                                };

                                let toggle_admin = {
                                    let id = u.id;
                                    let email = u.email.clone();
                                    let make_admin = !u.is_admin;
                                    let users = users.clone();
                                    let error = error.clone();
                                    let status = status.clone();
                                    Callback::from(move |_: MouseEvent| {
                                        let users = users.clone();
                                        let error = error.clone();
                                        let status = status.clone();
                                        let email = email.clone();
                                        spawn_local(async move {
                                            error.set(None);
                                            if let Err(msg) = admin::set_admin(id, make_admin).await {
                                                error.set(Some(msg));
                                            } else {
                                                status.set(Some(if make_admin {
                                                    format!("{email} is now an admin.")
                                                } else {
                                                    format!("{email} is no longer an admin.")
                                                }));
                                                refresh_admin_users(users, error).await;
                                            }
                                        });
                                    })
                                };

                                let delete = {
                                    let id = u.id;
                                    let email = u.email.clone();
                                    let users = users.clone();
                                    let error = error.clone();
                                    let status = status.clone();
                                    Callback::from(move |_: MouseEvent| {
                                        let confirmed = web_sys::window()
                                            .and_then(|w| {
                                                w.confirm_with_message(&format!(
                                                    "Delete the account \"{email}\" and all of its data? This cannot be undone."
                                                ))
                                                .ok()
                                            })
                                            .unwrap_or(false);
                                        if !confirmed {
                                            return;
                                        }
                                        let users = users.clone();
                                        let error = error.clone();
                                        let status = status.clone();
                                        let email = email.clone();
                                        spawn_local(async move {
                                            error.set(None);
                                            if let Err(msg) = admin::delete_user(id).await {
                                                error.set(Some(msg));
                                            } else {
                                                status.set(Some(format!("Deleted {email}.")));
                                                refresh_admin_users(users, error).await;
                                            }
                                        });
                                    })
                                };

                                html! {
                                    <li class="admin-user-row" key={u.id.to_string()}>
                                        <div class="admin-user-info">
                                            <span class="admin-user-email">{ &u.email }</span>
                                            <span class={classes!("admin-user-status", format!("status-{}", u.status))}>
                                                { &u.status }
                                            </span>
                                            if u.is_admin {
                                                <span class="admin-user-badge">{ "admin" }</span>
                                            }
                                            if is_self {
                                                <span class="admin-user-you">{ "(you)" }</span>
                                            }
                                        </div>
                                        <div class="admin-user-actions">
                                            if u.status == "pending" {
                                                <button onclick={approve}>{ "Approve" }</button>
                                            }
                                            if u.status == "approved" && !is_self {
                                                <button onclick={disable}>{ "Disable" }</button>
                                            }
                                            if u.status == "disabled" {
                                                <button onclick={enable}>{ "Enable" }</button>
                                            }
                                            if !is_self {
                                                <button onclick={toggle_admin}>
                                                    { if u.is_admin { "Remove admin" } else { "Make admin" } }
                                                </button>
                                            }
                                            if !is_self {
                                                <button class="remove" onclick={delete}>{ "Delete" }</button>
                                            }
                                        </div>
                                    </li>
                                }
                            }) }
                        </ul>
                    },
                }
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct SettingsMenuProps {
    pub state: UseReducerHandle<AppState>,
    pub is_admin: bool,
    /// Google Tasks import is tied to a server-side account, so it's hidden for a
    /// logged-out guest; file export/import work fully offline and stay available.
    pub is_logged_in: bool,
    /// The list currently being viewed - a markdown import lands here as a new item,
    /// as a top-level list when this is `None`.
    pub current_parent: Option<Uuid>,
    pub on_open_admin: Callback<()>,
}

#[function_component(SettingsMenu)]
pub fn settings_menu(props: &SettingsMenuProps) -> Html {
    let open = use_state(|| false);
    let root_ref = use_node_ref();
    use_click_outside(root_ref.clone(), open.clone());
    let file_input_ref = use_node_ref();
    let markdown_input_ref = use_node_ref();

    let toggle = {
        let open = open.clone();
        Callback::from(move |_: MouseEvent| open.set(!*open))
    };

    let open_admin = {
        let open = open.clone();
        let on_open_admin = props.on_open_admin.clone();
        Callback::from(move |_: MouseEvent| {
            open.set(false);
            on_open_admin.emit(());
        })
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

    let choose_import_markdown = {
        let open = open.clone();
        let markdown_input_ref = markdown_input_ref.clone();
        Callback::from(move |_: MouseEvent| {
            open.set(false);
            if let Some(input) = markdown_input_ref.cast::<HtmlInputElement>() {
                input.click();
            }
        })
    };

    let on_markdown_file_change = {
        let state = props.state.clone();
        let current_parent = props.current_parent;
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            let Some(file) = input.files().and_then(|list| list.get(0)) else {
                return;
            };
            input.set_value("");

            // The file's name (minus the .md/.markdown extension) becomes the imported
            // list's title.
            let raw_name = file.name();
            let list_name = raw_name
                .strip_suffix(".md")
                .or_else(|| raw_name.strip_suffix(".markdown"))
                .unwrap_or(&raw_name)
                .to_string();

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
                let mut import = markdown::parse_markdown(&text);
                if import.items.is_empty() {
                    if let Some(window) = web_sys::window() {
                        let _ = window.alert_with_message(
                            "No headings or list items were found in that markdown file.",
                        );
                    }
                    return;
                }
                // Wrap the whole import in a single new list item, named after the
                // file, so it lands as one item in the current list.
                let mut wrapper = Item::new(list_name, false);
                wrapper.is_list = true;
                let wrapper_id = wrapper.id;
                for m in import.memberships.iter_mut() {
                    if m.parent_id.is_none() {
                        m.parent_id = Some(wrapper_id);
                    }
                }
                import.items.push(wrapper);
                import.memberships.push(Membership::new(wrapper_id, None, 0.0));

                state.dispatch(Action::ImportMarkdown {
                    items: import.items,
                    memberships: import.memberships,
                    parent: current_parent,
                });
            });
            reader.set_onload(Some(onload.as_ref().unchecked_ref()));
            onload.forget();
            let _ = reader.read_as_text(&file);
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
        <div class="settings-menu" ref={root_ref}>
            <button class="settings-btn" onclick={toggle} title="Menu" aria-label="Menu">
                { menu_icon() }
            </button>
            if *open {
                <div class="settings-dropdown">
                    if props.is_logged_in {
                        <button class="settings-item" onclick={import_google}>
                            { "Import from Google Tasks" }
                        </button>
                    }
                    <button class="settings-item" onclick={export_json}>
                        { "Export tasks (JSON)" }
                    </button>
                    <button class="settings-item" onclick={choose_import_file}>
                        { "Import tasks (JSON)" }
                    </button>
                    <button class="settings-item" onclick={choose_import_markdown}>
                        { "Import markdown (.md)" }
                    </button>
                    if props.is_admin {
                        <button class="settings-item" onclick={open_admin}>
                            { "Admin" }
                        </button>
                    }
                </div>
            }
            <input
                type="file"
                accept="application/json"
                ref={file_input_ref}
                onchange={on_file_change}
                style="display: none;"
            />
            <input
                type="file"
                accept=".md,text/markdown"
                ref={markdown_input_ref}
                onchange={on_markdown_file_change}
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

    let select_all = {
        let selected = selected.clone();
        let count = props.lists.len();
        Callback::from(move |_: MouseEvent| selected.set((0..count).collect()))
    };
    let deselect_all = {
        let selected = selected.clone();
        Callback::from(move |_: MouseEvent| selected.set(HashSet::new()))
    };
    let invert_selection = {
        let selected = selected.clone();
        let count = props.lists.len();
        Callback::from(move |_: MouseEvent| {
            let inverted = (0..count).filter(|i| !selected.contains(i)).collect();
            selected.set(inverted);
        })
    };

    html! {
        <div class="editor-overlay">
            <div class="editor">
                <h2>{ "Import from Google Tasks" }</h2>
                if props.lists.is_empty() {
                    <p>{ "No Google task lists were found for this account." }</p>
                } else {
                    <div class="google-import-select-actions">
                        <button type="button" onclick={select_all}>{ "Select all" }</button>
                        <button type="button" onclick={deselect_all}>{ "Deselect all" }</button>
                        <button type="button" onclick={invert_selection}>{ "Invert" }</button>
                    </div>
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
