-- When the server last accepted a write to this row. `updated_at` is stamped by the client's
-- clock (and is what last-write-wins compares), so it can't be used as a pull cursor: a row
-- edited offline on one device and pushed after another device synced would have an
-- `updated_at` older than that device's cursor and never be pulled. Pulls filter on this
-- column instead. Existing rows get the migration time, so every client re-pulls once.
ALTER TABLE items ADD COLUMN server_updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp();
ALTER TABLE memberships ADD COLUMN server_updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp();
CREATE INDEX items_owner_server_updated_idx ON items (owner_id, server_updated_at);
CREATE INDEX memberships_server_updated_idx ON memberships (server_updated_at);
