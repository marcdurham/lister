use crate::model::{Item, Membership};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
