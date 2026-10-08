-- Read-only API tokens (e.g. for the home-monitor dashboard). Only the SHA-256 of the token is stored.
CREATE TABLE api_tokens (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ
);
CREATE INDEX idx_api_tokens_user_id ON api_tokens(user_id);
