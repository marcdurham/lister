use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ImportedTask {
    pub title: String,
    pub notes: Option<String>,
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ImportedTaskList {
    pub title: String,
    pub tasks: Vec<ImportedTask>,
}

/// Kick off the Google OAuth flow via a full-page navigation. The backend redirects to
/// Google, then back to `/?google_import=1` once the tasks have been fetched server-side.
pub fn start_connect() {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href("/api/google/connect");
    }
}

/// True if the current URL is the post-OAuth landing spot.
pub fn returned_from_google() -> bool {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|search| search.contains("google_import=1"))
        .unwrap_or(false)
}

/// Strip the `google_import` marker from the URL so a refresh doesn't re-trigger the import prompt.
pub fn clear_return_marker() {
    if let Some(window) = web_sys::window() {
        if let Ok(history) = window.history() {
            let _ = history.replace_state_with_url(
                &wasm_bindgen::JsValue::NULL,
                "",
                Some(&window.location().pathname().unwrap_or_default()),
            );
        }
    }
}

pub async fn fetch_imported_tasks() -> Vec<ImportedTaskList> {
    let Ok(resp) = Request::get("/api/google/imported-tasks").send().await else {
        return Vec::new();
    };
    if !resp.ok() {
        return Vec::new();
    }
    resp.json().await.unwrap_or_default()
}
