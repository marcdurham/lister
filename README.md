# Lister

An offline-first, hierarchical list-keeping PWA. Frontend is Rust + [Yew](https://yew.rs) compiled to WASM via [Trunk](https://trunkrs.dev); backend is [Actix Web](https://actix.rs) + [Postgres](https://www.postgresql.org) (via `sqlx`) serving the built assets and a sync API.

## Layout

- `shared/` — domain types (`Item`, `Membership`, sync DTOs) used by both `frontend` and `backend`, so the wire format can't drift.
- `frontend/` — Yew app (`trunk build`). All reads/writes go through a local `localStorage`-backed store (`src/store.rs`) — the app works fully offline. A background loop (`src/sync.rs`) pushes local changes and pulls remote ones every 30s (and on reconnect), using last-write-wins on `updated_at`.
- `backend/` — Actix Web server exposing `POST /api/sync` (push+pull in one round trip) and `/api/health`, and serving `frontend/dist`. Postgres schema/migrations live in `backend/migrations/`.

## Data model

Everything is one `items` table plus one `memberships` join table:

- an item can be a plain task, a note (`is_note`, not completable), or a list — any item can have children
- a membership row (`item_id`, `parent_id`) is how an item appears inside another item's list; `parent_id = NULL` means "top-level list"
- an item can have multiple membership rows (different `parent_id`) to appear in more than one list at once (tags)
- `position` (per membership) gives each list its own custom order for that item
- `visible` (per membership) lets an item be hidden from one list while staying visible in another

`updated_at`/`deleted_at` on both tables double as the sync log — no separate op-log needed.

Recurring tasks, time-gated visibility, and a calendar view are intentionally not implemented yet (see `tasks.md`); the schema has room to add them later as an additive migration.

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk sqlx-cli --no-default-features --features postgres,rustls
```

A local Postgres server, with a role/database for the app:

```bash
psql -d postgres -c "CREATE ROLE lister WITH LOGIN PASSWORD 'change-me';"
psql -d postgres -c "CREATE DATABASE lister OWNER lister;"
```

Copy `backend/.env.example` to `backend/.env` (or export `DATABASE_URL` yourself) — defaults to `postgres://lister:change-me@localhost/lister`.

## Develop

Run the frontend with hot reload:

```bash
cd frontend
trunk serve
```

Migrations run automatically when the backend starts (`sqlx::migrate!()`), or run them manually:

```bash
cd backend
sqlx migrate run
```

## Build & run for production

```bash
./run.sh
```

This builds the frontend release bundle and starts the backend on port 8400. Then open http://127.0.0.1:8400. The app is installable as a PWA; the static shell works offline via the service worker, and list data works offline via the local store + background sync described above.
