# Changes

## 2026-09-17 10:07 PDT
- Added `AGENTS.md` with instructions to keep this change log up to date.

## 2026-09-17 10:12 PDT
- Admin page now shows an inline confirmation message ("Deleted x@example.com.",
  "Approved ...", "Disabled ...", etc.) after each account action instead of leaving
  the admin to guess whether it worked or refresh the page to find out.

## 2026-09-17 10:16 PDT
- Changed the item row's edit icon to three vertical dots.

## 2026-09-17 10:18 PDT
- The notes field in the item editor now uses a full-sized (1rem) font instead of the
  smaller 0.85rem font it inherited from the old inline notes textarea.

## 2026-09-17 10:20 PDT
- New items are now added to the top of a list instead of the bottom.

## 2026-09-17 10:45 PDT
- Replaced the item editor's "Add to list" button with a "Lists" button that opens a new
  full-screen list manager: the item being edited is shown compactly at the top, a search
  box below it, and below that the normal item hierarchy (browsable exactly like the main
  view, with breadcrumbs) with a checkbox on every row to mark the edited item as a member
  of that list. Save applies all the additions/removals at once; Cancel/Back discards them.
  Prevents an item from being nested under itself or one of its own descendants.
- Added `Action::RemoveMembership` and `AppState::descendant_ids` to support the manager.
