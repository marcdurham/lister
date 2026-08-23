use actix_files::{Files, NamedFile};
use actix_web::{get, App, HttpServer, Responder};
use std::path::PathBuf;

const DIST_DIR: &str = "frontend/dist";

#[get("/api/health")]
async fn health() -> impl Responder {
    actix_web::web::Json(serde_json::json!({ "status": "ok" }))
}

async fn spa_fallback() -> actix_web::Result<NamedFile> {
    Ok(NamedFile::open(PathBuf::from(DIST_DIR).join("index.html"))?)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let bind_addr = std::env::var("LISTER_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    println!("Lister backend listening on http://{bind_addr}");

    HttpServer::new(|| {
        App::new()
            .service(health)
            .service(Files::new("/", DIST_DIR).index_file("index.html"))
            .default_service(actix_web::web::route().to(spa_fallback))
    })
    .bind(bind_addr)?
    .run()
    .await
}
