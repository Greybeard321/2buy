# 2buy — Shared Shopping List

Status: draft v0.8
Last updated: 2026-11-19

## 1. Overview

A small web app that hosts **one shared shopping list** on a server so family members can
coordinate groceries and supplies. There is no registration and no password login. The
admin generates a personal bookmark URL for each user; possession of that URL is the
only credential needed to use the app.

### 1.1 Goals

- One central list, accessible from any browser on the network/internet.
- Extremely low onboarding: the admin hands out a URL, the user bookmarks it.
- Every user can add, edit (including rename), disable/enable and remove items.
- Item names are unique on the list; adding an existing name updates that item.
- Items can be grouped and filtered (touch-friendly dropdown) by a free-text category
  matched case-insensitively, so a trip to a specific store can be planned.
- Tokens can be issued and revoked individually, with several tokens per person.
- Minimal operational and code complexity.

### 1.2 Non-goals

- No public sign-up, password recovery, or profile management.
- No per-user lists, permissions, or read-only roles in v1.
- No purchase history or multi-step completion workflow; `disabled` only crosses an
  item out and can be reversed. No undo after removal.
- No separate admin web UI; administration happens on the server via CLI/config.
- No realtime updates; users refresh/reload the page to see changes.

## 2. Terminology

| Term | Meaning |
| --- | --- |
| Admin | The owner/operator of the server. Creates and revokes user URLs. |
| User / member | A family member holding one or more personal URLs. |
| User URL | `https://<host>/u/<username>/<token>` — bookmarkable, bearer credential. |
| Token | An opaque random secret embedded in the URL. |
| Token ID | Short non-secret identifier used by the admin CLI to list and revoke a token. |
| Token registry | Server-side table with one row per generated URL: username, token, creation time, revocation state. |
| Item | One entry on the shared list. |
| Category | A free-text label typed by users; labels are compared case-insensitively after trimming, and matching labels form one group. |
| Disabled item | An item kept on the list but crossed out, e.g. because it was already bought or is no longer needed. It sorts to the bottom of its group. |
| Reuse | Submitting the name of an existing (including disabled) item in the add form. This edits and re-enables the original item instead of creating a duplicate. |
| Hide disabled | A view-only toggle in the frontend that removes crossed-out items from the display without changing any data. |
| View settings | The browser-session settings for sort order, category filter and hide-disabled toggle, stored in a session cookie. |

## 3. Access model

### 3.1 Single shared list

All users see and modify the **same** list. There are no private lists. Changes made by
one user become visible to others after they reload the page (§5.4).

### 3.2 Token issuance and registry

- The admin generates a URL per person (and per device, if desired) with a CLI command:

  ```sh
  2buy generate-url alice
  # Token ID: 7f3a9c
  # URL: https://shopping.example.com/u/alice/Rm9vLmJhcl9iYXpf...
  ```

- Each call generates a **new random token** (e.g. 32 bytes from a CSPRNG, base64url
  encoded) and adds a row to the token registry:

  | Column | Purpose |
  | --- | --- |
  | `id` | Short, non-secret token ID used by the admin CLI (e.g. `7f3a9c`) |
  | `username` | The username this token is bound to |
  | `token` | The **full token** in plaintext, so the admin can reproduce the exact URL later; unique across rows |
  | `created_at` | Creation timestamp (UTC) |
  | `revoked_at` | `NULL` while active; timestamp once revoked |

  A short prefix of `token` (first ~8 characters) is computed on display for
  `list-tokens`; it does not need its own column.
- **Multiple tokens for the same username are explicitly supported** (phone, laptop,
  a second bookmark for the same person). Each generated URL gets its own row.
- The registry is the source of truth for authentication: no token works unless it has
  an active row. This makes individual revocation possible without touching other URLs.
- Because the full token is stored, the admin can reprint the **same** URL at any time
  with `2buy show-url <token-id>`; calling it twice yields the identical URL. This is
  useful when a user loses their bookmark, and it does not create a new token.

### 3.3 Username binding

A token is only valid together with the username it was generated for. On every request:

1. Parse `username` and `token` from the URL path.
2. Validate the username against `^[a-z0-9][a-z0-9_-]{0,31}$`.
3. Look up the registry row by the full token (unique index; compare in constant time
   where practical).
4. Compare the row's username to the path username (constant-time comparison is
   preferred) and require `revoked_at IS NULL`.
5. On any failure return `403 Forbidden` with a generic message that does not reveal
   whether the username or token exists.

Consequences:

- Editing the username in a valid URL fails the username check.
- Copying a token from Alice's URL into Bob's URL looks up Alice's row, which does not
  match the username `bob`, so it fails too.
- Usernames are display labels only ("added by alice"); no user account records exist.

### 3.4 Revocation

- Admin command: `2buy revoke <token-id>` sets `revoked_at` on that one registry row.
- Revoked rows are kept for auditing and are shown by `list-tokens` as `revoked`.
- Revoking one token does not affect any other token, including other tokens for the
  same username.
- A revoked URL immediately receives `403` on all pages and API calls.
- Revocation is **permanent**: there is no reactivation command. To restore access,
  generate a new URL with `generate-url`.
- If a URL is merely lost, the admin uses `show-url` to recover the same URL. If it
  leaked, the admin revokes its ID and generates a fresh one.

### 3.5 Admin interface

CLI only:

| Command | Effect |
| --- | --- |
| `2buy generate-url <username>` | Create a new token and print its ID and full URL |
| `2buy list-tokens [--username <name>]` | Show ID, username, token prefix, creation time, status |
| `2buy show-url <token-id>` | Print the exact stored URL again; repeatable, no new token |
| `2buy revoke <token-id>` | Revoke exactly one token |
| `2buy list-items` | Optional: inspect current list for debugging |

Configuration (environment variables or one config file):

| Setting | Purpose |
| --- | --- |
| `BASE_URL` | Public base URL used when printing user URLs |
| `DATABASE_PATH` | Where the SQLite database is stored |
| `LIST_TITLE` | Optional heading shown in the UI |

### 3.6 Threat model / security requirements

- A URL is a **bearer secret**: anyone who has it can add, edit, disable and remove
  items. Treat sharing a URL like sharing a password; users must not post it publicly.
- The registry stores full tokens so the admin can reproduce URLs. These are bearer
  credentials, so protect `DATABASE_PATH` and its backups with strict filesystem
  permissions; anyone who can read the database can use every active URL. Optional
  hardening: encrypt the `token` column at rest with a key only the admin CLI can read.
- Full tokens/URLs are printed only by explicit admin CLI commands. Never write them
  to application logs or share them except with the intended user.
- Serve everything over **HTTPS**; redirect HTTP to HTTPS.
- Set `Referrer-Policy: no-referrer` and avoid third-party assets/analytics so the URL
  cannot leak via referrers or external logs.
- Do not log full request URLs containing tokens (log a path template + username, or
  redact the token).
- No externally reachable admin endpoint; admin actions require local server access.
- Basic rate limiting on API endpoints is desirable but not required for v1.

## 4. Data model

### 4.1 `tokens` table

As defined in §3.2: `id`, `username`, `token` (full value), `created_at`, `revoked_at`.
One row per URL ever generated, so the exact URL can be reconstructed at any time.

### 4.2 `items` table

| Column | Type | Constraints / notes |
| --- | --- | --- |
| `id` | integer primary key | Auto increment; opaque to the frontend |
| `name` | text | Required; trimmed; **unique across the whole list**; max 120 chars; free text |
| `quantity` | text | Required (default `1`); free text, e.g. `2`, `1 bottle`, `a handful`; max 40 chars |
| `category` | text | Required; free text, e.g. `supermarket`, `Hardware store`, `Office`; max 80 chars |
| `comment` | text | Optional; max 500 chars; empty means "no comment" |
| `disabled` | boolean (integer 0/1) | Default `false`; `true` means crossed out / no longer needed; never deletes the item |
| `created_by` | text | Username from the URL; internal only, not shown in UI |
| `created_at` | timestamp | Server-set (UTC) |
| `updated_at` | timestamp | Server-set on creation and on every edit/disable/enable/reuse |

- **All four item content fields are free text.** There is no server-side category list
  and no category enum.
- Groups are formed **implicitly** by items whose category strings match
  **case-insensitively after trimming**. `Hardware`, `hardware` and `HARDWARE` are one
  group. The group heading uses the spelling of the first-created item in that group.
- Item names are likewise compared **case-insensitively after trimming**:
  `Milk` and `milk` are the same item for reuse, conflict detection and autocomplete.
  A unique index on the normalized name key (e.g. `lower(trim(name))`) enforces this
  even with concurrent writes or renames. On reuse/rename the newly submitted spelling
  is stored, while the item's ID, creator and creation time are preserved.
- `created_by`, `created_at` and `updated_at` are **internal metadata only**; they are
  stored for debugging/audit purposes and are never displayed in the frontend UI.
- Disabling is reversible and affects only presentation/state of that one item. Disabled
  items remain in their category group and in all API responses, but sort after all
  enabled items in that group.

## 5. Functional requirements

### 5.1 Frontend

- A single-page web app served by the same server, with no user data baked in at build time.
- Header shows the list title and the current username (from the URL).
- Main view: items grouped by their category string (case-insensitive match, heading
  from the first-created item in the group). Within each group, **enabled items always
  come first and disabled items always go last**, regardless of the selected sort;
  within each of those two blocks the chosen sort order applies.
- **Sort control**: a dropdown with `Oldest first` (default: by `created_at`
  ascending), `Newest first` (`created_at` descending) and `Name A–Z`
  (case-insensitive by name). Disabled items remain at the bottom of their group in
  every mode.
- Each item displays only name, quantity, category and comment (if present), plus three
  controls: **Edit**, **Disable/Enable** toggle, and **Remove**. Attribution and
  timestamps (`created_by`, `created_at`, `updated_at`) are not shown.
- A disabled item is rendered crossed out (line-through) and visually muted, including
  its category and comment. The toggle button then reads **Enable**. An enabled item
  has a **Disable** button.
- Add form fields: name (required), quantity (default `1`), category (required),
  comment (optional). Edit reuses the same form, prefilled with the item's values
  (including its name), and allows **renaming** it. Editing preserves the item's
  disabled state unless the user explicitly toggles it.
- **Reuse notice**: if the entered name matches an existing item (case-insensitively),
  the form indicates
  that submitting will update (and re-enable, if disabled) that item rather than create
  a new one. The server enforces this regardless of what the UI shows.
- Remove requires a simple confirmation (inline confirm or native `confirm()`);
  no undo in v1.
- **Autocomplete** for free-text fields, sourced from **all** items on the list,
  **including disabled ones**:
  - name input: suggests existing item names
  - category input: suggests existing category names
  - Suggestions come from the full item list, not the filtered view. Selecting a
    suggestion does not itself create, change or enable anything; only submitting does.
- **Filter**: a single touch-friendly **dropdown** (`<select>`) with `All categories`
  plus one option per distinct category present in the list (case-insensitive
  grouping, label from the group heading). Selecting a category shows only that
  group; default is `All categories`. Filtering is view-only and never modifies data.
- **Hide disabled toggle**: a single checkbox/button, default off (disabled items
  visible and crossed out). When on, all disabled items are hidden from every group;
  groups left with no visible items disappear as well. This is view-only and does not
  modify the list.
- **View settings persistence**: sort order, selected category filter and the
  hide-disabled toggle are stored **per browser session in a cookie** (e.g.
  `2buy_view`), as URL-encoded JSON such as
  `{"sort":"created_asc","category":"hardware store","hideDisabled":false}`.
  The cookie is a **session cookie** (no `Expires`/`Max-Age`), `Path=/`, `SameSite=Lax`,
  and `Secure` when served over HTTPS. It contains no token or secret and is readable
  by JavaScript (`HttpOnly` is off). Missing or invalid values fall back to defaults.
  Settings therefore survive page reloads within a browser session, are lost when the
  browser session ends, and are not tied to a particular user URL.
- Works on desktop and mobile browsers; no login screen.

### 5.2 API

All endpoints except the static frontend require a valid username/token path prefix as
described in §3.3.

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/u/{username}/{token}/` | Serve frontend app; token still validated |
| `GET` | `/api/{username}/{token}/items` | List all items (JSON array), including disabled ones |
| `POST` | `/api/{username}/{token}/items` | Create item, or **update the existing item with the same name** and set `disabled=false`; returns the item |
| `PUT` | `/api/{username}/{token}/items/{id}` | Replace all content fields (including `name`, so it can rename) and `disabled`; returns updated item |
| `PATCH` | `/api/{username}/{token}/items/{id}` | Set only `{"disabled": true\|false}`; returns updated item |
| `DELETE` | `/api/{username}/{token}/items/{id}` | Remove item by ID |

- `POST` semantics (upsert by name, compared case-insensitively after trimming):
  - If no item has that normalized name: insert a new enabled item.
  - If an item has that name (enabled or disabled): treat it as an edit of that item —
    store the submitted spelling, `quantity`, `category` and `comment`, set
    `disabled=false`, keep its `id`, `created_by` and `created_at`, and set
    `updated_at`. No duplicate is created.
  - The response should indicate whether the result was `created` or `updated` so the
    UI can show the reuse notice/feedback accurately.
- A `PUT` that renames an item to a normalized name used by a **different** item returns
  `409 Conflict`. Renaming to a different capitalization/spelling of its own current
  name is allowed. `PATCH` and `DELETE` cannot create duplicates.
- Validation failures return `400` with a JSON error message; unknown item IDs return
  `404`; authentication failures return generic `403`.
- The username always comes from the validated URL path; a username supplied in a
  body/header is ignored.
- The toggle button uses `PATCH`; the Edit form uses `PUT` and therefore must send the
  current `disabled` value along with the four content fields.

### 5.3 Syncing

- **No automatic syncing in v1.** Users press the browser reload button (or a small
  "Refresh" button that reloads/re-fetches) to see changes made by others.
- The URL is the only state a user needs to keep; the list itself lives on the server
  and survives reloads, device changes and server restarts.

## 6. Proposed technical shape (not fixed)

- Backend: one small HTTP service (e.g. Python + stdlib/FastAPI, Go, or Node) using
  SQLite for both the item list and the token registry. Single process, no external DB.
- Frontend: plain HTML/CSS/vanilla JS served by the same process; `<datalist>` is enough
  for autocomplete, no framework needed.
- Deployment: run behind a reverse proxy that terminates TLS, or directly with TLS if
  the runtime supports it. Back up the SQLite file.

## 7. Example user flows

1. Admin runs `2buy generate-url alice`, then again for Alice's phone, and sends both
   URLs. `list-tokens` shows two active rows for `alice` with different IDs and creation
   times.
2. Bob opens his bookmarked URL and sees the current shared list grouped by category,
   with crossed-out items at the bottom of each group.
3. Bob types `hard` in the category field; autocomplete suggests `hardware store`
   case-insensitively (even if that category only exists on disabled items). He adds a
   screwdriver with quantity `2`. To plan his trip he picks `hardware store` from the
   category filter dropdown.
4. Alice buys one screwdriver. She edits the item, changing the quantity to `1` and the
   name to `screwdriver (phillips)`; the rename succeeds because the new name is unused.
   Later, after buying the last one, she clicks **Disable**; the item is crossed out and
   moves to the bottom of its group for everyone. Bob switches on **Hide disabled** to
   get it out of his way while shopping; the item is still on the server.
5. Weeks later Bob needs another screwdriver. He starts typing `screwdriver`; the
   disabled item `screwdriver (phillips)` is suggested (autocomplete ignores the
   **Hide disabled** view setting). He submits that exact name with quantity `1`: the
   server updates the original item's fields, re-enables it, and returns `updated`.
   There is still only one `screwdriver (phillips)` item, back at the top of its group.
6. Bob accidentally disables an item he still needs. He clicks **Enable** on that item
   and reloads to confirm the state is gone for everyone.
7. Bob tries changing his URL's username to `alice`; he gets a generic `403`.
8. Alice loses the bookmark for her laptop URL. Admin finds its ID with
   `2buy list-tokens --username alice` and runs `2buy show-url <id>` twice; both calls
   print the same URL, which she bookmarks again.
9. The phone is lost (not just its bookmark). Admin identifies the phone's token ID and
   runs `2buy revoke <id>`. The phone URL stops working; Alice's other URL keeps working.

## 8. Acceptance criteria

- A URL generated by the admin works in a fresh browser without any registration.
- Two tokens can be generated for the same username and both work independently.
- `list-tokens` shows username, token (prefix), creation time and status for every
  generated URL, including revoked ones.
- Revoking one token makes only that URL fail with `403`; other tokens (including other
  tokens for the same username) still work. Revocation is permanent; there is no
  reactivation command, so access requires generating a new URL.
- Changing the username segment of a valid URL is rejected with `403`.
- Reusing a token with another username is rejected with `403`.
- The registry stores the full token for each generated URL, together with its username
  and creation time.
- `2buy show-url <token-id>` prints the exact same URL every time it is called; it does
  not generate a new token or change the registry. This also works for revoked tokens,
  though the resulting URL is still rejected with `403`.
- Two browsers with different user URLs see the same items; after a page reload each
  sees the other's adds, edits, disable/enable toggles and removals.
- Any user can add an item with free-text name, quantity, category and optional comment,
  edit all four fields of an existing item (including renaming it), remove any item,
  and disable or re-enable it.
- Disabled items remain on the list, are shown crossed out/muted, sort to the bottom of
  their category group, and can be enabled again; the state survives a reload and a
  server restart.
- The **Hide disabled** toggle removes all disabled items (and empty groups) from the
  view without changing any data; switching it off shows them again at the bottom of
  their groups.
- Item names are unique case-insensitively after trimming: `Milk` and `milk` can never
  be two different items.
- Creating an item with an existing name (any capitalization) updates that item
  instead: its stored spelling, quantity, category and comment change, it is re-enabled
  if it was disabled, and its ID, creator and creation time stay the same.
- Renaming an item to an unused name succeeds and preserves its ID and creation time;
  changing only the capitalization of its own name is also allowed.
- Renaming an item to a name that matches a different item case-insensitively returns
  `409 Conflict`.
- Autocomplete for item names and category names includes disabled items and matches
  case-insensitively.
- Category groups are derived from case-insensitive category-string matches; the
  filter is a single dropdown and hides groups without modifying data.
- The sort dropdown offers oldest first (default), newest first and name A–Z; disabled
  items stay at the bottom of their group in every mode.
- Sort order, category filter and hide-disabled state are stored in a session cookie
  and restored after a page reload within the same browser session; the cookie holds
  no token or secret, and a new browser session starts from defaults.
- The UI never displays `created_by`, `created_at` or `updated_at`; they remain internal
  server-side metadata.
- Data and token registry survive server restarts (SQLite file).

## 9. Open questions to settle before implementation

1. **Deployment details** — public internet vs. home LAN only, and which
   language/framework do you prefer for the server?
