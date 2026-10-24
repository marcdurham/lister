-- An item's own flags now decide whether it's a list: before, any task with children
-- acted as a list whatever its `is_list` said, so a list converted back to a task kept
-- showing as a list. Flag every such parent explicitly so it keeps acting as a list.
-- `updated_at` is bumped so this wins last-write-wins against clients' cached copies.
UPDATE items SET is_list = TRUE, updated_at = now(), server_updated_at = clock_timestamp()
WHERE NOT is_list AND NOT is_note AND NOT is_link AND deleted_at IS NULL
  AND EXISTS (
    SELECT 1 FROM memberships m JOIN items c ON c.id = m.item_id
    WHERE m.parent_id = items.id AND m.deleted_at IS NULL AND c.deleted_at IS NULL
  );
