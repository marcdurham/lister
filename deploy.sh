#!/usr/bin/env bash
# Deploy Lister straight from this machine to the Docker host. This does the same job as
# .github/workflows/deploy.yml: rsync the source over, then `docker compose up --build`.
#
# Settings come from deploy.env next to this script. Copy deploy.env.example to get
# started; deploy.env is gitignored. This script also writes the app settings to
# ~/lister/.env on the host, so running `docker compose ...` by hand there uses the same
# password and settings.
#
# Usage: ./deploy.sh
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

if [[ ! -f deploy.env ]]; then
  echo "deploy.env not found. Copy deploy.env.example to deploy.env and fill it in." >&2
  exit 1
fi
# Read deploy.env as literal KEY=value lines (not shell code), so values may contain
# spaces, $ and other special characters without quoting. One pair of matching
# surrounding quotes is stripped if present.
while IFS= read -r line || [[ -n "$line" ]]; do
  line="${line%$'\r'}"
  [[ "$line" =~ ^[[:space:]]*(#|$) ]] && continue
  if [[ ! "$line" =~ ^[[:space:]]*([A-Za-z_][A-Za-z0-9_]*)=(.*)$ ]]; then
    echo "deploy.env: can't parse line: $line" >&2
    exit 1
  fi
  key="${BASH_REMATCH[1]}"
  value="${BASH_REMATCH[2]}"
  if [[ "$value" =~ ^\"(.*)\"$ || "$value" =~ ^\'(.*)\'$ ]]; then
    value="${BASH_REMATCH[1]}"
  fi
  printf -v "$key" '%s' "$value"
  export "${key?}"
done < deploy.env

missing=()
for var in SSH_USER DEPLOYMENT_HOST POSTGRES_PASSWORD; do
  [[ -n "${!var:-}" ]] || missing+=("$var")
done
if (( ${#missing[@]} )); then
  echo "Missing required settings in deploy.env: ${missing[*]}" >&2
  exit 1
fi

APP_VARS=(POSTGRES_PASSWORD GOOGLE_CLIENT_ID GOOGLE_CLIENT_SECRET GOOGLE_REDIRECT_URI DEFAULT_ADMINISTRATORS)
for var in "${APP_VARS[@]}"; do
  if [[ "${!var:-}" == *"'"* || "${!var:-}" == *$'\n'* ]]; then
    echo "$var must not contain a single quote or a newline." >&2
    exit 1
  fi
done

REMOTE="${SSH_USER}@${DEPLOYMENT_HOST}"
REMOTE_DIR="${REMOTE_DIR:-lister}"   # relative to the remote user's home
HOST_PORT="${HOST_PORT:-8400}"

echo "==> Copying source to ${REMOTE}:~/${REMOTE_DIR}/"
ssh "$REMOTE" "mkdir -p ~/${REMOTE_DIR}"
# Respect .gitignore so build artifacts and local secrets don't get copied. Excluded
# paths are also protected from --delete, so the host's .env survives.
rsync -az --delete \
  --exclude='.git' \
  --exclude='.claude' \
  --exclude='.env' \
  --exclude='deploy.env' \
  --exclude='.pi-loop-log.jsonl' \
  --filter=':- .gitignore' \
  ./ "${REMOTE}:~/${REMOTE_DIR}/"

echo "==> Writing ~/${REMOTE_DIR}/.env on the host"
for var in "${APP_VARS[@]}"; do
  [[ -n "${!var:-}" ]] && printf "%s='%s'\n" "$var" "${!var}"
done | ssh "$REMOTE" "umask 077 && cat > ~/${REMOTE_DIR}/.env"

echo "==> Building and starting containers"
ssh "$REMOTE" "cd ~/${REMOTE_DIR} && docker compose up --build -d --remove-orphans && docker image prune -f"

echo "==> Waiting for the app to report healthy"
for _ in $(seq 1 30); do
  if ssh "$REMOTE" "curl -fsS http://localhost:${HOST_PORT}/api/health" >/dev/null 2>&1; then
    echo "Deployed: lister is up on ${DEPLOYMENT_HOST}:${HOST_PORT}."
    exit 0
  fi
  sleep 2
done

echo "The app did not become healthy within 60s. Recent logs:" >&2
ssh "$REMOTE" "cd ~/${REMOTE_DIR} && docker compose ps -a && docker compose logs --tail=40" >&2
exit 1
