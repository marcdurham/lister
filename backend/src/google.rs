use actix_web::{get, web, HttpRequest, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

use crate::auth;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const TASKLISTS_URL: &str = "https://tasks.googleapis.com/tasks/v1/users/@me/lists";
const SCOPE: &str = "https://www.googleapis.com/auth/tasks.readonly";

/// Transient server-side state for the OAuth round trip and the tasks pulled from Google,
/// waiting to be picked up by the frontend. Nothing here needs to survive a restart.
#[derive(Default)]
pub struct GoogleImportCache {
    pending_states: Mutex<HashMap<String, Uuid>>,
    imported: Mutex<HashMap<Uuid, Vec<ImportedTaskList>>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ImportedTask {
    pub title: String,
    pub notes: Option<String>,
    pub done: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ImportedTaskList {
    pub title: String,
    pub tasks: Vec<ImportedTask>,
}

fn client_config() -> Option<(String, String, String)> {
    let id = std::env::var("GOOGLE_CLIENT_ID").ok()?;
    let secret = std::env::var("GOOGLE_CLIENT_SECRET").ok()?;
    let redirect = std::env::var("GOOGLE_REDIRECT_URI").ok()?;
    Some((id, secret, redirect))
}

#[get("/api/google/connect")]
pub async fn connect(
    pool: web::Data<PgPool>,
    cache: web::Data<GoogleImportCache>,
    req: HttpRequest,
) -> impl Responder {
    let Some(user_id) = auth::current_user_id(&req, pool.get_ref()).await else {
        return HttpResponse::Unauthorized().body("not logged in");
    };
    let Some((client_id, _secret, redirect_uri)) = client_config() else {
        return HttpResponse::ServiceUnavailable()
            .body("Google Tasks import is not configured on this server");
    };

    let state = Uuid::new_v4().to_string();
    cache
        .pending_states
        .lock()
        .unwrap()
        .insert(state.clone(), user_id);

    let url = format!(
        "{AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&access_type=offline&prompt=consent&scope={}&state={}",
        urlencoding::encode(&client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(SCOPE),
        urlencoding::encode(&state),
    );

    HttpResponse::Found()
        .append_header(("Location", url))
        .finish()
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct GTaskList {
    id: String,
    title: String,
}

#[derive(Deserialize)]
struct GTaskListsResponse {
    items: Option<Vec<GTaskList>>,
}

#[derive(Deserialize)]
struct GTask {
    title: String,
    notes: Option<String>,
    status: Option<String>,
}

#[derive(Deserialize)]
struct GTasksResponse {
    items: Option<Vec<GTask>>,
}

#[get("/api/google/callback")]
pub async fn callback(
    cache: web::Data<GoogleImportCache>,
    query: web::Query<HashMap<String, String>>,
) -> impl Responder {
    let Some(code) = query.get("code") else {
        return HttpResponse::BadRequest().body("missing code");
    };
    let Some(state) = query.get("state") else {
        return HttpResponse::BadRequest().body("missing state");
    };
    let Some(user_id) = cache.pending_states.lock().unwrap().remove(state) else {
        return HttpResponse::BadRequest().body("invalid or expired state");
    };
    let Some((client_id, client_secret, redirect_uri)) = client_config() else {
        return HttpResponse::ServiceUnavailable().body("Google Tasks import is not configured");
    };

    let client = reqwest::Client::new();
    let token_resp = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await;

    let token = match token_resp {
        Ok(resp) if resp.status().is_success() => resp.json::<TokenResponse>().await.ok(),
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_default();
            log::error!("google token exchange failed: {body}");
            None
        }
        Err(err) => {
            log::error!("google token request failed: {err}");
            None
        }
    };
    let Some(token) = token else {
        return HttpResponse::BadGateway().body("failed to exchange code with Google");
    };

    let lists = match client
        .get(TASKLISTS_URL)
        .bearer_auth(&token.access_token)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => resp
            .json::<GTaskListsResponse>()
            .await
            .ok()
            .and_then(|r| r.items)
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let mut imported = Vec::new();
    for list in lists {
        let tasks_url = format!("https://tasks.googleapis.com/tasks/v1/lists/{}/tasks", list.id);
        let tasks = match client
            .get(&tasks_url)
            .bearer_auth(&token.access_token)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => resp
                .json::<GTasksResponse>()
                .await
                .ok()
                .and_then(|r| r.items)
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        imported.push(ImportedTaskList {
            title: list.title,
            tasks: tasks
                .into_iter()
                .map(|t| ImportedTask {
                    title: t.title,
                    notes: t.notes,
                    done: t.status.as_deref() == Some("completed"),
                })
                .collect(),
        });
    }

    cache.imported.lock().unwrap().insert(user_id, imported);

    HttpResponse::Found()
        .append_header(("Location", "/?google_import=1"))
        .finish()
}

#[get("/api/google/imported-tasks")]
pub async fn imported_tasks(
    pool: web::Data<PgPool>,
    cache: web::Data<GoogleImportCache>,
    req: HttpRequest,
) -> impl Responder {
    let Some(user_id) = auth::current_user_id(&req, pool.get_ref()).await else {
        return HttpResponse::Unauthorized().finish();
    };
    let lists = cache
        .imported
        .lock()
        .unwrap()
        .remove(&user_id)
        .unwrap_or_default();
    HttpResponse::Ok().json(lists)
}
