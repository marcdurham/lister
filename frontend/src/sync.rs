use crate::store;
use gloo_net::http::Request;
use shared::SyncRequest;
use uuid::Uuid;

pub fn is_online() -> bool {
    web_sys::window()
        .map(|w| w.navigator().on_line())
        .unwrap_or(true)
}

/// A user-facing message for a non-2xx sync response, including the server's own error
/// text when it sent plain text (proxy error pages are HTML, so those get a hint instead).
async fn server_error(response: gloo_net::http::Response) -> String {
    let status = response.status();
    if status == 401 {
        return "Sign in to sync with the server.".to_string();
    }
    if status == 413 {
        return "The server rejected the sync as too large (413).".to_string();
    }
    let body = response.text().await.unwrap_or_default();
    let body = body.trim();
    if body.is_empty() || body.starts_with('<') {
        format!("The server returned an error ({status}).")
    } else {
        let detail: String = body.chars().take(300).collect();
        format!("The server returned an error ({status}): {detail}")
    }
}

/// Push locally dirty rows and pull whatever changed server-side since our last cursor.
/// The background loop ignores the error and retries on the next tick; the Sync button
/// shows it.
pub async fn sync_once() -> Result<(), String> {
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

    let response = Request::post("/api/sync")
        .json(&request)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|_| "Couldn't reach the server.".to_string())?;
    if !response.ok() {
        return Err(server_error(response).await);
    }
    let body: shared::SyncResponse = response
        .json()
        .await
        .map_err(|_| "The server sent an unreadable response.".to_string())?;

    store::mark_synced(&item_ids, &membership_ids);
    store::apply_pulled(body.items, body.memberships);
    store::set_cursor(body.cursor);
    store::set_last_sync(chrono::Utc::now());
    Ok(())
}

/// What a per-item push/sync ended up doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemSyncOutcome {
    /// Local changes were uploaded and the server now has this exact version.
    Uploaded,
    /// Nothing to upload - the server already had this exact version.
    AlreadyInSync,
    /// The server had a newer version and it replaced the local one.
    Pulled,
    /// The server has a newer version, which was left alone (push-only).
    ServerNewer,
}

/// Push a single item (plus any not-yet-uploaded ancestor lists it hangs off) and check
/// what the server holds for it afterwards. The server keeps whichever copy has the newer
/// `updated_at`, so if its copy is newer the push is a no-op; with `pull` that newer copy
/// then replaces the local one, otherwise it's reported and left alone.
///
/// The request's `since` is set just before the item's own `updated_at`, so the response
/// contains the item exactly when the server holds a version at least as new as ours -
/// which is what tells us the push actually landed.
pub async fn sync_item(item_id: Uuid, pull: bool) -> Result<ItemSyncOutcome, String> {
    if !is_online() {
        return Err("You're offline.".to_string());
    }
    let was_dirty = store::is_item_dirty(item_id);
    let (items, memberships) = store::push_bundle(item_id);
    let Some(local) = items.iter().find(|i| i.id == item_id).cloned() else {
        return Err("Item not found locally.".to_string());
    };
    let pushed_item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    let pushed_membership_ids: Vec<Uuid> = memberships.iter().map(|m| m.id).collect();

    let request = SyncRequest {
        since: Some(local.updated_at - chrono::Duration::milliseconds(1)),
        items,
        memberships,
    };
    let response = Request::post("/api/sync")
        .json(&request)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|_| "Couldn't reach the server.".to_string())?;
    if !response.ok() {
        return Err(server_error(response).await);
    }
    let body: shared::SyncResponse = response
        .json()
        .await
        .map_err(|_| "The server sent an unreadable response.".to_string())?;

    let Some(server) = body.items.iter().find(|i| i.id == item_id).cloned() else {
        return Err("The server did not accept this item.".to_string());
    };

    // Postgres stores microseconds, so compare at that precision.
    if server.updated_at.timestamp_micros() > local.updated_at.timestamp_micros() {
        if pull {
            let memberships = body
                .memberships
                .into_iter()
                .filter(|m| m.item_id == item_id)
                .collect();
            store::overwrite_item_from_server(server, memberships);
            return Ok(ItemSyncOutcome::Pulled);
        }
        return Ok(ItemSyncOutcome::ServerNewer);
    }

    store::mark_synced(&pushed_item_ids, &pushed_membership_ids);
    Ok(if was_dirty {
        ItemSyncOutcome::Uploaded
    } else {
        ItemSyncOutcome::AlreadyInSync
    })
}
