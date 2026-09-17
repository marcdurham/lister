use actix_web::cookie::{Cookie, SameSite};
use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::{Duration, Utc};
use log;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

const SESSION_COOKIE: &str = "lister_session";
const SESSION_DAYS: i64 = 30;

#[derive(Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UserView {
    pub id: Uuid,
    pub email: String,
    pub is_admin: bool,
}

#[derive(Serialize)]
pub struct PendingView {
    pub status: &'static str,
}

/// Emails from DEFAULT_ADMINISTRATORS (comma-separated), lowercased and trimmed. These
/// accounts are auto-approved as admins the moment they register, and re-synced to admin
/// status on every server start in case they registered before being added to the list.
fn default_admin_emails() -> Vec<String> {
    std::env::var("DEFAULT_ADMINISTRATORS")
        .unwrap_or_default()
        .split(',')
        .map(|e| e.trim().to_lowercase())
        .filter(|e| !e.is_empty())
        .collect()
}

pub async fn seed_default_admins(pool: &PgPool) {
    let emails = default_admin_emails();
    if emails.is_empty() {
        return;
    }
    if let Err(err) = sqlx::query!(
        "UPDATE users SET is_admin = true, status = 'approved' WHERE email = ANY($1)",
        &emails,
    )
    .execute(pool)
    .await
    {
        log::warn!("failed to seed default admins: {err}");
    }
}

fn hash_password(password: &str) -> Result<String, ()> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| ())
}

fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

async fn create_session(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Uuid> {
    let id = Uuid::new_v4();
    let now = Utc::now();
    let expires_at = now + Duration::days(SESSION_DAYS);
    sqlx::query!(
        "INSERT INTO sessions (id, user_id, created_at, expires_at) VALUES ($1, $2, $3, $4)",
        id,
        user_id,
        now,
        expires_at,
    )
    .execute(pool)
    .await?;
    Ok(id)
}

fn session_cookie(session_id: Uuid) -> Cookie<'static> {
    Cookie::build(SESSION_COOKIE, session_id.to_string())
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(actix_web::cookie::time::Duration::days(SESSION_DAYS))
        .finish()
}

/// Resolve the logged-in user from the session cookie, if any and not expired.
pub async fn current_user_id(req: &HttpRequest, pool: &PgPool) -> Option<Uuid> {
    let cookie = req.cookie(SESSION_COOKIE)?;
    let session_id = Uuid::parse_str(cookie.value()).ok()?;
    let row = sqlx::query!(
        "SELECT user_id FROM sessions WHERE id = $1 AND expires_at > now()",
        session_id,
    )
    .fetch_optional(pool)
    .await
    .ok()??;
    Some(row.user_id)
}

#[post("/api/auth/register")]
pub async fn register(pool: web::Data<PgPool>, body: web::Json<Credentials>) -> impl Responder {
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || body.password.len() < 8 {
        return HttpResponse::BadRequest()
            .body("email is required and password must be at least 8 characters");
    }
    let Ok(password_hash) = hash_password(&body.password) else {
        return HttpResponse::InternalServerError().body("failed to hash password");
    };

    let user_id = Uuid::new_v4();
    let now = Utc::now();
    // The site is invite-only: anyone matching DEFAULT_ADMINISTRATORS is auto-approved as
    // an admin, everyone else lands as "pending" until an admin approves them.
    let is_admin = default_admin_emails().contains(&email);
    let status = if is_admin { "approved" } else { "pending" };
    let inserted = sqlx::query!(
        "INSERT INTO users (id, email, password_hash, created_at, is_admin, status) VALUES ($1, $2, $3, $4, $5, $6)",
        user_id,
        email,
        password_hash,
        now,
        is_admin,
        status,
    )
    .execute(pool.get_ref())
    .await;

    if let Err(err) = inserted {
        if let Some(db_err) = err.as_database_error() {
            if db_err.is_unique_violation() {
                return HttpResponse::Conflict().body("an account with that email already exists");
            }
        }
        return HttpResponse::InternalServerError().body(format!("register failed: {err}"));
    }

    // The very first account claims any pre-auth (legacy) data so it isn't orphaned.
    let user_count = sqlx::query_scalar!("SELECT count(*) FROM users")
        .fetch_one(pool.get_ref())
        .await
        .ok()
        .flatten();
    if user_count == Some(1) {
        let _ = sqlx::query!(
            "UPDATE items SET owner_id = $1 WHERE owner_id IS NULL",
            user_id
        )
        .execute(pool.get_ref())
        .await;
    }

    if status != "approved" {
        return HttpResponse::Accepted().json(PendingView { status: "pending" });
    }

    let session_id = match create_session(pool.get_ref(), user_id).await {
        Ok(id) => id,
        Err(err) => return HttpResponse::InternalServerError().body(format!("session failed: {err}")),
    };

    HttpResponse::Ok()
        .cookie(session_cookie(session_id))
        .json(UserView {
            id: user_id,
            email,
            is_admin,
        })
}

#[post("/api/auth/login")]
pub async fn login(pool: web::Data<PgPool>, body: web::Json<Credentials>) -> impl Responder {
    let email = body.email.trim().to_lowercase();
    let user = sqlx::query!(
        "SELECT id, password_hash, is_admin, status FROM users WHERE email = $1",
        email,
    )
    .fetch_optional(pool.get_ref())
    .await;

    let user = match user {
        Ok(Some(u)) => u,
        Ok(None) => return HttpResponse::Unauthorized().body("invalid email or password"),
        Err(err) => return HttpResponse::InternalServerError().body(format!("login failed: {err}")),
    };

    if !verify_password(&body.password, &user.password_hash) {
        return HttpResponse::Unauthorized().body("invalid email or password");
    }

    match user.status.as_str() {
        "disabled" => return HttpResponse::Forbidden().body("This account has been disabled."),
        "pending" => {
            return HttpResponse::Forbidden()
                .body("Your account is awaiting administrator approval.")
        }
        _ => {}
    }

    let session_id = match create_session(pool.get_ref(), user.id).await {
        Ok(id) => id,
        Err(err) => return HttpResponse::InternalServerError().body(format!("session failed: {err}")),
    };

    HttpResponse::Ok().cookie(session_cookie(session_id)).json(UserView {
        id: user.id,
        email,
        is_admin: user.is_admin,
    })
}

#[post("/api/auth/logout")]
pub async fn logout(pool: web::Data<PgPool>, req: HttpRequest) -> impl Responder {
    if let Some(cookie) = req.cookie(SESSION_COOKIE) {
        if let Ok(session_id) = Uuid::parse_str(cookie.value()) {
            let _ = sqlx::query!("DELETE FROM sessions WHERE id = $1", session_id)
                .execute(pool.get_ref())
                .await;
        }
    }
    let mut removal = Cookie::build(SESSION_COOKIE, "").path("/").finish();
    removal.make_removal();
    HttpResponse::Ok().cookie(removal).finish()
}

#[get("/api/auth/me")]
pub async fn me(pool: web::Data<PgPool>, req: HttpRequest) -> impl Responder {
    let Some(user_id) = current_user_id(&req, pool.get_ref()).await else {
        return HttpResponse::Unauthorized().finish();
    };
    match sqlx::query!(
        "SELECT email, is_admin, status FROM users WHERE id = $1",
        user_id
    )
    .fetch_optional(pool.get_ref())
    .await
    {
        Ok(Some(row)) if row.status == "approved" => HttpResponse::Ok().json(UserView {
            id: user_id,
            email: row.email,
            is_admin: row.is_admin,
        }),
        _ => HttpResponse::Unauthorized().finish(),
    }
}
