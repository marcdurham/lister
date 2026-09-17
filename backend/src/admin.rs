use actix_web::{delete, get, post, web, HttpRequest, HttpResponse, Responder};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth;

#[derive(Serialize)]
pub struct AdminUserView {
    pub id: Uuid,
    pub email: String,
    pub status: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

const VALID_STATUSES: [&str; 3] = ["pending", "approved", "disabled"];

/// Resolve the caller as an approved admin, or None.
async fn require_admin(req: &HttpRequest, pool: &PgPool) -> Option<Uuid> {
    let user_id = auth::current_user_id(req, pool).await?;
    let row = sqlx::query!("SELECT is_admin, status FROM users WHERE id = $1", user_id)
        .fetch_optional(pool)
        .await
        .ok()??;
    (row.is_admin && row.status == "approved").then_some(user_id)
}

#[get("/api/admin/users")]
pub async fn list_users(pool: web::Data<PgPool>, req: HttpRequest) -> impl Responder {
    if require_admin(&req, pool.get_ref()).await.is_none() {
        return HttpResponse::Forbidden().finish();
    }
    let rows = sqlx::query_as!(
        AdminUserView,
        r#"SELECT id, email, status, is_admin, created_at FROM users ORDER BY created_at ASC"#
    )
    .fetch_all(pool.get_ref())
    .await;
    match rows {
        Ok(rows) => HttpResponse::Ok().json(rows),
        Err(err) => HttpResponse::InternalServerError().body(format!("list users failed: {err}")),
    }
}

#[derive(Deserialize)]
pub struct SetStatusBody {
    pub status: String,
}

#[post("/api/admin/users/{id}/status")]
pub async fn set_status(
    pool: web::Data<PgPool>,
    req: HttpRequest,
    path: web::Path<Uuid>,
    body: web::Json<SetStatusBody>,
) -> impl Responder {
    let Some(admin_id) = require_admin(&req, pool.get_ref()).await else {
        return HttpResponse::Forbidden().finish();
    };
    let target_id = path.into_inner();
    if !VALID_STATUSES.contains(&body.status.as_str()) {
        return HttpResponse::BadRequest().body("invalid status");
    }
    if target_id == admin_id && body.status != "approved" {
        return HttpResponse::BadRequest().body("cannot change your own status");
    }

    match sqlx::query!(
        "UPDATE users SET status = $1 WHERE id = $2",
        body.status,
        target_id,
    )
    .execute(pool.get_ref())
    .await
    {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(err) => HttpResponse::InternalServerError().body(format!("update failed: {err}")),
    }
}

#[derive(Deserialize)]
pub struct SetAdminBody {
    pub is_admin: bool,
}

#[post("/api/admin/users/{id}/admin")]
pub async fn set_admin(
    pool: web::Data<PgPool>,
    req: HttpRequest,
    path: web::Path<Uuid>,
    body: web::Json<SetAdminBody>,
) -> impl Responder {
    let Some(admin_id) = require_admin(&req, pool.get_ref()).await else {
        return HttpResponse::Forbidden().finish();
    };
    let target_id = path.into_inner();
    if target_id == admin_id && !body.is_admin {
        return HttpResponse::BadRequest().body("cannot remove your own admin access");
    }

    match sqlx::query!(
        "UPDATE users SET is_admin = $1 WHERE id = $2",
        body.is_admin,
        target_id,
    )
    .execute(pool.get_ref())
    .await
    {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(err) => HttpResponse::InternalServerError().body(format!("update failed: {err}")),
    }
}

#[delete("/api/admin/users/{id}")]
pub async fn delete_user(
    pool: web::Data<PgPool>,
    req: HttpRequest,
    path: web::Path<Uuid>,
) -> impl Responder {
    let Some(admin_id) = require_admin(&req, pool.get_ref()).await else {
        return HttpResponse::Forbidden().finish();
    };
    let target_id = path.into_inner();
    if target_id == admin_id {
        return HttpResponse::BadRequest().body("cannot delete your own account");
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => return HttpResponse::InternalServerError().body(format!("tx failed: {err}")),
    };

    if let Err(err) = sqlx::query!(
        "DELETE FROM memberships WHERE item_id IN (SELECT id FROM items WHERE owner_id = $1)",
        target_id
    )
    .execute(&mut *tx)
    .await
    {
        return HttpResponse::InternalServerError().body(format!("delete failed: {err}"));
    }
    if let Err(err) = sqlx::query!("DELETE FROM items WHERE owner_id = $1", target_id)
        .execute(&mut *tx)
        .await
    {
        return HttpResponse::InternalServerError().body(format!("delete failed: {err}"));
    }
    if let Err(err) = sqlx::query!("DELETE FROM users WHERE id = $1", target_id)
        .execute(&mut *tx)
        .await
    {
        return HttpResponse::InternalServerError().body(format!("delete failed: {err}"));
    }

    if let Err(err) = tx.commit().await {
        return HttpResponse::InternalServerError().body(format!("commit failed: {err}"));
    }
    HttpResponse::Ok().finish()
}
