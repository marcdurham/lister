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
