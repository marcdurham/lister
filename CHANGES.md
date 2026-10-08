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

## 2026-09-17 12:34 PDT
- Dragging to reorder now works from anywhere on an item row (not just the six-dot
  handle) as long as it's not the checkbox or the edit button.
- While dragging, every other row's edit button turns into a larger chevron "drop to
  nest" target; dropping onto it moves the dragged item to become a child of that row,
  turning the target into a list automatically if it wasn't one already.
- Combined the separate online/offline status pill and login/logout buttons into a
  single icon button in the header. Its ring is green when online and yellow when
  offline; the glyph itself differs when someone is signed in. Clicking it opens a
  small menu with the online/offline text, the signed-in email (if any), and a
  log in/log out action.
- The item-type toggle in the composer is now a Note/Task/List dropdown (previously
  just Note/Task), so a new item can be created as a list directly.
- Added a compact search toggle next to the composer: it swaps the "Add" box for a
  live search box that filters the current list by title, with a "+" button to switch
  back to add mode.
- Added a "Copy list" button that copies the current list as a markdown bullet list of
  just the item titles, with children indented under their parents.

## 2026-09-17 13:31 PDT
- Replaced the native HTML5 drag-and-drop on item rows with a hand-rolled pointer-events
  implementation, since native `draggable`/`dragstart` never fires at all on touch
  devices. A mouse press-and-drag on the row body now starts reordering as soon as it
  moves past a small threshold; a touch/pen press arms into a drag after a brief hold
  (native scrolling is disabled on the row via `touch-action: none` so that hold can be
  timed), and if the finger moves before the hold elapses it's treated as an ordinary
  scroll (scrolled manually in JS) instead of a drag, so scrolling a list that starts on
  an item still works.
- Fixed a bug in the first pass of that touch support where the hold timer's closure
  captured a `use_state` snapshot that never reflected later updates (a `UseStateHandle`
  read in a closure only ever sees the value from the render that created it), so the
  hold-to-arm never actually fired. Switched the drag tracker to `use_mut_ref`
  (real shared `Rc<RefCell<_>>` state) so it's read live from every closure, including
  the delayed timer.

## 2026-09-17 13:50 PDT
- Item rows are no longer text-selectable (`user-select: none` on `.item`), so a
  press-and-hold or drag no longer risks highlighting text instead of reordering; the
  item editor's text/notes fields are unaffected and remain selectable as normal.
- Added "Import markdown (.md)" to the menu. Headings become parent list items (a
  sub-heading nests under the nearest shallower heading); bullet/numbered list items
  become children, nested by indentation, under the enclosing list item or heading;
  any other text under a heading becomes that heading's notes. A line starting with
  `TODO:` imports as an unchecked task and `DONE:` as a checked one; a heading or list
  item that is *only* `TODO`/`TODO:` isn't a task itself, but everything nested under
  it defaults to being one. New items are added as new top-level lists, after whatever
  already exists. Added unit tests in `frontend/src/markdown.rs` covering heading
  nesting, indentation nesting, notes collection, and the TODO/DONE rules (run via
  `cargo test -p frontend --bin frontend`, since the crate has no `[lib]` target).

## 2026-09-17 14:11 PDT
- A markdown import now lands as a single new item inside whatever list you're
  currently viewing (top-level if you're at the root "Lists" screen), instead of always
  adding new top-level lists. The imported item is named after the file (its `.md`/
  `.markdown` extension stripped), and everything the file parsed into is nested under it.

## 2026-09-17 17:42 PDT
- Removed the six-dot grip icon from item rows (dragging still works by pressing anywhere
  on the row, so the icon wasn't needed).
- Added "Select all" / "Deselect all" / "Invert" buttons to the Google Tasks import dialog.
- Any open dropdown menu (the header's hamburger menu and the online/session menu) now
  closes as soon as you click or tap anywhere outside it, instead of staying open until
  something explicitly closed it.
- Added a per-list "Show multiple levels of children in this list" setting (in that list's
  own item editor): when on, the list view shows every descendant level, not just direct
  children, each level indented about one character further. Dragging is disabled on those
  deeper rows (their real parent differs from the list being viewed, so dropping onto them
  can't correctly reorder/nest) but tapping to open/edit/check them off still works.
- Added due dates, time-gated visibility, and recurrence to items, editable from the item
  editor's new "Due date"/"Show"/"Hide"/"Repeats every" fields:
  - A due date, plus a "Show" mode that keeps an item out of the list entirely until
    either a fixed date/time or a chosen amount of time before its due date.
  - A "Hide" mode that removes an item from the list either at a fixed date/time or a
    chosen amount of time after it was created.
  - "Show hidden items" now also reveals items that are outside their show/hide window
    (as well as its previous membership-`visible` meaning), so you can preview what's
    scheduled or expired.
  - Recurring tasks: checking a task with "Repeats every" set doesn't mark it done - it
    stays unchecked and its due date jumps forward by the configured interval instead.
  - New `shared::Item` fields (`due_at`, `show_after`, `show_before_due_amount/unit`,
    `hide_after`, `hide_after_created_amount/unit`, `recur_amount/unit`,
    `show_nested_children`) sync to the backend like any other item field; migration
    `0005_scheduling.sql` adds the matching columns.

## 2026-09-17 18:02 PDT
- Items with a scheduled "Show"/"Hide" time now appear/disappear on their own instead of
  only when something else triggers a re-render (an edit, the 30s sync, or a page
  refresh). Added a `visibility_tick_loop` that dispatches a no-op `Action::Tick` every
  30s purely to force re-evaluation of each item's show/hide window against the current
  time; unlike the sync loop it also runs while offline, so this works for a guest who's
  never logged in too.

## 2026-09-17 23:15 PDT
- Added a Theme section (System / Light / Dark) to the settings menu. The choice is
  stored as a bare string under `lister.theme`, and a small inline script in
  `index.html` resolves it (following the OS setting in System mode, live) into a
  `data-theme` attribute on `<html>` before the app loads, so there's no flash of the
  wrong theme on startup. The stylesheet's dark rules now key off
  `:root[data-theme="dark"]` instead of a `prefers-color-scheme` media query.
- Reworked the drag-to-nest target. It used to be a small chevron button in place of
  the edit button, shorter than the row, so moving the pointer from one row's chevron
  to the next passed over plain row (a reorder target) and the dragged item flashed
  back and forth. The target now covers the right 45% of the row at the row's full
  height, so adjacent targets touch with no gap. Hovering it outlines the whole row in
  white (an inset outline, so the row's size doesn't change) and swaps the row's
  left-hand icon for a right-pointing chevron. The edit button stays in the row
  (invisible) during a drag so row heights don't shift when a drag starts.

## 2026-09-30
- Added a "Remote mode" toggle to the settings menu (remembered in the browser). While it's
  on:
  - every item row shows an icon for where it lives: a disk (local only - never uploaded),
    a cloud with a check (on the server and identical), or a cloud with an up arrow (on the
    server, but with local changes not yet uploaded);
  - a bar under the header summarizes how many items are local-only / changed, shows when
    the last full sync finished, and has a "Sync" button that pushes and pulls everything;
  - the item editor shows an "Uploaded:" timestamp ("Not uploaded" for a local-only item),
    the item's status, and a "Remote" menu with "Push to server" (upload; leaves a newer
    server copy alone and says so) and "Sync with server" (upload, or pull the server's copy
    if it's newer).
- Local storage now records when each item was last pushed/pulled (`ItemRecord::pushed_at`);
  `AppState` carries per-item sync status; `sync::sync_item` does the per-item push/sync.

## 2026-09-30 (later)
- After "Sync with server" pulls a newer copy into the item editor, the editor is rebuilt
  from it; the "server had a newer version" message now carries over to the rebuilt editor
  instead of disappearing (`ItemEditor::initial_message`, cleared when another item is opened).

## 2026-09-30 13:26 PDT
- Fixed devices silently missing each other's lists even though both said "Synced". Pulls
  used the client-stamped `updated_at` against a server-time cursor, so an item edited on
  one device (offline, or with a slow clock) and pushed after another device had synced had
  an `updated_at` older than that device's cursor and was never pulled. Migration 0006 adds
  a server-stamped `server_updated_at` to `items` and `memberships` (set on every accepted
  write); `/api/sync` now pulls rows whose `server_updated_at` (or `updated_at`, which keeps
  the per-item sync check working) is past the cursor. The returned cursor is now the
  database clock minus a 5s overlap so concurrent commits can't slip past it. Existing rows
  are stamped at migration time, so every client re-pulls everything once after deploy,
  which also recovers the currently missing lists. Conflict resolution is unchanged
  (still last-write-wins on `updated_at`). Added a regression test; it wasn't run here
  because the test database needs credentials this session doesn't have.

## 2026-09-30 14:25 PDT
- Added an item count to the right of the breadcrumbs, in an amber pill. It counts every
  non-deleted item under the current list at every nesting depth (an item that sits in
  several lists counts once); at the top level it counts everything across all lists.
  Uses the new `AppState::live_descendant_count`, with a unit test.

## 2026-09-30
- List manager: added a "Top level (root list)" checkbox so an item can be added to the
  root list, and a virtual "Orphans" list at the end of the root list that holds every live
  item belonging to no list (browsable, and checkable as a target). Search in the manager
  already covers all items, orphans included. Added `AppState::orphan_items`.

## 2026-09-30 (2)
- Main view search from the root list now also finds orphan items (items in no list).
  They appear as non-draggable rows using a placeholder membership, and open in the
  item editor like any other result.

## 2026-09-30 (3)
- Main view: the root list now ends with a virtual "Orphans" row (with a count) that opens
  a list of all items belonging to no list. It has its own breadcrumb and count, hides the
  add-item composer, and its rows are non-draggable (placeholder memberships).

## 2026-10-05 23:59 PDT
- New "Link" item type (composer kind "Link"; also "Convert to link" in the editor). A link
  holds a URL (new `Item::is_link`/`Item::url`, migration 0007) and tapping its row opens the
  URL in a new tab; the edit button still opens the editor, which has a URL field and an
  open-link button. A link's text may be left blank, in which case the URL itself is shown
  (`Item::display_text`, also used by search). Markdown export writes links as `[text](url)`.
- Tasks now have to be opened to be completed: the row checkbox is display-only and tapping
  it opens the item editor like the rest of the row. The editor has a "Done" checkbox for
  tasks that applies immediately.
- List manager: a new "In these lists" section at the top shows every list the item is in
  (unchecked ones stay until the manager closes, so an accidental uncheck is easy to undo),
  ordered highest-level first. Browse and search rows put the item's current lists first,
  then higher-level lists before deeper ones (`AppState::min_depths`, with a unit test).

## 2026-10-06 14:50 PDT
- Item editor: replaced the "Done" checkbox at the top with a green "Done" button at the
  bottom, next to Save/Cancel. It saves any pending edits, marks the task done (or a done
  task "Not done"), and closes the editor.
- Fixed the previous change's CSS, which had accidentally turned the trash button's
  `.trash-btn .child-count` badge rule into a global `.child-count` rule.

## 2026-10-06 15:30 PDT
- Inertial (momentum) touch scrolling now works everywhere. Item rows used
  `touch-action: none` and scrolled the page by hand with `scrollBy`, so a flick stopped
  dead the moment the finger lifted - and the `.item-main` rows on other pages (list
  manager, etc.) couldn't be scrolled by touch at all. Rows now use `touch-action: pan-y`,
  so the browser scrolls natively with momentum; the touch hold-to-drag still works,
  with a non-passive `touchmove` listener on each draggable row cancelling the native
  scroll only once a drag has armed.

## 2026-10-08 09:14 PDT
- Fixed sync failing every time ("Sync failed - the server couldn't be reached") after
  moving an item into a list it was already in. Moving rewrote the membership's parent,
  leaving two live memberships for the same (item, list) pair, which the server's
  `uq_membership_item_parent` index rejected - failing the whole sync with a 500.
  - Move into a list / move to root now just removes the moved membership when the item
    is already in the target list.
  - The server now stores such a duplicate membership as deleted instead of erroring,
    and the client pulls that back and drops its local duplicate, so clients already
    stuck this way recover on their next sync.

## 2026-10-08 14:17 PDT
- The Sync button's error now shows what actually went wrong instead of always saying
  "the server couldn't be reached": not signed in, request too large (413), the server's
  own error text (e.g. a database error), an unreadable response, or a real network
  failure. Per-item push/sync errors include the server's error text too.

## 2026-10-08 14:27 PDT
- Raised the backend's JSON body limit from Actix's 2 MB default to 20 MB, so a large
  sync push (e.g. the first push after a big import) isn't rejected. The reverse proxy's
  `client_max_body_size` for lister.xf2.us needs to be at least as large (nginx's
  default is 1 MB).
