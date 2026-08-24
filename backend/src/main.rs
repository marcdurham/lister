mod api;
mod auth;
mod db;

use actix_files::{Files, NamedFile};
use actix_web::{get, web, App, HttpServer, Responder};
use std::path::PathBuf;

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

    let bind_addr = std::env::var("LISTER_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    println!("Lister backend listening on http://{bind_addr}");

    let pool = db::connect().await;

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(health)
            .service(api::sync)
            .service(auth::register)
            .service(auth::login)
            .service(auth::logout)
            .service(auth::me)
            .service(Files::new("/", dist_dir()).index_file("index.html"))
            .default_service(actix_web::web::route().to(spa_fallback))
    })
    .bind(bind_addr)?
    .run()
    .await
}
