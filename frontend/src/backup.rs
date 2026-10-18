use crate::model::{Item, Membership};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

/// On-disk shape of a full export. `version` lets future imports detect and migrate
/// older exports if the format ever changes.
#[derive(Serialize, Deserialize)]
pub struct ExportData {
    pub version: u32,
    pub exported_at: DateTime<Utc>,
    pub items: Vec<Item>,
    pub memberships: Vec<Membership>,
}

const CURRENT_VERSION: u32 = 1;

pub fn export_json(items: Vec<Item>, memberships: Vec<Membership>) -> String {
    let data = ExportData {
        version: CURRENT_VERSION,
        exported_at: Utc::now(),
        items,
        memberships,
    };
    serde_json::to_string_pretty(&data).unwrap_or_default()
}

pub fn parse_import(text: &str) -> Result<ExportData, serde_json::Error> {
    serde_json::from_str(text)
}

/// The rows of an import that a merge should add: everything whose id isn't already
/// present locally. Re-importing an export into the account it came from adds nothing.
pub fn new_rows(
    data: ExportData,
    items: &HashMap<Uuid, Item>,
    memberships: &HashMap<Uuid, Membership>,
) -> (Vec<Item>, Vec<Membership>) {
    let new_items = data
        .items
        .into_iter()
        .filter(|i| !items.contains_key(&i.id))
        .collect();
    let new_memberships = data
        .memberships
        .into_iter()
        .filter(|m| !memberships.contains_key(&m.id))
        .collect();
    (new_items, new_memberships)
}

/// Trigger a browser download of `contents` as a file named `filename`.
pub fn download_json(filename: &str, contents: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };

    let parts = js_sys::Array::of1(&JsValue::from_str(contents));
    let props = BlobPropertyBag::new();
    props.set_type("application/json");
    let Ok(blob) = Blob::new_with_str_sequence_and_options(&parts, &props) else {
        return;
    };
    let Ok(url) = Url::create_object_url_with_blob(&blob) else {
        return;
    };

    if let Ok(el) = document.create_element("a") {
        let anchor: HtmlAnchorElement = el.unchecked_into();
        anchor.set_href(&url);
        anchor.set_download(filename);
        if let Some(body) = document.body() {
            let _ = body.append_child(&anchor);
            anchor.click();
            let _ = body.remove_child(&anchor);
        }
    }
    let _ = Url::revoke_object_url(&url);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rows_skips_ids_that_already_exist() {
        let kept = Item::new("Kept".to_string(), false);
        let fresh = Item::new("Fresh".to_string(), false);
        let kept_m = Membership::new(kept.id, None, 1.0);
        let fresh_m = Membership::new(fresh.id, None, 2.0);

        let items = HashMap::from([(kept.id, kept.clone())]);
        let memberships = HashMap::from([(kept_m.id, kept_m.clone())]);
        let data = ExportData {
            version: CURRENT_VERSION,
            exported_at: Utc::now(),
            items: vec![kept, fresh.clone()],
            memberships: vec![kept_m, fresh_m.clone()],
        };

        let (new_items, new_memberships) = new_rows(data, &items, &memberships);
        assert_eq!(
            new_items.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![fresh.id]
        );
        assert_eq!(
            new_memberships.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![fresh_m.id]
        );
    }
}
