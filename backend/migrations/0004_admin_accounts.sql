ALTER TABLE users
    ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN status TEXT NOT NULL DEFAULT 'approved';

ALTER TABLE users
    ADD CONSTRAINT users_status_check CHECK (status IN ('pending', 'approved', 'disabled'));
