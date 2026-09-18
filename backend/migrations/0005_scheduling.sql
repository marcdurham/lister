-- Due dates, time-gated visibility, recurrence, and per-list nested display.
ALTER TABLE items ADD COLUMN due_at TIMESTAMPTZ;

-- Absolute instant an item becomes visible; either set directly (fixed mode) or derived
-- from show_before_due_amount/unit relative to due_at (recomputed whenever due_at changes).
ALTER TABLE items ADD COLUMN show_after TIMESTAMPTZ;
ALTER TABLE items ADD COLUMN show_before_due_amount BIGINT;
ALTER TABLE items ADD COLUMN show_before_due_unit TEXT;

-- Absolute instant an item becomes hidden; either set directly (fixed mode) or derived
-- from hide_after_created_amount/unit relative to created_at.
ALTER TABLE items ADD COLUMN hide_after TIMESTAMPTZ;
ALTER TABLE items ADD COLUMN hide_after_created_amount BIGINT;
ALTER TABLE items ADD COLUMN hide_after_created_unit TEXT;

-- Recurrence: when a task with these set is marked done, it un-marks itself and its
-- due_at advances by this amount/unit instead of staying done.
ALTER TABLE items ADD COLUMN recur_amount BIGINT;
ALTER TABLE items ADD COLUMN recur_unit TEXT;

-- Per-list display setting: show every descendant level (lightly indented) instead of
-- just direct children.
ALTER TABLE items ADD COLUMN show_nested_children BOOLEAN NOT NULL DEFAULT FALSE;
