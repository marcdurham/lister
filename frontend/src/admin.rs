use chrono::{DateTime, Utc};
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AdminUser {
    pub id: Uuid,
    pub email: String,
    pub status: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

async fn error_message(resp: gloo_net::http::Response) -> String {
    resp.text()
        .await
        .unwrap_or_else(|_| "request failed".to_string())
}

pub async fn list_users() -> Result<Vec<AdminUser>, String> {
    let resp = Request::get("/api/admin/users")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(error_message(resp).await)
    }
}

#[derive(Serialize)]
struct StatusBody<'a> {
    status: &'a str,
}

pub async fn set_status(id: Uuid, status: &str) -> Result<(), String> {
    let resp = Request::post(&format!("/api/admin/users/{id}/status"))
        .json(&StatusBody { status })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(error_message(resp).await)
    }
}

#[derive(Serialize)]
struct AdminBody {
    is_admin: bool,
}

pub async fn set_admin(id: Uuid, is_admin: bool) -> Result<(), String> {
    let resp = Request::post(&format!("/api/admin/users/{id}/admin"))
        .json(&AdminBody { is_admin })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(error_message(resp).await)
    }
}

pub async fn delete_user(id: Uuid) -> Result<(), String> {
    let resp = Request::delete(&format!("/api/admin/users/{id}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(error_message(resp).await)
    }
}

/// A read-only API token of the logged-in account (the token itself is only shown once, on creation).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ApiToken {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct NewApiToken {
    pub name: String,
    pub token: String,
}

pub async fn list_tokens() -> Result<Vec<ApiToken>, String> {
    let resp = Request::get("/api/tokens")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(error_message(resp).await)
    }
}

#[derive(Serialize)]
struct TokenBody<'a> {
    name: &'a str,
}

pub async fn create_token(name: &str) -> Result<NewApiToken, String> {
    let resp = Request::post("/api/tokens")
        .json(&TokenBody { name })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(error_message(resp).await)
    }
}

pub async fn revoke_token(id: Uuid) -> Result<(), String> {
    let resp = Request::delete(&format!("/api/tokens/{id}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(error_message(resp).await)
    }
}
