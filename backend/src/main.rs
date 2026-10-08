mod admin;
mod api;
mod auth;
mod db;
mod google;
mod tokens;

use actix_files::{Files, NamedFile};
use actix_web::{get, web, App, HttpServer, Responder};
use std::path::PathBuf;

/// Largest JSON body accepted (e.g. a big first push after an import). Keep the reverse
/// proxy's `client_max_body_size` at least this large.
const MAX_JSON_BYTES: usize = 20 * 1024 * 1024;

fn dist_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend/dist")
}

#[get("/api/health")]
async fn health() -> impl Responder {
    actix_web::web::Json(serde_json::json!({ "status": "ok" }))
}

async fn spa_fallback() -> actix_web::Result<NamedFile> {
    Ok(NamedFile::open(dist_dir().join("index.html"))?)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init();

    let bind_addr = std::env::var("LISTER_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    println!("Lister backend listening on http://{bind_addr}");

    let pool = db::connect().await;
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("mint-token") {
        let Some(email) = args.get(2) else {
            eprintln!("usage: backend mint-token <email> [name]");
            std::process::exit(2);
        };
        tokens::mint_cli(&pool, email, args.get(3).map_or("API token", String::as_str)).await;
        return Ok(());
    }
    auth::seed_default_admins(&pool).await;

    let google_cache = web::Data::new(google::GoogleImportCache::default());

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(google_cache.clone())
            .app_data(web::JsonConfig::default().limit(MAX_JSON_BYTES))
            .service(health)
            .service(api::sync)
            .service(auth::register)
            .service(auth::login)
            .service(auth::logout)
            .service(auth::me)
            .service(admin::list_users)
            .service(admin::set_status)
            .service(admin::set_admin)
            .service(admin::delete_user)
            .service(tokens::create)
            .service(tokens::list)
            .service(tokens::revoke)
            .service(tokens::get_list)
            .service(google::connect)
            .service(google::callback)
            .service(google::imported_tasks)
            .service(Files::new("/", dist_dir()).index_file("index.html"))
            .default_service(actix_web::web::route().to(spa_fallback))
    })
    .bind(bind_addr)?
    .run()
    .await
}
