use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub is_admin: bool,
}

/// What happens right after submitting the registration form: either the account is a
/// default admin and is logged straight in, or it's invite-only and waits for approval.
pub enum RegisterOutcome {
    LoggedIn(User),
    Pending,
}

#[derive(Serialize)]
struct Credentials<'a> {
    email: &'a str,
    password: &'a str,
}

async fn error_message(resp: gloo_net::http::Response) -> String {
    resp.text()
        .await
        .unwrap_or_else(|_| "request failed".to_string())
}

pub async fn me() -> Option<User> {
    let resp = Request::get("/api/auth/me").send().await.ok()?;
    if !resp.ok() {
        return None;
    }
    resp.json().await.ok()
}

pub async fn login(email: &str, password: &str) -> Result<User, String> {
    let resp = Request::post("/api/auth/login")
        .json(&Credentials { email, password })
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

pub async fn register(email: &str, password: &str) -> Result<RegisterOutcome, String> {
    let resp = Request::post("/api/auth/register")
        .json(&Credentials { email, password })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == 202 {
        Ok(RegisterOutcome::Pending)
    } else if resp.ok() {
        resp.json()
            .await
            .map(RegisterOutcome::LoggedIn)
            .map_err(|e| e.to_string())
    } else {
        Err(error_message(resp).await)
    }
}

pub async fn logout() {
    let _ = Request::post("/api/auth/logout").send().await;
}
