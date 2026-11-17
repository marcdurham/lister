//! Read-only API tokens and `GET /api/lists`, so other apps (the home-monitor dashboard) can
//! show the items of one list without a browser session.
//!
//! A token is sent as `Authorization: Bearer <token>` (a login session cookie works too). It
//! only opens the read endpoints in this file, never sync. Tokens are minted with
//! `POST /api/tokens` while logged in, or `backend mint-token <email> [name]` on the server.

use std::collections::HashMap;

use actix_web::{delete, get, post, web, HttpRequest, HttpResponse, Responder};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::auth;

fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Mints a token for `user_id`; the plain token is returned once and never stored.
pub async fn create_token(pool: &PgPool, user_id: Uuid, name: &str) -> sqlx::Result<(Uuid, String)> {
    let token = format!("lst_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO api_tokens (id, user_id, name, token_hash, created_at) VALUES ($1, $2, $3, $4, now())")
        .bind(id)
        .bind(user_id)
        .bind(name)
        .bind(hash_token(&token))
        .execute(pool)
        .await?;
    Ok((id, token))
}

/// `backend mint-token <email> [name]`: prints a new token for an approved account.
pub async fn mint_cli(pool: &PgPool, email: &str, name: &str) {
    let row = sqlx::query("SELECT id FROM users WHERE email = $1 AND status = 'approved'")
        .bind(email.trim().to_lowercase())
        .fetch_optional(pool)
        .await
        .expect("user lookup failed");
    let Some(row) = row else {
        eprintln!("no approved account with email {email}");
        std::process::exit(1);
    };
    let (_, token) = create_token(pool, row.get("id"), name).await.expect("could not create token");
    println!("{token}");
}

/// The user behind a bearer token, or else the logged-in session's user.
async fn api_user(req: &HttpRequest, pool: &PgPool) -> Option<Uuid> {
    if let Some(token) = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    {
        let row = sqlx::query(
            "UPDATE api_tokens t SET last_used_at = now() FROM users u
             WHERE t.token_hash = $1 AND u.id = t.user_id AND u.status = 'approved' RETURNING t.user_id",
        )
        .bind(hash_token(token.trim()))
        .fetch_optional(pool)
        .await
        .ok()??;
        return Some(row.get("user_id"));
    }
    auth::current_user_id(req, pool).await
}

#[derive(Deserialize)]
struct NewToken {
    name: Option<String>,
}

#[post("/api/tokens")]
pub async fn create(pool: web::Data<PgPool>, req: HttpRequest, body: web::Json<NewToken>) -> impl Responder {
    let Some(user_id) = auth::current_user_id(&req, pool.get_ref()).await else {
        return HttpResponse::Unauthorized().finish();
    };
    let name = body.name.as_deref().unwrap_or("").trim();
    let name = if name.is_empty() { "API token" } else { name };
    match create_token(pool.get_ref(), user_id, name).await {
        Ok((id, token)) => HttpResponse::Ok().json(json!({ "id": id, "name": name, "token": token })),
        Err(err) => HttpResponse::InternalServerError().body(format!("create token failed: {err}")),
    }
}

#[get("/api/tokens")]
pub async fn list(pool: web::Data<PgPool>, req: HttpRequest) -> impl Responder {
    let Some(user_id) = auth::current_user_id(&req, pool.get_ref()).await else {
        return HttpResponse::Unauthorized().finish();
    };
    let rows = sqlx::query("SELECT id, name, created_at, last_used_at FROM api_tokens WHERE user_id = $1 ORDER BY created_at")
        .bind(user_id)
        .fetch_all(pool.get_ref())
        .await;
    match rows {
        Ok(rows) => HttpResponse::Ok().json(
            rows.iter()
                .map(|r| {
                    json!({
                        "id": r.get::<Uuid, _>("id"),
                        "name": r.get::<String, _>("name"),
                        "created_at": r.get::<DateTime<Utc>, _>("created_at"),
                        "last_used_at": r.get::<Option<DateTime<Utc>>, _>("last_used_at"),
                    })
                })
                .collect::<Vec<_>>(),
        ),
        Err(err) => HttpResponse::InternalServerError().body(format!("list tokens failed: {err}")),
    }
}

#[delete("/api/tokens/{id}")]
pub async fn revoke(pool: web::Data<PgPool>, req: HttpRequest, id: web::Path<Uuid>) -> impl Responder {
    let Some(user_id) = auth::current_user_id(&req, pool.get_ref()).await else {
        return HttpResponse::Unauthorized().finish();
    };
    match sqlx::query("DELETE FROM api_tokens WHERE id = $1 AND user_id = $2")
        .bind(id.into_inner())
        .bind(user_id)
        .execute(pool.get_ref())
        .await
    {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(err) => HttpResponse::InternalServerError().body(format!("revoke failed: {err}")),
    }
}

#[derive(Deserialize)]
struct ListQuery {
    path: String,
}

#[derive(Serialize)]
struct ListItem {
    id: Uuid,
    text: String,
    done: bool,
    is_note: bool,
    is_list: bool,
    is_link: bool,
    url: Option<String>,
    due_at: Option<DateTime<Utc>>,
    /// 0 for the list's own items, deeper for nested ones (only when the list shows nested children).
    depth: u32,
}

struct Node {
    item: ListItem,
    text_lower: String,
    show_nested: bool,
    time_visible: bool,
}

/// parent (None = top level) -> (position, visible, child), in list order.
type Children = HashMap<Option<Uuid>, Vec<(f64, bool, Uuid)>>;

/// The list at `segments` (lowercased names). Tried first from the top level down; when that
/// fails, the first segment may be a list anywhere in the tree (so a list that was moved into
/// another one is still found by its name): the shallowest match whose remaining segments
/// resolve beneath it wins. `node` gives an existing item's lowercased name and whether it is a
/// list; ids it doesn't know (deleted items) are skipped.
fn resolve_path<'a>(segments: &[String], children: &Children, node: impl Fn(&Uuid) -> Option<(&'a str, bool)>) -> Option<Uuid> {
    let named = |parent: Option<Uuid>, seg: &str| {
        children.get(&parent)?.iter().map(|c| c.2).find(|id| node(id).is_some_and(|(name, _)| name == seg))
    };
    let walk_from = |start: Option<Uuid>, segs: &[String]| segs.iter().try_fold(start, |cur, seg| named(cur, seg).map(Some));
    let (first, rest) = segments.split_first()?;
    if let Some(found) = walk_from(None, segments) {
        return found;
    }
    // Breadth-first from the top level, so shallower lists win; an item can sit in several lists.
    let mut seen = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<Uuid> = children.get(&None).into_iter().flatten().map(|c| c.2).collect();
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        let Some((name, is_list)) = node(&id) else { continue };
        let kids = children.get(&Some(id));
        if name == first && (is_list || kids.is_some_and(|k| !k.is_empty())) {
            if let Some(found) = walk_from(Some(id), rest) {
                return found;
            }
        }
        queue.extend(kids.into_iter().flatten().map(|c| c.2));
    }
    None
}

#[cfg(test)]
mod resolve_path_tests {
    use super::*;

    struct Tree {
        children: Children,
        names: HashMap<Uuid, (String, bool)>,
    }

    impl Tree {
        fn new() -> Self {
            Tree { children: HashMap::new(), names: HashMap::new() }
        }
        fn add(&mut self, parent: Option<Uuid>, name: &str, is_list: bool) -> Uuid {
            let id = Uuid::new_v4();
            let siblings = self.children.entry(parent).or_default();
            siblings.push((siblings.len() as f64, true, id));
            self.names.insert(id, (name.to_lowercase(), is_list));
            id
        }
        fn resolve(&self, path: &str) -> Option<Uuid> {
            let segs: Vec<String> = path.split('/').map(|s| s.to_lowercase()).collect();
            resolve_path(&segs, &self.children, |id| self.names.get(id).map(|(n, l)| (n.as_str(), *l)))
        }
    }

    #[test]
    fn top_level_path_still_resolves() {
        let mut t = Tree::new();
        let home = t.add(None, "Home", true);
        let groceries = t.add(Some(home), "Groceries", true);
        assert_eq!(t.resolve("Home"), Some(home));
        assert_eq!(t.resolve("home/groceries"), Some(groceries));
    }

    #[test]
    fn finds_a_moved_list_by_name_anywhere() {
        let mut t = Tree::new();
        let projects = t.add(None, "Projects", true);
        let dev = t.add(Some(projects), "Dev", true);
        let features = t.add(Some(dev), "home-monitor features", true);
        let sub = t.add(Some(features), "Later", true);
        assert_eq!(t.resolve("home-monitor features"), Some(features));
        assert_eq!(t.resolve("Dev/home-monitor features/Later"), Some(sub));
        assert_eq!(t.resolve("Nope"), None);
        assert_eq!(t.resolve("Dev/Nope"), None);
    }

    #[test]
    fn prefers_top_level_then_shallowest_and_skips_plain_items() {
        let mut t = Tree::new();
        let a = t.add(None, "A", true);
        let b = t.add(Some(a), "B", true);
        let deep = t.add(Some(b), "House", true);
        let _plain = t.add(Some(a), "House", false); // a plain item, not a list: ignored
        assert_eq!(t.resolve("House"), Some(deep));
        let top = t.add(None, "House", true);
        assert_eq!(t.resolve("House"), Some(top));
    }
}

/// `GET /api/lists?path=Home/Groceries`: the items of the list at `path` (list names from a
/// top-level list down, matched case-insensitively; if that fails, the path may start at a list
/// anywhere, see `resolve_path`), in the list's order, minus deleted and
/// hidden ones. Done items are included (flagged) so the caller can decide what to show.
#[get("/api/lists")]
pub async fn get_list(pool: web::Data<PgPool>, req: HttpRequest, q: web::Query<ListQuery>) -> impl Responder {
    let Some(owner_id) = api_user(&req, pool.get_ref()).await else {
        return HttpResponse::Unauthorized().finish();
    };
    let segments: Vec<String> = q
        .path
        .split('/')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if segments.is_empty() {
        return HttpResponse::BadRequest().body("path is required, e.g. path=Home/Groceries");
    }

    let items = sqlx::query(
        "SELECT id, text, is_note, is_list, done, due_at, show_after, hide_after, show_nested_children, is_link, url
         FROM items WHERE owner_id = $1 AND deleted_at IS NULL",
    )
    .bind(owner_id)
    .fetch_all(pool.get_ref())
    .await;
    let memberships = sqlx::query(
        "SELECT m.item_id, m.parent_id, m.position, m.visible FROM memberships m
         JOIN items i ON i.id = m.item_id
         WHERE i.owner_id = $1 AND m.deleted_at IS NULL AND i.deleted_at IS NULL",
    )
    .bind(owner_id)
    .fetch_all(pool.get_ref())
    .await;
    let (items, memberships) = match (items, memberships) {
        (Ok(i), Ok(m)) => (i, m),
        (Err(e), _) | (_, Err(e)) => return HttpResponse::InternalServerError().body(format!("query failed: {e}")),
    };

    let now = Utc::now();
    let mut nodes: HashMap<Uuid, Node> = HashMap::new();
    for r in &items {
        let show_after: Option<DateTime<Utc>> = r.get("show_after");
        let hide_after: Option<DateTime<Utc>> = r.get("hide_after");
        let text: String = r.get("text");
        let id: Uuid = r.get("id");
        nodes.insert(
            id,
            Node {
                text_lower: text.trim().to_lowercase(),
                show_nested: r.get("show_nested_children"),
                time_visible: show_after.map_or(true, |t| now >= t) && hide_after.map_or(true, |t| now < t),
                item: ListItem {
                    id,
                    text,
                    done: r.get("done"),
                    is_note: r.get("is_note"),
                    is_list: r.get("is_list"),
                    is_link: r.get("is_link"),
                    url: r.get("url"),
                    due_at: r.get("due_at"),
                    depth: 0,
                },
            },
        );
    }
    // parent (None = top level) -> (position, visible, child), in list order.
    let mut children: HashMap<Option<Uuid>, Vec<(f64, bool, Uuid)>> = HashMap::new();
    for m in &memberships {
        children
            .entry(m.get("parent_id"))
            .or_default()
            .push((m.get("position"), m.get("visible"), m.get("item_id")));
    }
    for v in children.values_mut() {
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
    }

    let Some(list_id) = resolve_path(&segments, &children, |id| nodes.get(id).map(|n| (n.text_lower.as_str(), n.item.is_list))) else {
        return HttpResponse::NotFound().body(format!("no list at path `{}`", q.path));
    };

    fn walk(
        parent: Uuid,
        depth: u32,
        nested: bool,
        children: &HashMap<Option<Uuid>, Vec<(f64, bool, Uuid)>>,
        nodes: &HashMap<Uuid, Node>,
        out: &mut Vec<ListItem>,
    ) {
        for (_, visible, id) in children.get(&Some(parent)).into_iter().flatten() {
            let Some(n) = nodes.get(id) else { continue };
            if !visible || !n.time_visible || depth > 8 {
                continue;
            }
            out.push(ListItem { depth, ..clone_item(&n.item) });
            if nested {
                walk(*id, depth + 1, nested, children, nodes, out);
            }
        }
    }
    fn clone_item(i: &ListItem) -> ListItem {
        ListItem {
            id: i.id,
            text: i.text.clone(),
            done: i.done,
            is_note: i.is_note,
            is_list: i.is_list,
            is_link: i.is_link,
            url: i.url.clone(),
            due_at: i.due_at,
            depth: 0,
        }
    }

    let node = &nodes[&list_id];
    let mut out = Vec::new();
    walk(list_id, 0, node.show_nested, &children, &nodes, &mut out);
    HttpResponse::Ok().json(json!({ "id": list_id, "text": node.item.text, "path": q.path, "items": out }))
}
