FROM rust:1.79-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libpq-dev curl git && \
    cargo install trunk sqlx-cli --no-default-features --features postgres,rustls && \
    rustup target add wasm32-unknown-unknown

WORKDIR /app
COPY . .
RUN cd frontend && trunk build --release
RUN cd backend && cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl-dev libpq5 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/backend/target/release/backend /usr/local/bin/lister-backend
COPY --from=builder /app/frontend/dist /app/frontend/dist

EXPOSE 8080
CMD ["lister-backend"]
