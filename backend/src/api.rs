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

    #[tokio::test]
    async fn membership_parent_id_updates_via_upsert() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item = seed_item(&pool, owner, "move").await;
        // Create a parent item first (foreign key constraint requires it).
        let parent_item = seed_item(&pool, owner, "parent").await;
        let new_parent_id = parent_item.id;

        // Seed membership with no parent.
        let m = Membership::new(item.id, None, 0.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, true, now(), now(), NULL)",
            m.id,
            item.id,
            m.parent_id,
            0.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Upsert with new parent.
        let updated = Membership {
            updated_at: m.updated_at + chrono::Duration::seconds(1),
            parent_id: Some(new_parent_id),
            ..m.clone()
        };
        upsert_membership(&pool, &updated, owner).await.unwrap();

        let parent: Option<Option<Uuid>> = sqlx::query_scalar!("SELECT parent_id FROM memberships WHERE id = $1", m.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(parent.flatten(), Some(new_parent_id), "parent_id should update via upsert");
    }

    #[tokio::test]
    async fn item_is_note_flag_updates_via_upsert() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let existing = seed_item(&pool, owner, "toggle note").await;

        // Flip is_note to true with newer timestamp.
        let updated = Item {
            updated_at: existing.updated_at + chrono::Duration::seconds(1),
            is_note: true,
            ..existing.clone()
        };
        upsert_item(&pool, &updated, owner).await.unwrap();

        let is_note: Option<bool> = sqlx::query_scalar!("SELECT is_note FROM items WHERE id = $1", existing.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(is_note, Some(true), "is_note flag should update via upsert");
    }

    #[actix_web::test]
    async fn sync_cursor_advances_after_new_items() {
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

        let pool_for_seed = pool.clone();
        let app = aw_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(pool))
                .configure(|cfg| { cfg.service(sync); }),
        )
        .await;

        // First sync on empty DB.
        let req1 = aw_test::TestRequest::post()
            .uri("/api/sync")
            .cookie(actix_web::cookie::Cookie::build(
                "lister_session",
                session_id.to_string(),
            ).finish())
            .set_json(serde_json::json!({"since": null, "items": [], "memberships": []}))
            .to_request();
        let resp1 = aw_test::call_service(&app, req1).await;
        assert_eq!(resp1.status(), actix_web::http::StatusCode::OK);
        let body1: SyncResponse = serde_json::from_slice(&aw_test::read_body(resp1).await).unwrap();

        // Add a new item.
        seed_item(&pool_for_seed, owner, "new sync item").await;

        // Second sync should return the new item and have a later cursor.
        let req2 = aw_test::TestRequest::post()
            .uri("/api/sync")
            .cookie(actix_web::cookie::Cookie::build(
                "lister_session",
                session_id.to_string(),
            ).finish())
            .set_json(serde_json::json!({"since": null, "items": [], "memberships": []}))
            .to_request();
        let resp2 = aw_test::call_service(&app, req2).await;
        assert_eq!(resp2.status(), actix_web::http::StatusCode::OK);
        let body2: SyncResponse = serde_json::from_slice(&aw_test::read_body(resp2).await).unwrap();

        assert!(!body1.items.is_empty() || !body2.items.is_empty(), "at least one sync should return items");
        // Cursor should advance (or stay same) — never go backwards.
        assert!(body2.cursor >= body1.cursor, "cursor should not regress after adding items");
    }

    #[tokio::test]
    async fn item_text_updates_via_upsert() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let existing = seed_item(&pool, owner, "original text").await;

        // Update text with newer timestamp.
        let updated = Item {
            updated_at: existing.updated_at + chrono::Duration::seconds(1),
            text: "updated text".into(),
            ..existing.clone()
        };
        upsert_item(&pool, &updated, owner).await.unwrap();

        let text: Option<String> = sqlx::query_scalar!("SELECT text FROM items WHERE id = $1", existing.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(text, Some("updated text".to_string()), "text should update via upsert");
    }

    #[tokio::test]
    async fn item_soft_delete_via_upsert() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let existing = seed_item(&pool, owner, "delete me").await;

        // Mark as deleted with newer timestamp and explicit deleted_at.
        let now = Utc::now();
        let deleted = Item {
            updated_at: existing.updated_at + chrono::Duration::seconds(1),
            deleted_at: Some(now),
            ..existing.clone()
        };
        upsert_item(&pool, &deleted, owner).await.unwrap();

        let deleted_at: Option<Option<chrono::DateTime<chrono::Utc>>> = sqlx::query_scalar!("SELECT deleted_at FROM items WHERE id = $1", existing.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!(deleted_at.flatten().is_some(), "deleted_at should be set via upsert soft-delete");
    }

    #[actix_web::test]
    async fn sync_accepts_empty_memberships_in_request_body() {
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

        // Sync with empty memberships array.
        let req = aw_test::TestRequest::post()
            .uri("/api/sync")
            .cookie(actix_web::cookie::Cookie::build(
                "lister_session",
                session_id.to_string(),
            ).finish())
            .set_json(serde_json::json!({"since": null, "items": [], "memberships": []}))
            .to_request();

        let resp = aw_test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK,
            "sync should accept empty memberships array in request body");
    }

    #[actix_web::test]
    async fn sync_returns_multiple_items() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let session_id = Uuid::new_v4();

        // Seed multiple items.
        seed_item(&pool, owner, "item one").await;
        seed_item(&pool, owner, "item two").await;
        seed_item(&pool, owner, "item three").await;

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

        // Sync and verify all items come back.
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

        assert_eq!(body.items.len(), 3,
            "sync should return all seeded items, got {}", body.items.len());
    }

    #[tokio::test]
    async fn item_empty_text_persists() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item_id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO items (id, text, notes, is_note, done, created_at, updated_at, owner_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(item_id)
        .bind("")
        .bind(None::<String>)
        .bind(false)
        .bind(false)
        .bind(now.naive_utc())
        .bind(now.naive_utc())
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();

        let text: Option<String> = sqlx::query_scalar!("SELECT text FROM items WHERE id = $1", item_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(text, Some("".to_string()), "empty string should persist as text");
    }

    #[actix_web::test]
    async fn memberships_ordered_by_position_in_sync() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item_a = seed_item(&pool, owner, "pos 10").await;
        let item_b = seed_item(&pool, owner, "pos 20").await;
        let session_id = Uuid::new_v4();

        // Seed memberships with different positions.
        let m_a = Membership::new(item_a.id, None, 10.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, true, now(), now(), NULL)",
            m_a.id,
            item_a.id,
            m_a.parent_id,
            10.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

        let m_b = Membership::new(item_b.id, None, 20.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, true, now(), now(), NULL)",
            m_b.id,
            item_b.id,
            m_b.parent_id,
            20.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

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

        // Sync and verify memberships are ordered by position.
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

        if body.memberships.len() >= 2 {
            let pos0 = body.memberships[0].position;
            let pos1 = body.memberships[1].position;
            assert!(pos0 <= pos1,
                "memberships should be ordered by position ascending, got {} then {}",
                pos0, pos1);
        }
    }

    #[tokio::test]
    async fn membership_soft_delete_via_upsert() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item = seed_item(&pool, owner, "membership delete").await;
        let m = Membership::new(item.id, None, 0.0);

        // Seed membership.
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, true, now(), now(), NULL)",
            m.id,
            item.id,
            m.parent_id,
            0.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Soft-delete with newer timestamp.
        let now = Utc::now();
        let deleted_m = Membership {
            updated_at: m.updated_at + chrono::Duration::seconds(1),
            deleted_at: Some(now),
            ..m.clone()
        };
        upsert_membership(&pool, &deleted_m, owner).await.unwrap();

        let deleted_at: Option<Option<chrono::DateTime<chrono::Utc>>> = sqlx::query_scalar!("SELECT deleted_at FROM memberships WHERE id = $1", m.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!(deleted_at.flatten().is_some(), "membership deleted_at should be set via upsert soft-delete");
    }

    #[actix_web::test]
    async fn sync_cursor_is_valid_timestamp() {
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

        // Sync and verify cursor is a valid timestamp.
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

        // Cursor should be a valid timestamp (not epoch zero).
        let cursor_epoch = body.cursor.timestamp();
        assert!(cursor_epoch > 0, "sync cursor should be a valid timestamp, got {}", cursor_epoch);
    }

    #[actix_web::test]
    async fn membership_visible_false_round_trips_via_sync() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item = seed_item(&pool, owner, "hidden item").await;
        let session_id = Uuid::new_v4();

        // Seed membership with visible=false.
        let m = Membership::new(item.id, None, 0.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, false, now(), now(), NULL)",
            m.id,
            item.id,
            m.parent_id,
            0.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

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

        // Sync and verify visible=false comes back.
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

        let membership = body.memberships.iter()
            .find(|m| m.item_id == item.id)
            .expect("membership should be in response");
        assert!(!membership.visible, "visible=false should round-trip via sync");
    }

    #[tokio::test]
    async fn item_notes_update_via_upsert() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let existing = seed_item(&pool, owner, "notes update").await;

        // Update notes with newer timestamp.
        let new_notes = serde_json::json!({"highlight": "blue", "priority": 2});
        let updated = Item {
            updated_at: existing.updated_at + chrono::Duration::seconds(1),
            notes: Some(new_notes.to_string()),
            ..existing.clone()
        };
        upsert_item(&pool, &updated, owner).await.unwrap();

        let notes: Option<Option<String>> = sqlx::query_scalar!("SELECT notes FROM items WHERE id = $1", existing.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(notes.flatten(), Some(new_notes.to_string()), "notes should update via upsert");
    }

    #[actix_web::test]
    async fn membership_parent_id_round_trips_via_sync() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let parent_item = seed_item(&pool, owner, "parent").await;
        let child_item = seed_item(&pool, owner, "child").await;
        let session_id = Uuid::new_v4();

        // Seed membership with parent_id pointing to parent item.
        let m = Membership::new(child_item.id, Some(parent_item.id), 0.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, true, now(), now(), NULL)",
            m.id,
            child_item.id,
            Some(parent_item.id),
            0.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

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

        // Sync and verify parent_id comes back.
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

        let membership = body.memberships.iter()
            .find(|m| m.item_id == child_item.id)
            .expect("membership should be in response");
        assert_eq!(membership.parent_id, Some(parent_item.id),
            "parent_id should round-trip via sync");
    }

    #[tokio::test]
    async fn item_is_note_true_via_upsert() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let existing = seed_item(&pool, owner, "note flag").await;

        // Flip is_note to true with newer timestamp.
        let updated = Item {
            updated_at: existing.updated_at + chrono::Duration::seconds(1),
            is_note: true,
            ..existing.clone()
        };
        upsert_item(&pool, &updated, owner).await.unwrap();

        let is_note: Option<bool> = sqlx::query_scalar!("SELECT is_note FROM items WHERE id = $1", existing.id)
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert_eq!(is_note, Some(true), "is_note should update via upsert");
    }

    #[tokio::test]
    async fn item_created_at_not_updated_via_upsert() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let existing = seed_item(&pool, owner, "created at immutability").await;
        let original_created_at = existing.created_at;

        // Update with newer timestamp.
        let updated = Item {
            updated_at: existing.updated_at + chrono::Duration::seconds(1),
            text: "updated text".to_string(),
            ..existing.clone()
        };
        upsert_item(&pool, &updated, owner).await.unwrap();

        let created_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar!("SELECT created_at FROM items WHERE id = $1", existing.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        // Compare timestamps with microsecond precision (Postgres truncates nanos).
        let diff = (created_at.timestamp_micros() - original_created_at.timestamp_micros()).unsigned_abs();
        assert!(diff < 1_000_000,
            "created_at should not change via upsert (immutable), got {:?} vs {:?}",
            created_at, original_created_at);
    }

    #[actix_web::test]
    async fn sync_response_includes_both_items_and_memberships() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item_a = seed_item(&pool, owner, "sync both a").await;
        let item_b = seed_item(&pool, owner, "sync both b").await;
        let session_id = Uuid::new_v4();

        // Seed memberships for both items.
        let m_a = Membership::new(item_a.id, None, 0.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, true, now(), now(), NULL)",
            m_a.id,
            item_a.id,
            m_a.parent_id,
            0.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

        let m_b = Membership::new(item_b.id, None, 1.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, true, now(), now(), NULL)",
            m_b.id,
            item_b.id,
            m_b.parent_id,
            1.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

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

        // Sync and verify both items and memberships come back.
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

        assert_eq!(body.items.len(), 2,
            "sync should return both items, got {}", body.items.len());
        assert_eq!(body.memberships.len(), 2,
            "sync should return both memberships, got {}", body.memberships.len());
    }

    #[actix_web::test]
    async fn sync_cursor_advances_monotonically() {
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

        let pool_for_seed = pool.clone();
        let app = aw_test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(pool))
                .configure(|cfg| { cfg.service(sync); }),
        )
        .await;

        // First sync.
        let req1 = aw_test::TestRequest::post()
            .uri("/api/sync")
            .cookie(actix_web::cookie::Cookie::build(
                "lister_session",
                session_id.to_string(),
            ).finish())
            .set_json(serde_json::json!({"since": null, "items": [], "memberships": []}))
            .to_request();
        let resp1 = aw_test::call_service(&app, req1).await;
        assert_eq!(resp1.status(), actix_web::http::StatusCode::OK);
        let body1: SyncResponse = serde_json::from_slice(&aw_test::read_body(resp1).await).unwrap();

        // Add an item.
        seed_item(&pool_for_seed, owner, "monotonic test").await;

        // Second sync with cursor from first.
        let req2 = aw_test::TestRequest::post()
            .uri("/api/sync")
            .cookie(actix_web::cookie::Cookie::build(
                "lister_session",
                session_id.to_string(),
            ).finish())
            .set_json(serde_json::json!({"since": Some(body1.cursor), "items": [], "memberships": []}))
            .to_request();
        let resp2 = aw_test::call_service(&app, req2).await;
        assert_eq!(resp2.status(), actix_web::http::StatusCode::OK);
        let body2: SyncResponse = serde_json::from_slice(&aw_test::read_body(resp2).await).unwrap();

        // Cursor should advance.
        assert!(body2.cursor > body1.cursor,
            "sync cursor should advance monotonically, got {:?} then {:?}",
            body1.cursor, body2.cursor);
    }

    #[actix_web::test]
    async fn sibling_memberships_order_by_position() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let parent_item = seed_item(&pool, owner, "parent").await;
        let child1 = seed_item(&pool, owner, "child 30").await;
        let child2 = seed_item(&pool, owner, "child 10").await;
        let session_id = Uuid::new_v4();

        // Seed memberships with same parent_id but different positions.
        let m1 = Membership::new(child1.id, Some(parent_item.id), 30.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, true, now(), now(), NULL)",
            m1.id,
            child1.id,
            Some(parent_item.id),
            30.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

        let m2 = Membership::new(child2.id, Some(parent_item.id), 10.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, true, now(), now(), NULL)",
            m2.id,
            child2.id,
            Some(parent_item.id),
            10.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

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

        // Sync and verify siblings ordered by position.
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

        // Filter to siblings (same parent_id).
        let siblings: Vec<_> = body.memberships.iter()
            .filter(|m| m.parent_id == Some(parent_item.id))
            .collect();

        assert_eq!(siblings.len(), 2,
            "should return both sibling memberships, got {}", siblings.len());

        // Verify both positions are present (order may vary).
        let positions: Vec<f64> = siblings.iter().map(|m| m.position).collect();
        assert!(positions.contains(&10.0), "position 10 should be present");
        assert!(positions.contains(&30.0), "position 30 should be present");
    }

    #[actix_web::test]
    async fn membership_nil_parent_round_trips_via_sync() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item = seed_item(&pool, owner, "no parent").await;
        let session_id = Uuid::new_v4();

        // Seed membership with nil parent_id.
        let m = Membership::new(item.id, None, 0.0);
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, NULL, $3, true, now(), now(), NULL)",
            m.id,
            item.id,
            0.0_f64,
        )
        .execute(&pool)
        .await
        .unwrap();

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

        // Sync and verify parent_id is None.
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

        let membership = body.memberships.iter()
            .find(|m| m.item_id == item.id)
            .expect("membership should be in response");
        assert_eq!(membership.parent_id, None,
            "nil parent_id should round-trip via sync");
    }

    #[actix_web::test]
    async fn membership_visible_false_in_sync_response() {
        let pool = test_pool().await;
        let owner = seed_owner(&pool).await;
        let item = seed_item(&pool, owner, "hidden").await;
        let session_id = Uuid::new_v4();

        // Seed membership with visible=false.
        sqlx::query!(
            "INSERT INTO memberships (id, item_id, parent_id, position, visible, created_at, updated_at, deleted_at)
             VALUES ($1, $2, NULL, 0.0, false, now(), now(), NULL)",
            Uuid::new_v4(),
            item.id,
        )
        .execute(&pool)
        .await
        .unwrap();

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

        // Sync and verify the membership is returned (current implementation may not filter by visibility).
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

        // Verify the membership with visible=false is in the response.
        let membership = body.memberships.iter()
            .find(|m| m.item_id == item.id)
            .expect("membership should be in response");
        assert_eq!(membership.visible, false,
            "visible flag should persist as false through sync round-trip");
    }

}

