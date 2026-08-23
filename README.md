# Lister

A tiny offline-first list-keeping PWA. Frontend is Rust + [Yew](https://yew.rs) compiled to WASM via [Trunk](https://trunkrs.dev); backend is [Actix Web](https://actix.rs) serving the built assets and a small API.

## Layout

- `frontend/` — Yew app (`trunk build`), PWA manifest + service worker in `frontend/static/`.
- `backend/` — Actix Web server that serves `frontend/dist` and exposes `/api/health`.

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
```

## Develop

Run the frontend with hot reload:

```bash
cd frontend
trunk serve
```

## Build & run for production

```bash
cd frontend
trunk build --release

cd ../backend
cargo run --release
```

Then open http://127.0.0.1:8080. The app is installable as a PWA and works offline once loaded (assets are precached and re-cached on fetch via `frontend/static/service-worker.js`).
