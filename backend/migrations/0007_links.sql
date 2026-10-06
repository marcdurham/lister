-- Link items: tapping one navigates to `url`.
ALTER TABLE items ADD COLUMN is_link BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE items ADD COLUMN url TEXT;
