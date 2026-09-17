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

## 2026-09-17 10:55 PDT
- The browser's back button now navigates up the list hierarchy one level at a time
  (each in-app navigation pushes a history entry), matching the old "Up" button.
- Removed the "Up" button; every list except the top level now shows a ".." row at the
  top of its items that navigates up when clicked. Verified both in a local browser
  session (multi-level navigate-in, then browser Back and the ".." row each correctly
  step back up one level).

## 2026-09-17 11:10 PDT
- The app no longer requires logging in to use it: it now loads straight into the
  local/offline list view for everyone. A "Log in" button in the header opens the
  existing sign-in/register form as a dismissible panel ("Back" to return to the
  list); logging in swaps in the account's data and starts syncing to the server
  as before. The backend's `/api/sync` already rejected unauthenticated requests,
  so guests simply keep working offline until they choose to log in.

## 2026-09-17 12:00 PDT
- Replaced the gear glyph on the header's settings button with a proper hamburger-menu
  SVG icon, and made the menu itself visible even when logged out, so a guest can still
  export/import tasks as JSON files without an account. "Import from Google Tasks" stays
  hidden until logged in, since it's tied to a server-side account via OAuth.
