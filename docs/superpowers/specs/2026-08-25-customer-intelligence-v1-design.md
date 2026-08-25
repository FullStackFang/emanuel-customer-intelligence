# Emanuel Customer Intelligence — v1 design ("grab the data")

Date: 2026-08-25
Status: approved in conversation, pending written review

## 1. Purpose

A desktop application for Temple Emanu-El staff that mirrors a governed subset of the
org's Salesforce data onto the local machine, profiles it so staff can see which fields
actually carry signal, and lets them build simple segments against it. No web server,
no cloud copy of member data, no vendor in the data path.

v1 stops at "data is flowing and explorable". Segmentation analytics, exports, and any
NPSP-specific modelling come after.

Starting point: the reviewed scaffold in `Downloads/non-profit-segmentation`
(Tauri v2 + Rust core + React screens). It is used as a base; this spec lists what
changes.

## 2. Non-negotiables (governance)

1. **The webview is untrusted.** It can only call a fixed set of named Rust commands.
   It never holds an OAuth token, never issues SQL, never makes a network request.
   Tauri capabilities: `core:default` + `opener:allow-open-url` only.
2. **Read-only against Salesforce.** No command writes to Salesforce. OAuth scopes are
   `api refresh_token openid id profile email`.
3. **Allowlist, not mirror-everything.** Row data is pulled only for objects the user
   explicitly selects, and only for fields not withheld (see §5.2).
4. **Nothing secret on disk in plaintext.** OAuth tokens and the DB encryption key
   live in Windows Credential Manager. The DB file is encrypted at rest.
5. **Append-only audit.** Every connect / scan / selection change / sync / query /
   withholding override is recorded locally. No command can edit or delete audit rows.
6. **Sensitive-by-default.** Fields whose name or type suggests pastoral, medical, or
   free-text content are withheld from sync unless a user overrides per field, and the
   override itself is audited.

## 3. Project layout

New sibling repo: `fullstackfang/emanuel-customer-intelligence/`.

```
emanuel-customer-intelligence/
  package.json              react 19, typescript, vite 6, @tauri-apps/api 2, lucide-react
  index.html
  vite.config.ts
  tsconfig.json             allowJs: true (design system is .jsx)
  .env.example              SF_CLIENT_ID=, SF_LOGIN_URL=https://emanu-el.my.salesforce.com
  .env                      gitignored; real consumer key
  .gitignore                .env, node_modules, dist, src-tauri/target, src-tauri/gen/schemas
  src/
    main.tsx
    App.tsx                 signed-out screen | AppFrame + routes
    api.ts                  typed invoke() wrappers, 1:1 with Rust commands
    design-system/          verbatim copy of emanuel-grant-management-app/src/design-system
    assets/emanuel_logo.png
    pages/
      OverviewPage.tsx
      DataPage.tsx          object allowlist (left) + explorer/profile (right)
      SegmentsPage.tsx
      AuditPage.tsx
  src-tauri/
    Cargo.toml
    tauri.conf.json
    capabilities/default.json
    src/
      main.rs               builder, state, command registration
      config.rs             reads SF_CLIENT_ID / SF_LOGIN_URL (dotenv in dev, env in prod)
      secrets.rs            keyring: tokens + db key
      auth.rs               PKCE + state + loopback + exchange + refresh + revoke + identity
      salesforce.rs         describe global / describe object / count / paginated query
      store.rs              SQLCipher open, catalog, objects, mirror tables, audit
      profile.rs            column profiler + sensitivity heuristic
      segment.rs            validated filter → SQL builder (pure, unit-tested)
      commands.rs           the invoke surface; thin; every command writes audit
```

The design system is copied, not shared, in v1. Extracting a package is a later task
if a third app needs it.

## 4. Auth

Authorization Code + PKCE, public client (no secret). `SF_LOGIN_URL` is the org's
My Domain (`https://emanu-el.my.salesforce.com`).

Flow (`auth::login`):
1. Generate 64-byte random `verifier`, `challenge = b64url(sha256(verifier))`,
   32-byte random `state`.
2. Bind a one-shot HTTP listener on `127.0.0.1:1717` **before** opening the browser
   (avoids the race where the redirect arrives first). If the port is busy, fail with a
   clear error; no port scanning.
3. Open the system browser at
   `{login_url}/services/oauth2/authorize?response_type=code&client_id=…&redirect_uri=http://localhost:1717/callback&code_challenge=…&code_challenge_method=S256&state=…&scope=api refresh_token openid id profile email`.
4. Accept exactly one request on the listener; parse query with a real parser (not
   `split("code=")`); require `state` to match, else discard and fail. Respond with a
   plain "You can close this tab" page. Listener times out after 5 minutes.
5. POST `{login_url}/services/oauth2/token` with `grant_type=authorization_code`,
   `code`, `client_id`, `redirect_uri`, `code_verifier`. Store `TokenSet
   {access_token, refresh_token, instance_url, id (identity url), issued_at}` in the
   keychain.
6. GET the identity URL → `{user_id, display_name, username, organization_id}`; cached
   in app state for the header chip and audit rows.

Refresh (`auth::ensure_fresh`): every Salesforce call goes through a helper that
retries once on HTTP 401 by POSTing `grant_type=refresh_token` and re-saving tokens.
No client secret is sent (ECA "Require Secret for Refresh Token Flow" is off).

Disconnect (`auth::logout`): POST `{login_url}/services/oauth2/revoke` with the refresh
token (best effort), delete keychain entries, clear cached identity. Does **not** delete
the local mirror — that is a separate, explicit `purge_local_data` command with its own
audit row.

Keychain: `keyring` crate, service `emanuel-customer-intelligence`, entries
`salesforce_tokens` (JSON) and `db_key` (hex). Windows Credential Manager blob limit
is ~2.5 KB; a Salesforce token set is well under that.

## 5. Data pipeline

### 5.1 Scan (`scan`)

Describe global → filter to `queryable && !customSetting && !deprecatedAndHidden`.
For each object: describe fields, skip compound/unselectable types (`address`,
`location`, `base64`), and fetch `SELECT COUNT() FROM Object` for a record count
(skip objects where count fails — some system objects reject it).

Writes to the catalog:

- `_objects(name PK, label, record_count, selected INT DEFAULT 0, last_synced_at, last_sync_rows)`
- `_fields(object, field, sf_type, label, sensitive INT, withheld INT, PRIMARY KEY(object, field))`

`sensitive` comes from the heuristic (§5.2). `withheld` is initialised to `sensitive`
on first scan; on re-scan existing `withheld` and `selected` values are preserved
(upsert, do not overwrite user decisions). Scan pulls **no row data**.

Scan of a few hundred objects is a few hundred describe calls; it runs in the
background and reports progress via a Tauri event `scan:progress {done, total}`.

### 5.2 Sensitivity heuristic

`profile::is_sensitive(field_name, sf_type)`:
- name contains any of: `note, medical, health, private, confidential, ssn, dob,
  birth, diagnos, pastoral, counsel, disab, allerg, emergency, death, deceased,
  yahrzeit, bereave, hospital, illness` (case-insensitive)
- or `sf_type ∈ {textarea, richtextarea, encryptedstring}`

Conservative on purpose: over-flag, let a human clear it.

### 5.3 Select (`set_object_selected`, `set_field_withheld`)

The Data page lists objects with record counts and a checkbox. Toggling writes
`_objects.selected` and an audit row. Fields flagged sensitive show a "withheld" badge;
a per-field toggle clears `withheld` and writes an audit row with `action =
'field.override'`. Clearing withholding on a field that is not flagged sensitive is a
no-op.

### 5.4 Sync (`sync_selected`)

For each `selected` object, in turn:
1. Columns = fields where `withheld = 0`.
2. `SELECT <cols> FROM <Object>` via REST `/query`, following `nextRecordsUrl`.
3. In one transaction: drop and recreate the mirror table `"<Object>"` with TEXT
   columns for exactly those fields, insert all rows, update
   `_objects.last_synced_at / last_sync_rows`.
4. Audit `sync.object {object, rows}`.

Full replace per object; incremental (`SystemModstamp`) sync and Bulk API 2.0 are v2.
Progress via event `sync:progress {object, rows_so_far}`. A failing object is recorded
in the audit row with its error and skipped; the run continues.

All values stored as TEXT, faithful mirror; the profiler infers meaning.

### 5.5 Profile (`profile_selected`, `get_profile`)

Per column of each synced object: `row_count, non_null, fill_rate, distinct_count,
top_values (top 5 "value (n)" joined by " | "), sensitive`. Top values are never
materialised for sensitive columns (they are not on disk anyway when withheld; the
guard remains for overridden ones — stored as `[hidden: sensitive]`). Written to
`_profile(object, field, …, PRIMARY KEY(object, field))`.

### 5.6 Segment (`query_segment`)

Request `{object, filters: [{field, op, value}], group_by?}`. `segment.rs` builds SQL
purely, given the list of valid columns for the object:
- object must exist in `_objects` with `last_synced_at IS NOT NULL`
- every `field` and `group_by` must exist in `_fields` for that object with
  `withheld = 0`
- `op ∈ {=, !=, >, <, >=, <=, contains}`; `contains` becomes `LIKE ?` with `%v%`
- values are always bound parameters; identifiers are double-quoted after passing an
  `^[A-Za-z0-9_]+$` check (rejected otherwise — no silent character replacement)
- group-by breakdown limited to top 20

Returns `{count, breakdown: [[label, n]]}`. Audited as `segment.query` with the object
and filter field names (not values).

## 6. Storage

SQLCipher-encrypted SQLite via `rusqlite` `bundled-sqlcipher-vendored-openssl`
(Windows needs the vendored OpenSSL). File:
`%APPDATA%\emanuel-customer-intelligence\mirror.db` (Tauri `app_data_dir`).

Key: 32 random bytes, hex-encoded, generated on first open, stored in the keychain as
`db_key`. `PRAGMA key` is applied immediately after open; `PRAGMA cipher_memory_security
= ON`.

**Build probe is task 1 of the implementation plan.** If `bundled-sqlcipher-vendored-openssl`
does not compile on the MSVC toolchain within a bounded effort, fall back to DuckDB
(`duckdb` crate, `bundled`) using DuckDB ≥ 1.4 native encryption
(`ATTACH … (ENCRYPTION_KEY …)`). The `store.rs` interface is the same either way.

Schema tables (all created on open, `IF NOT EXISTS`): `_objects`, `_fields`,
`_profile`, `_audit`, plus one mirror table per synced object.

## 7. Audit

`_audit(id INTEGER PK, at TEXT ISO-8601 UTC, sf_user_id TEXT, sf_username TEXT,
action TEXT, object TEXT NULL, detail TEXT JSON NULL)`.

Actions: `auth.connect`, `auth.disconnect`, `auth.refresh_failed`, `scan.run`,
`object.select`, `object.deselect`, `field.override`, `field.rewithhold`,
`sync.object`, `sync.object_failed`, `profile.run`, `segment.query`, `data.purge`.

The only commands touching `_audit` are inserts inside other commands and
`get_audit(limit, offset)`. There is no update/delete path in Rust; SQLCipher plus the
keychain-held key means the file cannot be edited outside the app without the key.
(Audit is tamper-evident for the app's users, not for a local administrator with the
key — acceptable for v1; a remote mirror is a later option.)

## 8. Command surface (`commands.rs`)

| command | args | returns |
|---|---|---|
| `get_status` | — | `{connected, identity?, last_scan_at?, object_count, selected_count, synced_rows}` |
| `connect` | — | `Identity` |
| `disconnect` | — | — |
| `scan` | — | `{objects}` (emits `scan:progress`) |
| `list_objects` | — | `ObjectRow[]` |
| `set_object_selected` | `{object, selected}` | — |
| `list_fields` | `{object}` | `FieldRow[]` (incl. `sensitive`, `withheld`, profile if present) |
| `set_field_withheld` | `{object, field, withheld}` | — |
| `sync_selected` | — | `{objects_synced, rows}` (emits `sync:progress`) |
| `profile_selected` | — | — |
| `query_segment` | `SegmentReq` | `SegmentResult` |
| `get_audit` | `{limit, offset}` | `AuditRow[]` |
| `purge_local_data` | — | — (drops mirror tables + profile, keeps catalog + audit) |

Every command returns `Result<T, String>` with a user-readable message; internal
errors are logged with `tracing`, never with token contents.

## 9. Frontend

React 19 + TypeScript. `src/design-system/` is a verbatim copy of the grants app's
(JSX; `allowJs`). Fonts: the design system's `fonts.css` imports Google Fonts — the
Tauri webview has network access for that, but the app must render acceptably offline
with the fallback stack (system-ui), which the token file already provides.

Screens (all inside `AppFrame`, nav = Overview · Data · Segments · Audit):

- **Signed-out**: mirrors the grants app sign-in card; single "Connect to Salesforce"
  button; explains that login happens in the browser.
- **Overview**: status card (org, user, connected since), pipeline stat row (objects
  scanned / selected / rows mirrored / last sync), next-action button (Scan → Select →
  Sync → Profile), Disconnect and Purge local data (the latter behind a confirm modal).
- **Data**: left column = object list with record count, checkbox, synced badge,
  search; right = fields table for the selected object: field, type, fill bar,
  distinct, top values, sensitive/withheld badges, override toggle. "Only populated"
  filter as in the scaffold.
- **Segments**: the scaffold's Segment Builder with design-system `Select`/`Input`/
  `Button`/`Card`; bar breakdown uses design tokens.
- **Audit**: `Table` of audit rows, newest first, paged.

No emoji, no gold text under 18px, Title Case nav — per the design system README.

## 10. Error handling

- OAuth failures (state mismatch, timeout, token exchange error) surface as an `Alert`
  on the signed-out screen with a retry; never auto-retry the browser launch.
- 401 mid-session → one refresh attempt → if it fails, app drops to signed-out with an
  "Session expired" alert; local data stays.
- Per-object failures during scan/sync are collected and shown in a summary alert,
  not thrown for the whole run.
- Port 1717 busy → explicit message naming the port.

## 11. Testing

Rust (`cargo test`):
- `auth`: PKCE challenge derivation against a known vector; callback query parsing
  (missing code, wrong state, extra params).
- `segment`: SQL builder — valid build; rejects unknown field, withheld field, bad op,
  identifier with quotes/semicolons/spaces; values never interpolated.
- `store`: open with key → write → reopen with same key reads; reopen with wrong key
  fails; audit insert then verify no `DELETE`/`UPDATE` helpers exist (compile-time by
  construction — tested by the absence of an API).
- `profile`: sensitivity heuristic table test.

Frontend (`vitest`): `api.ts` wrappers call `invoke` with the right names/args
(mock `@tauri-apps/api/core`).

Manual E2E against the production org (read-only):
1. `npm run tauri dev` → Connect → browser login → header shows name.
2. Scan → Data page lists objects with counts.
3. Select one small object (e.g. `Campaign`) → Sync → Profile → fields table populated.
4. Segments → filter + group-by returns counts.
5. Audit page shows every step above.
6. Disconnect → signed-out; reconnect works without re-scan.

## 12. Out of scope for v1

Writes to Salesforce · export/CSV · incremental sync · Bulk API 2.0 · multi-user
distribution and code signing · shared design-system package · AI/agents ·
NPSP-specific analytics (households, donor RFM, etc.) · remote audit mirror.
