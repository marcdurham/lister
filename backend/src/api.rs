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

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test as aw_test;
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn test_pool() -> PgPool {
        let url = std::env::var("LISTER_TEST_DB").unwrap_or_else(|_| {
            "postgres://marc@127.0.0.1:5433/lister_test".into()
        });
        PgPool::connect(&url).await.expect("connect to test db")
    }

    async fn seed_owner(pool: &PgPool) -> Uuid {
        let id = Uuid::new_v4();
        let email = format!("test-{}@example.com", id);
        sqlx::query!(
            "INSERT INTO users (id, email, password_hash, created_at) VALUES ($1, $2, $3, now())",
            id,
            email,
            "$argon2id$v=19$m=15000$...",
        )
        .execute(pool)
        .await
        .expect("insert user");
        id
    }

    async fn seed_item(pool: &PgPool, owner_id: Uuid, text: &str) -> Item {
        let item = Item::new(text.into(), false);
        sqlx::query!(
            "INSERT INTO items (id, text, notes, is_note, done, created_at, updated_at, deleted_at, owner_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
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
        .await
        .expect("insert item");
        item
    }

    #[tokio::test]
    async fn upsert_item_conflict_resolution_newer_wins() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let existing = seed_item(&pool, owner, "original").await;

        // Upsert with OLDER updated_at — should NOT overwrite.
        let stale = Item {
            updated_at: existing.updated_at - chrono::Duration::seconds(10),
            text: "stale".into(),
            ..existing.clone()
        };
        upsert_item(&pool, &stale, owner).await.unwrap();

        let row: Option<String> = sqlx::query_scalar!("SELECT text FROM items WHERE id = $1", existing.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(row.as_deref(), Some("original"));

        // Upsert with NEWER updated_at — should overwrite.
        let fresh = Item {
            updated_at: existing.updated_at + chrono::Duration::seconds(10),
            text: "fresh".into(),
            ..existing.clone()
        };
        upsert_item(&pool, &fresh, owner).await.unwrap();

        let row: Option<String> = sqlx::query_scalar!("SELECT text FROM items WHERE id = $1", existing.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(row.as_deref(), Some("fresh"));
    }

    #[tokio::test]
    async fn upsert_item_cross_owner_stale_ignored() {
        let pool = test_pool().await;
        let owner_a = seed_owner(&pool).await;
        let owner_b = seed_owner(&pool).await;
        let item = seed_item(&pool, owner_a, "a's item").await;

        // Owner B tries to upsert with newer timestamp — should NOT overwrite because
        // the WHERE clause requires items.owner_id == EXCLUDED.owner_id.
        let fresh = Item {
            updated_at: item.updated_at + chrono::Duration::seconds(10),
            text: "b's lie".into(),
            ..item.clone()
        };
        upsert_item(&pool, &fresh, owner_b).await.unwrap();

        let row: Option<String> = sqlx::query_scalar!("SELECT text FROM items WHERE id = $1", item.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(row.as_deref(), Some("a's item"));
    }

    #[tokio::test]
    async fn upsert_membership_ownership_check() {
        let pool = test_pool().await;
        let owner_a = seed_owner(&pool).await;
        let owner_b = seed_owner(&pool).await;
        let item_a = seed_item(&pool, owner_a, "owned by A").await;

        // Upsert membership for an item owned by someone else — should be silently dropped.
        let membership = Membership::new(item_a.id, None, 1.0);
        upsert_membership(&pool, &membership, owner_b).await.unwrap();

        let count: Option<i64> = sqlx::query_scalar!("SELECT count(*) FROM memberships WHERE id = $1", membership.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, Some(0), "cross-owner membership should be silently dropped");
    }

    #[tokio::test]
    async fn upsert_membership_own_item_works() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item = seed_item(&pool, owner, "mine").await;

        let membership = Membership::new(item.id, None, 2.5);
        upsert_membership(&pool, &membership, owner).await.unwrap();

        let count: Option<i64> = sqlx::query_scalar!("SELECT count(*) FROM memberships WHERE id = $1", membership.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, Some(1));
    }

    #[actix_web::test]
    async fn sync_endpoint_requires_auth() {
        let pool = test_pool().await;
        let app = aw_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(pool))
                .configure(|cfg| { cfg.service(sync); }),
        )
        .await;

        let req = aw_test::TestRequest::post()
            .uri("/api/sync")
            .set_json(serde_json::json!({"since": null, "items": [], "memberships": []}))
            .to_request();

        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn upsert_item_updates_deleted_at_when_newer() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item = seed_item(&pool, owner, "alive").await;

        // Mark as deleted with a newer timestamp.
        let deleted = Item {
            updated_at: item.updated_at + chrono::Duration::seconds(5),
            deleted_at: Some(Utc::now()),
            ..item.clone()
        };
        upsert_item(&pool, &deleted, owner).await.unwrap();

        let row: Option<Option<chrono::DateTime<chrono::Utc>>> = sqlx::query_scalar!(
            "SELECT deleted_at FROM items WHERE id = $1", item.id
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(row.flatten().is_some(), "deleted_at should be set after soft-delete upsert");
    }

    #[actix_web::test]
    async fn sync_endpoint_returns_200_with_valid_session() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        // Create a session so auth resolves.
        let session_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO sessions (id, user_id, created_at, expires_at) VALUES ($1, $2, now(), now() + interval '30 days')",
            session_id,
            owner,
        )
        .execute(&pool)
        .await
        .unwrap();

        let app = aw_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(pool))
                .configure(|cfg| { cfg.service(sync); }),
        )
        .await;

        let req = aw_test::TestRequest::post()
            .uri("/api/sync")
            .cookie(actix_web::cookie::Cookie::build(
                "lister_session",
                session_id.to_string(),
            ).finish())
            .set_json(serde_json::json!({"since": null, "items": [], "memberships": []}))
            .to_request();

        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn upsert_membership_conflict_resolution_newer_wins() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item = seed_item(&pool, owner, "ordered").await;

        // Seed an existing membership with position 1.0.
        let existing_m = Membership::new(item.id, None, 1.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, true, now(), now(), NULL)",
            existing_m.id,
            item.id,
            existing_m.parent_id,
            1.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Upsert with OLDER updated_at — position should stay at 1.0.
        let stale = Membership {
            updated_at: existing_m.updated_at - chrono::Duration::seconds(10),
            position: 99.0,
            ..existing_m.clone()
        };
        upsert_membership(&pool, &stale, owner).await.unwrap();

        let pos: Option<f64> = sqlx::query_scalar!("SELECT position FROM memberships WHERE id = $1", existing_m.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!((pos.unwrap() - 1.0).abs() < f64::EPSILON, "stale upsert should not change position");

        // Upsert with NEWER updated_at — position should update.
        let fresh = Membership {
            updated_at: existing_m.updated_at + chrono::Duration::seconds(10),
            position: 42.0,
            ..existing_m.clone()
        };
        upsert_membership(&pool, &fresh, owner).await.unwrap();

        let pos: Option<f64> = sqlx::query_scalar!("SELECT position FROM memberships WHERE id = $1", existing_m.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!((pos.unwrap() - 42.0).abs() < f64::EPSILON, "fresh upsert should update position");
    }

    #[actix_web::test]
    async fn sync_returns_seeded_data() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item = seed_item(&pool, owner, "sync me").await;
        let session_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO sessions (id, user_id, created_at, expires_at) VALUES ($1, $2, now(), now() + interval '30 days')",
            session_id,
            owner,
        )
        .execute(&pool)
        .await
        .unwrap();

        let app = aw_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(pool))
                .configure(|cfg| { cfg.service(sync); }),
        )
        .await;

        let req = aw_test::TestRequest::post()
            .uri("/api/sync")
            .cookie(actix_web::cookie::Cookie::build(
                "lister_session",
                session_id.to_string(),
            ).finish())
            .set_json(serde_json::json!({"since": null, "items": [], "memberships": []}))
            .to_request();

        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body: SyncResponse = serde_json::from_slice(&aw_test::read_body(resp).await).unwrap();
        assert!(!body.items.is_empty(), "should return at least one item");
        assert!(body.cursor > chrono::DateTime::UNIX_EPOCH,
            "cursor should be a real timestamp, not epoch zero");
    }

    #[tokio::test]
    async fn membership_visible_false_persists() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item = seed_item(&pool, owner, "hidden").await;

        let m = Membership::new(item.id, None, 0.0);
        upsert_membership(&pool, &m, owner).await.unwrap();

        // Flip visible to false via a fresh membership with newer timestamp.
        let hidden = Membership {
            updated_at: m.updated_at + chrono::Duration::seconds(1),
            visible: false,
            ..m.clone()
        };
        upsert_membership(&pool, &hidden, owner).await.unwrap();

        let vis: Option<bool> = sqlx::query_scalar!("SELECT visible FROM memberships WHERE id = $1", m.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(vis, Some(false), "visible=false should persist through upsert");
    }

    #[actix_web::test]
    async fn sync_filters_by_since() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        // Seed two items: one old, one new.
        let old_ts = Utc::now() - chrono::Duration::days(2);
        let old_item_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO items (id, text, notes, is_note, done, created_at, updated_at, deleted_at, owner_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8)"
        )
        .bind(old_item_id)
        .bind("old")
        .bind(None::<String>)
        .bind(false)
        .bind(false)
        .bind(old_ts.naive_utc())
        .bind(old_ts.naive_utc())
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();

        let new_item = seed_item(&pool, owner, "new").await;

        // Create session.
        let session_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO sessions (id, user_id, created_at, expires_at) VALUES ($1, $2, now(), now() + interval '30 days')",
            session_id,
            owner,
        )
        .execute(&pool)
        .await
        .unwrap();

        let app = aw_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(pool))
                .configure(|cfg| { cfg.service(sync); }),
        )
        .await;

        // Sync with since=now-1day should only return the new item.
        let since = Utc::now() - chrono::Duration::days(1);
        let req = aw_test::TestRequest::post()
            .uri("/api/sync")
            .cookie(actix_web::cookie::Cookie::build(
                "lister_session",
                session_id.to_string(),
            ).finish())
            .set_json(serde_json::json!({"since": since.to_rfc3339(), "items": [], "memberships": []}))
            .to_request();

        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body: SyncResponse = serde_json::from_slice(&aw_test::read_body(resp).await).unwrap();
        let texts: Vec<&str> = body.items.iter().map(|i| i.text.as_str()).collect();
        assert!(texts.contains(&"new"), "should include new item");
        assert!(!texts.contains(&"old"), "should exclude old item past since cursor");
    }

    #[actix_web::test]
    async fn sync_empty_db_returns_empty_items() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let session_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO sessions (id, user_id, created_at, expires_at) VALUES ($1, $2, now(), now() + interval '30 days')",
            session_id,
            owner,
        )
        .execute(&pool)
        .await
        .unwrap();

        let app = aw_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(pool))
                .configure(|cfg| { cfg.service(sync); }),
        )
        .await;

        let req = aw_test::TestRequest::post()
            .uri("/api/sync")
            .cookie(actix_web::cookie::Cookie::build(
                "lister_session",
                session_id.to_string(),
            ).finish())
            .set_json(serde_json::json!({"since": null, "items": [], "memberships": []}))
            .to_request();

        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body: SyncResponse = serde_json::from_slice(&aw_test::read_body(resp).await).unwrap();
        assert!(body.items.is_empty(), "empty DB should return empty items");
        assert!(body.memberships.is_empty(), "empty DB should return empty memberships");
    }

    #[tokio::test]
    async fn item_done_flag_updates_via_upsert() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let existing = seed_item(&pool, owner, "todo").await;

        // Mark as done with newer timestamp.
        let done = Item {
            updated_at: existing.updated_at + chrono::Duration::seconds(5),
            done: true,
            ..existing.clone()
        };
        upsert_item(&pool, &done, owner).await.unwrap();

        let done_flag: Option<bool> = sqlx::query_scalar!("SELECT done FROM items WHERE id = $1", existing.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(done_flag, Some(true), "done flag should update via upsert");
    }

    #[tokio::test]
    async fn cross_owner_membership_upsert_ignored() {
        let pool = test_pool().await;
        let owner_a = seed_owner(&pool).await;
        let owner_b = seed_owner(&pool).await;
        let item_a = seed_item(&pool, owner_a, "a's item").await;

        // Owner A creates a membership.
        let m = Membership::new(item_a.id, None, 1.0);
        upsert_membership(&pool, &m, owner_a).await.unwrap();

        // Owner B tries to update with newer timestamp — should be ignored.
        let fresh = Membership {
            updated_at: m.updated_at + chrono::Duration::seconds(10),
            position: 99.0,
            ..m.clone()
        };
        upsert_membership(&pool, &fresh, owner_b).await.unwrap();

        let pos: Option<f64> = sqlx::query_scalar!("SELECT position FROM memberships WHERE id = $1", m.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!((pos.unwrap() - 1.0).abs() < f64::EPSILON, "cross-owner upsert should not change position");
    }

    #[actix_web::test]
    async fn invalid_session_returns_401() {
        let pool = test_pool().await;
        let app = aw_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(pool))
                .configure(|cfg| { cfg.service(sync); }),
        )
        .await;

        let req = aw_test::TestRequest::post()
            .uri("/api/sync")
            .cookie(actix_web::cookie::Cookie::build(
                "lister_session",
                Uuid::new_v4().to_string(),
            ).finish())
            .set_json(serde_json::json!({"since": null, "items": [], "memberships": []}))
            .to_request();

        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn membership_position_updates_via_upsert() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item = seed_item(&pool, owner, "reorder").await;

        // Seed initial membership.
        let m = Membership::new(item.id, None, 5.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, true, now(), now(), NULL)",
            m.id,
            item.id,
            m.parent_id,
            5.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Upsert with newer timestamp and different position.
        let updated = Membership {
            updated_at: m.updated_at + chrono::Duration::seconds(1),
            position: 7.5,
            ..m.clone()
        };
        upsert_membership(&pool, &updated, owner).await.unwrap();

        let pos: Option<f64> = sqlx::query_scalar!("SELECT position FROM memberships WHERE id = $1", m.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!((pos.unwrap() - 7.5).abs() < f64::EPSILON, "position should update to 7.5");
    }

    #[tokio::test]
    async fn item_notes_persist_through_db() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let notes_json = serde_json::json!({"highlight": "red", "priority": 1});
        let item_id = Uuid::new_v4();

        let now = Utc::now();
        sqlx::query(
            "INSERT INTO items (id, text, notes, is_note, done, created_at, updated_at, owner_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(item_id)
        .bind("with notes")
        .bind(notes_json.to_string())
        .bind(false)
        .bind(false)
        .bind(now.naive_utc())
        .bind(now.naive_utc())
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();

        let notes: Option<Option<String>> = sqlx::query_scalar!("SELECT notes FROM items WHERE id = $1", item_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        let notes_str = notes.flatten().expect("notes should persist");
        let parsed: serde_json::Value = serde_json::from_str(&notes_str).unwrap();
        assert_eq!(parsed["highlight"], "red");
        assert_eq!(parsed["priority"], 1);
    }
}
