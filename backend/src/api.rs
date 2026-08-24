use actix_web::{post, web, HttpResponse, Responder};
use chrono::{DateTime, Utc};
use shared::{Item, Membership, SyncRequest, SyncResponse};
use sqlx::PgPool;

fn epoch() -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(0, 0).expect("valid epoch timestamp")
}

async fn upsert_item(pool: &PgPool, item: &Item) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO items (id, text, notes, is_note, done, created_at, updated_at, deleted_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (id) DO UPDATE SET
            text = EXCLUDED.text,
            notes = EXCLUDED.notes,
            is_note = EXCLUDED.is_note,
            done = EXCLUDED.done,
            updated_at = EXCLUDED.updated_at,
            deleted_at = EXCLUDED.deleted_at
        WHERE items.updated_at < EXCLUDED.updated_at
        "#,
        item.id,
        item.text,
        item.notes,
        item.is_note,
        item.done,
        item.created_at,
        item.updated_at,
        item.deleted_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn upsert_membership(pool: &PgPool, membership: &Membership) -> sqlx::Result<()> {
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
async fn sync(pool: web::Data<PgPool>, body: web::Json<SyncRequest>) -> impl Responder {
    let req = body.into_inner();
    let since = req.since.unwrap_or_else(epoch);

    for item in &req.items {
        if let Err(err) = upsert_item(&pool, item).await {
            return HttpResponse::InternalServerError().body(format!("sync item failed: {err}"));
        }
    }
    for membership in &req.memberships {
        if let Err(err) = upsert_membership(&pool, membership).await {
            return HttpResponse::InternalServerError()
                .body(format!("sync membership failed: {err}"));
        }
    }

    let items = match sqlx::query_as!(
        Item,
        r#"SELECT id, text, notes, is_note, done, created_at, updated_at, deleted_at
           FROM items WHERE updated_at > $1"#,
        since,
    )
    .fetch_all(pool.get_ref())
    .await
    {
        Ok(rows) => rows,
        Err(err) => return HttpResponse::InternalServerError().body(format!("pull items failed: {err}")),
    };

    let memberships = match sqlx::query_as!(
        Membership,
        r#"SELECT id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at
           FROM memberships WHERE updated_at > $1"#,
        since,
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
