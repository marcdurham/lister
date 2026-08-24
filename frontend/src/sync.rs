use crate::store;
use gloo_net::http::Request;
use shared::SyncRequest;

pub fn is_online() -> bool {
    web_sys::window()
        .map(|w| w.navigator().on_line())
        .unwrap_or(true)
}

/// Push locally dirty rows and pull whatever changed server-side since our last cursor.
/// Errors are swallowed - the caller just retries on the next tick.
pub async fn sync_once() -> bool {
    let items = store::dirty_items();
    let memberships = store::dirty_memberships();
    let since = store::get_cursor();

    let item_ids: Vec<_> = items.iter().map(|i| i.id).collect();
    let membership_ids: Vec<_> = memberships.iter().map(|m| m.id).collect();

    let request = SyncRequest {
        since,
        items,
        memberships,
    };

    let response = match Request::post("/api/sync").json(&request) {
        Ok(builder) => builder.send().await,
        Err(_) => return false,
    };

    let response = match response {
        Ok(resp) if resp.ok() => resp,
        _ => return false,
    };

    let body: shared::SyncResponse = match response.json().await {
        Ok(body) => body,
        Err(_) => return false,
    };

    store::mark_synced(&item_ids, &membership_ids);
    store::apply_pulled(body.items, body.memberships);
    store::set_cursor(body.cursor);
    true
}
