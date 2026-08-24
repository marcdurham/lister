#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

echo "Building frontend..."
(cd frontend && trunk build --release)

echo "Starting backend on port 8400..."
export LISTER_BIND="127.0.0.1:8400"
cd backend
exec cargo run --release
