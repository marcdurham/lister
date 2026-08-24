use actix_web::{post, web, HttpRequest, HttpResponse, Responder};
use chrono::{DateTime, Utc};
use shared::{Item, Membership, SyncRequest, SyncResponse};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth;

fn epoch() -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(0, 0).expect("valid epoch timestamp")
}

async fn upsert_item(pool: &PgPool, item: &Item, owner_id: Uuid) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO items (id, text, notes, is_note, done, created_at, updated_at, deleted_at, owner_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (id) DO UPDATE SET
            text = EXCLUDED.text,
            notes = EXCLUDED.notes,
            is_note = EXCLUDED.is_note,
            done = EXCLUDED.done,
            updated_at = EXCLUDED.updated_at,
            deleted_at = EXCLUDED.deleted_at
        WHERE items.updated_at < EXCLUDED.updated_at AND items.owner_id = EXCLUDED.owner_id
        "#,
        item.id,
        item.text,
        item.notes,
        item.is_note,
        item.done,
        item.created_at,
        item.updated_at,
        item.deleted_at,
        owner_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert/update a membership, but only if the caller owns the item it points at -
/// otherwise it's silently dropped rather than letting one account graft rows onto another's items.
async fn upsert_membership(pool: &PgPool, membership: &Membership, owner_id: Uuid) -> sqlx::Result<()> {
    let item_owner = sqlx::query_scalar!(
        "SELECT owner_id FROM items WHERE id = $1",
        membership.item_id
    )
    .fetch_optional(pool)
    .await?
    .flatten();

    if item_owner != Some(owner_id) {
        return Ok(());
    }

    sqlx::query!(
        r#"
        INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (id) DO UPDATE SET
            item_id = EXCLUDED.item_id,
            parent_id = EXCLUDED.parent_id,
            position = EXCLUDED.position,
            visible = EXCLUDED.visible,
            updated_at = EXCLUDED.updated_at,
            deleted_at = EXCLUDED.deleted_at
        WHERE memberships.updated_at < EXCLUDED.updated_at
        "#,
        membership.id,
        membership.item_id,
        membership.parent_id,
        membership.position,
        membership.visible,
        membership.created_at,
        membership.updated_at,
        membership.deleted_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[post("/api/sync")]
async fn sync(pool: web::Data<PgPool>, req: HttpRequest, body: web::Json<SyncRequest>) -> impl Responder {
    let Some(owner_id) = auth::current_user_id(&req, pool.get_ref()).await else {
        return HttpResponse::Unauthorized().finish();
    };

    let body = body.into_inner();
    let since = body.since.unwrap_or_else(epoch);

    for item in &body.items {
        if let Err(err) = upsert_item(&pool, item, owner_id).await {
            return HttpResponse::InternalServerError().body(format!("sync item failed: {err}"));
        }
    }
    for membership in &body.memberships {
        if let Err(err) = upsert_membership(&pool, membership, owner_id).await {
            return HttpResponse::InternalServerError()
                .body(format!("sync membership failed: {err}"));
        }
    }

    let items = match sqlx::query_as!(
        Item,
        r#"SELECT id, text, notes, is_note, done, created_at, updated_at, deleted_at
           FROM items WHERE updated_at > $1 AND owner_id = $2"#,
        since,
        owner_id,
    )
    .fetch_all(pool.get_ref())
    .await
    {
        Ok(rows) => rows,
        Err(err) => return HttpResponse::InternalServerError().body(format!("pull items failed: {err}")),
    };

    let memberships = match sqlx::query_as!(
        Membership,
        r#"SELECT m.id, m.item_id, m.parent_id, m.position, m.visible, m.created_at, m.updated_at, m.deleted_at
           FROM memberships m
           JOIN items i ON i.id = m.item_id
           WHERE m.updated_at > $1 AND i.owner_id = $2"#,
        since,
        owner_id,
    )
    .fetch_all(pool.get_ref())
    .await
    {
        Ok(rows) => rows,
        Err(err) => {
            return HttpResponse::InternalServerError().body(format!("pull memberships failed: {err}"))
        }
    };

    HttpResponse::Ok().json(SyncResponse {
        items,
        memberships,
        cursor: Utc::now(),
    })
}
