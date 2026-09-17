FROM rust:1.98-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libpq-dev curl git && \
    cargo install trunk --locked && \
    cargo install sqlx-cli --version 0.8.6 --locked --no-default-features --features postgres,rustls && \
    rustup target add wasm32-unknown-unknown

WORKDIR /app
COPY . .
RUN cd frontend && trunk build --release
ENV SQLX_OFFLINE=true
RUN cd backend && cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl-dev libpq5 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/backend /usr/local/bin/lister-backend
# The backend resolves its static assets at runtime via CARGO_MANIFEST_DIR/../frontend/dist,
# which is baked in at compile time as /app/backend - recreate that layout here so it resolves.
RUN mkdir -p /app/backend
COPY --from=builder /app/frontend/dist /app/frontend/dist

EXPOSE 8080
CMD ["lister-backend"]
