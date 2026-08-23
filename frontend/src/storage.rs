use crate::model::ListState;
use gloo_storage::{LocalStorage, Storage};

const KEY: &str = "lister.state";

pub fn load() -> Option<ListState> {
    LocalStorage::get(KEY).ok()
}

pub fn save(state: &ListState) {
    let _ = LocalStorage::set(KEY, state);
}
