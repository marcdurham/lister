CREATE TABLE items (
    id UUID PRIMARY KEY,
    text TEXT NOT NULL,
    notes TEXT,
    is_note BOOLEAN NOT NULL DEFAULT FALSE,
    done BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    deleted_at TIMESTAMPTZ
);

CREATE TABLE memberships (
    id UUID PRIMARY KEY,
    item_id UUID NOT NULL REFERENCES items(id),
    parent_id UUID REFERENCES items(id),
    position DOUBLE PRECISION NOT NULL,
    visible BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_memberships_parent_id ON memberships(parent_id);
CREATE INDEX idx_memberships_item_id ON memberships(item_id);
CREATE INDEX idx_items_updated_at ON items(updated_at);
CREATE INDEX idx_memberships_updated_at ON memberships(updated_at);

CREATE UNIQUE INDEX uq_membership_item_parent
    ON memberships(item_id, COALESCE(parent_id, '00000000-0000-0000-0000-000000000000'))
    WHERE deleted_at IS NULL;
