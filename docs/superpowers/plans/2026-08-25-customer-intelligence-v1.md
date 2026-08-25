# Emanuel Customer Intelligence v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Tauri desktop app that authenticates to Salesforce with PKCE, mirrors only user-selected objects (minus withheld sensitive fields) into an encrypted local SQLite file, profiles the columns, and lets staff build simple segments — with every action audited.

**Architecture:** Untrusted React webview ↔ fixed set of Rust `#[tauri::command]`s ↔ trusted Rust core (OAuth + keychain, Salesforce REST client, SQLCipher store, profiler, validated segment SQL builder). All network and SQL live in Rust. The frontend is the grants app's design system (copied verbatim) around four pages: Overview, Data, Segments, Audit.

**Tech Stack:** Tauri 2.11 · Rust (stable MSVC) · rusqlite 0.40 `bundled-sqlcipher-vendored-openssl` · keyring 4 · reqwest 0.13 · React 19 + TypeScript + Vite · lucide-react · Vitest

**Spec:** `docs/superpowers/specs/2026-08-25-customer-intelligence-v1-design.md` — read it first; this plan implements it section by section.

## Global Constraints

- Project root: `C:\Users\Stephen.Fang\OneDrive\Documents\workspace\github.com\fullstackfang\emanuel-customer-intelligence` (already a git repo with the spec committed). All paths below are relative to it. The design system source to copy is `../emanuel-grant-management-app/src/design-system/` and `../emanuel-grant-management-app/src/assets/emanuel_logo.png`.
- Webview capability: `core:default` only. No `fs`, `shell`, or `http` plugin permissions, ever.
- OAuth scopes exactly: `api refresh_token openid id profile email`. Login URL `https://emanu-el.my.salesforce.com`. Redirect `http://localhost:1717/callback`. Public client — never send a `client_secret`.
- Salesforce REST API version: `v62.0`.
- `.env` is gitignored and holds `SF_CLIENT_ID` (the Consumer Key the user supplied in chat) and `SF_LOGIN_URL`. Never write the key into any committed file, test, or doc.
- Secrets in Windows Credential Manager under service `emanuel-customer-intelligence`, entries `salesforce_tokens` and `db_key`. Tests use service `emanuel-customer-intelligence-test`.
- DB file: `{app_data_dir}/mirror.db` (Tauri `app.path().app_data_dir()`). Never in the project folder.
- SQL identifiers (object/field names) must match `^[A-Za-z0-9_]+$` and are rejected otherwise — no character replacement. Values are always bound parameters.
- Audit table `_audit`: inserts only. No Rust function may `UPDATE` or `DELETE` from it.
- UI copy: Title Case nav and page titles, sentence case elsewhere, no emoji, no gold text under 18px, JetBrains Mono for API names (`--font-mono`).
- Commit after every task with a conventional-commit message. Do not commit `.env`.
- Rust: `cargo fmt` before each commit; `cargo test` must pass. Run cargo commands from `src-tauri/`.

---

## File map

| Path | Responsibility |
|---|---|
| `src-tauri/src/lib.rs` | module declarations, `run()`: builder, state, command registration |
| `src-tauri/src/main.rs` | template entry, calls `run()` |
| `src-tauri/src/config.rs` | `Config { client_id, login_url }` from env/.env |
| `src-tauri/src/secrets.rs` | `Secrets` keychain wrapper: get/set/delete, `db_key()` |
| `src-tauri/src/auth.rs` | `TokenSet`, `Identity`, PKCE, `authorize_url`, `parse_callback`, `login`, `refresh`, `revoke`, `fetch_identity` |
| `src-tauri/src/salesforce.rs` | `SfClient`: describe global/object, count, paginated query, 401→refresh |
| `src-tauri/src/store.rs` | `Store` over SQLCipher: schema, catalog, selection, mirror tables, audit, status, `ident()` |
| `src-tauri/src/profile.rs` | `is_sensitive`, `profile_object`, `profile_all` |
| `src-tauri/src/segment.rs` | `build()` pure SQL builder + `run()` |
| `src-tauri/src/commands.rs` | `AppState`, all `#[tauri::command]`s, progress events |
| `src-tauri/capabilities/default.json` | `core:default` only |
| `src/api.ts` | typed `invoke` wrappers + event listeners |
| `src/App.tsx` | signed-out screen vs `AppFrame` + page switch |
| `src/pages/OverviewPage.tsx` | status, stats, next action, disconnect/purge |
| `src/pages/DataPage.tsx` | object allowlist + fields/profile table + withhold toggles |
| `src/pages/SegmentsPage.tsx` | filters + group-by + result |
| `src/pages/AuditPage.tsx` | paged audit table |
| `src/design-system/` | verbatim copy (one-line eyebrow edit in `chrome.jsx`) |

---

### Task 1: Scaffold the Tauri project and copy the design system

**Files:**
- Create: everything `create-tauri-app` generates (`package.json`, `index.html`, `vite.config.ts`, `tsconfig.json`, `src/`, `src-tauri/`)
- Create: `.env.example`, `.env`
- Create: `src/design-system/**` (copied), `src/assets/emanuel_logo.png` (copied)
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `tsconfig.json`, `src/design-system/ui-kits/grant-management/chrome.jsx` (one line)

**Interfaces:**
- Produces: a project where `npm run tauri dev` opens a window; `import { Button } from './design-system'` works from TS.

- [ ] **Step 1: Scaffold into the existing folder**

Run from the workspace parent (`fullstackfang/`):

```bash
npx --yes create-tauri-app@latest emanuel-customer-intelligence --manager npm --template react-ts --identifier org.emanuelnyc.customerintelligence --yes --force
cd emanuel-customer-intelligence
npm install
npm install react@19 react-dom@19 lucide-react
npm install -D @types/react@19 @types/react-dom@19 vitest
```

Expected: `src-tauri/` with `Cargo.toml`, `tauri.conf.json`, `capabilities/default.json`, `src/main.rs`, `src/lib.rs`, `icons/`; `src/` with `App.tsx`, `main.tsx`, `vite-env.d.ts`. The `--force` flag is needed because the folder already contains `.git`, `.gitignore`, and `docs/`. If the generator overwrote `.gitignore`, re-add the lines `.env`, `.env.*`, `!.env.example`, `src-tauri/target/`, `src-tauri/gen/schemas/`.

- [ ] **Step 2: Baseline dev run**

Run: `npm run tauri dev`
Expected: a window titled "emanuel-customer-intelligence" with the template greeting. First Rust compile takes several minutes. Close the window (Ctrl+C in the terminal).

- [ ] **Step 3: Env files**

Create `.env.example`:

```
# Salesforce External Client App — Consumer Key (public client id, no secret)
SF_CLIENT_ID=
# Org My Domain login URL
SF_LOGIN_URL=https://emanu-el.my.salesforce.com
```

Create `.env` with the same two lines, `SF_CLIENT_ID=` set to the Consumer Key the user provided in the conversation. Verify `git status` does NOT list `.env`.

- [ ] **Step 4: Copy the design system and logo**

```bash
cp -r ../emanuel-grant-management-app/src/design-system src/design-system
mkdir -p src/assets && cp ../emanuel-grant-management-app/src/assets/emanuel_logo.png src/assets/
```

Then edit ONE line in `src/design-system/ui-kits/grant-management/chrome.jsx`: the eyebrow under "Temple Emanu-El" in the header reads `Philanthropic Fund`; change that text to `Customer Intelligence`. Nothing else changes.

- [ ] **Step 5: tsconfig for JSX design system**

In `tsconfig.json`, inside `compilerOptions`, add `"allowJs": true` and make sure `"jsx": "react-jsx"` is present. Confirm `src/vite-env.d.ts` contains `/// <reference types="vite/client" />` (needed for the `.png` import).

- [ ] **Step 6: Tauri config and capability**

Edit `src-tauri/tauri.conf.json`:
- `productName`: `Emanuel Customer Intelligence`
- `identifier`: `org.emanuelnyc.customerintelligence`
- `app.windows[0]`: `"title": "Emanuel Customer Intelligence", "width": 1280, "height": 840, "minWidth": 1024, "minHeight": 700`

Replace `src-tauri/capabilities/default.json` with:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Least privilege: the webview may only call the app's own commands. No fs, shell, http, or opener access from JS — all egress happens in Rust.",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

- [ ] **Step 7: Prove the design system renders**

Replace `src/App.tsx` with:

```tsx
import "./design-system/styles.css";
import { Button, Card, CardHeader, CardTitle } from "./design-system";

export default function App() {
  return (
    <div style={{ padding: "var(--space-8)" }}>
      <Card>
        <CardHeader><CardTitle>Design system smoke</CardTitle></CardHeader>
        <Button onClick={() => alert("ok")}>Primary Button</Button>
      </Card>
    </div>
  );
}
```

Delete `src/App.css` and any `import "./App.css"` line if the template created one. Run `npm run tauri dev`. Expected: a white card with a sapphire "Primary Button" in DM Sans on a warm off-white background. Run `npx tsc --noEmit` — expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add -A
git status   # confirm .env is NOT staged
git commit -m "chore: scaffold Tauri v2 app with Emanuel design system"
```

---

### Task 2: SQLCipher store — build probe, open with key, schema

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/store.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod store;`)

**Interfaces:**
- Produces: `store::open(path: &Path, key_hex: &str) -> anyhow::Result<Store>`; `Store::conn(&self) -> &Connection`; `store::ident(name: &str) -> anyhow::Result<String>` (validated, double-quoted identifier).

- [ ] **Step 1: Dependencies**

Replace the `[dependencies]` section of `src-tauri/Cargo.toml` with:

```toml
[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time"] }
reqwest = { version = "0.13", features = ["json"] }
rusqlite = { version = "0.40", features = ["bundled-sqlcipher-vendored-openssl"] }
keyring = "4"
sha2 = "0.11"
base64 = "0.23"
getrandom = "0.4"
hex = "0.4"
url = "2"
tiny_http = "0.12"
dotenvy = "0.15"
chrono = { version = "0.4", features = ["clock"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"

[dev-dependencies]
tempfile = "3"
```

Keep the template's `[build-dependencies]` and `[lib]` sections as generated.

- [ ] **Step 2: Build probe**

Run from `src-tauri/`: `cargo build 2>&1 | tail -30`
Expected: success (vendored OpenSSL compiles with Perl 5.38 present; takes 5–15 minutes the first time).

If it fails in `openssl-sys`/`libsqlite3-sys` after one honest retry, STOP and report the error to the user with this fallback, do not improvise further: switch the dependency to `duckdb = { version = "1", features = ["bundled"] }` and reimplement `store::open` with `ATTACH '<path>' AS db (ENCRYPTION_KEY '<key>')`. The rest of `Store`'s interface stays identical.

- [ ] **Step 3: Write the failing tests**

Create `src-tauri/src/store.rs`:

```rust
//! Local encrypted mirror: schema, catalog, selection, mirror tables, audit.
//! Everything is TEXT — a faithful mirror; the profiler infers meaning.

use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use std::path::Path;

pub struct Store {
    conn: Connection,
}

/// Validate a Salesforce API name and return it double-quoted for SQL.
/// Rejects anything outside [A-Za-z0-9_] — no silent replacement.
pub fn ident(name: &str) -> Result<String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(anyhow!("invalid identifier: {name:?}"));
    }
    Ok(format!("\"{name}\""))
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS _meta(key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE IF NOT EXISTS _objects(
  name TEXT PRIMARY KEY, label TEXT, record_count INTEGER,
  selected INTEGER NOT NULL DEFAULT 0, last_synced_at TEXT, last_sync_rows INTEGER);
CREATE TABLE IF NOT EXISTS _fields(
  object TEXT, field TEXT, sf_type TEXT, label TEXT,
  sensitive INTEGER NOT NULL DEFAULT 0, withheld INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(object, field));
CREATE TABLE IF NOT EXISTS _profile(
  object TEXT, field TEXT, row_count INTEGER, non_null INTEGER, fill_rate REAL,
  distinct_count INTEGER, top_values TEXT, sensitive INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(object, field));
CREATE TABLE IF NOT EXISTS _audit(
  id INTEGER PRIMARY KEY AUTOINCREMENT, at TEXT NOT NULL,
  sf_user_id TEXT, sf_username TEXT, action TEXT NOT NULL,
  object TEXT, detail TEXT);
";

pub fn open(path: &Path, key_hex: &str) -> Result<Store> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).context("create app data dir")?;
    }
    let conn = Connection::open(path).context("open db")?;
    conn.pragma_update(None, "key", format!("x'{key_hex}'"))
        .context("apply key")?;
    conn.pragma_update(None, "cipher_memory_security", 1)?;
    // Touching the schema is what actually verifies the key.
    conn.execute_batch(SCHEMA)
        .map_err(|e| anyhow!("database could not be opened (wrong key or corrupt file): {e}"))?;
    Ok(Store { conn })
}

impl Store {
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    const OTHER: &str = "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100";

    #[test]
    fn ident_accepts_api_names_and_rejects_everything_else() {
        assert_eq!(ident("Account").unwrap(), "\"Account\"");
        assert_eq!(ident("npsp__Household__c").unwrap(), "\"npsp__Household__c\"");
        for bad in ["", "Acc ount", "x\"y", "a;b", "a-b", "Ω"] {
            assert!(ident(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn open_write_reopen_same_key_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mirror.db");
        {
            let s = open(&path, KEY).unwrap();
            s.conn().execute("INSERT INTO _meta(key, value) VALUES('a','1')", []).unwrap();
        }
        let s = open(&path, KEY).unwrap();
        let v: String = s.conn().query_row("SELECT value FROM _meta WHERE key='a'", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "1");
    }

    #[test]
    fn open_with_wrong_key_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mirror.db");
        open(&path, KEY).unwrap();
        assert!(open(&path, OTHER).is_err());
    }

    #[test]
    fn file_on_disk_is_not_plaintext_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mirror.db");
        open(&path, KEY).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3"), "header must be encrypted");
    }
}
```

Add `pub mod store;` at the top of `src-tauri/src/lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test store:: 2>&1 | tail -20`
Expected: 4 passed. (These are written together with the implementation because the module is a thin wrapper; the key assertion is `file_on_disk_is_not_plaintext_sqlite` — if it fails, SQLCipher is not actually active and the build probe must be revisited.)

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add -A && git commit -m "feat(store): SQLCipher-encrypted store with schema and identifier validation"
```

---

### Task 3: Config and keychain secrets

**Files:**
- Create: `src-tauri/src/config.rs`, `src-tauri/src/secrets.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod config; pub mod secrets;`)

**Interfaces:**
- Produces: `Config::from_env() -> Result<Config>` with `client_id: String`, `login_url: String` (no trailing slash). `Secrets::new(service: &str)`, `Secrets::default_service()`, `get(&self, name) -> Result<Option<String>>`, `set(&self, name, value) -> Result<()>`, `delete(&self, name) -> Result<()>`, `db_key(&self) -> Result<String>` (64 hex chars, generated once). Constants `secrets::TOKENS = "salesforce_tokens"`, `secrets::DB_KEY = "db_key"`.

- [ ] **Step 1: Write failing tests for config**

Create `src-tauri/src/config.rs`:

```rust
//! Runtime configuration. The consumer key is a public client id; it is still
//! kept out of git via .env. Loaded from the process env, falling back to a
//! .env file in the cwd or its parent (tauri dev runs from src-tauri/).

use anyhow::{anyhow, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub client_id: String,
    pub login_url: String,
}

impl Config {
    pub fn from_env() -> Result<Config> {
        let _ = dotenvy::from_filename(".env").or_else(|_| dotenvy::from_filename("../.env"));
        let client_id = std::env::var("SF_CLIENT_ID").ok().filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow!("SF_CLIENT_ID is not set (see .env.example)"))?;
        let login_url = std::env::var("SF_LOGIN_URL")
            .unwrap_or_else(|_| "https://login.salesforce.com".to_string());
        Ok(Config::new(client_id, login_url))
    }

    pub fn new(client_id: impl Into<String>, login_url: impl Into<String>) -> Config {
        let login_url = login_url.into().trim().trim_end_matches('/').to_string();
        Config { client_id: client_id.into(), login_url }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_strips_trailing_slash_and_whitespace() {
        let c = Config::new("id", " https://x.my.salesforce.com/ ");
        assert_eq!(c.login_url, "https://x.my.salesforce.com");
        assert_eq!(c.client_id, "id");
    }
}
```

- [ ] **Step 2: Write secrets with tests**

Create `src-tauri/src/secrets.rs`:

```rust
//! Windows Credential Manager access. The webview never sees any of this.

use anyhow::{Context, Result};
use keyring::v1::{Entry, Error as KeyringError};

pub const TOKENS: &str = "salesforce_tokens";
pub const DB_KEY: &str = "db_key";
const SERVICE: &str = "emanuel-customer-intelligence";

#[derive(Clone, Debug)]
pub struct Secrets {
    service: String,
}

impl Secrets {
    pub fn default_service() -> Secrets {
        Secrets::new(SERVICE)
    }
    pub fn new(service: &str) -> Secrets {
        Secrets { service: service.to_string() }
    }

    fn entry(&self, name: &str) -> Result<Entry> {
        Entry::new(&self.service, name).context("keychain entry")
    }

    pub fn get(&self, name: &str) -> Result<Option<String>> {
        match self.entry(name)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("keychain read failed: {e}")),
        }
    }

    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        self.entry(name)?.set_password(value).context("keychain write")
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("keychain delete failed: {e}")),
        }
    }

    /// The SQLCipher key: 32 random bytes, hex, generated once and kept in the keychain.
    pub fn db_key(&self) -> Result<String> {
        if let Some(k) = self.get(DB_KEY)? {
            return Ok(k);
        }
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).context("os random")?;
        let k = hex::encode(bytes);
        self.set(DB_KEY, &k)?;
        Ok(k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_secrets() -> Secrets {
        Secrets::new("emanuel-customer-intelligence-test")
    }

    #[test]
    fn roundtrip_and_delete() {
        let s = test_secrets();
        s.delete("rt").unwrap();
        assert_eq!(s.get("rt").unwrap(), None);
        s.set("rt", "{\"a\":1}").unwrap();
        assert_eq!(s.get("rt").unwrap().as_deref(), Some("{\"a\":1}"));
        s.delete("rt").unwrap();
        assert_eq!(s.get("rt").unwrap(), None);
    }

    #[test]
    fn db_key_is_generated_once_and_is_64_hex() {
        let s = test_secrets();
        s.delete(DB_KEY).unwrap();
        let k1 = s.db_key().unwrap();
        let k2 = s.db_key().unwrap();
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 64);
        assert!(k1.chars().all(|c| c.is_ascii_hexdigit()));
        s.delete(DB_KEY).unwrap();
    }
}
```

Add `pub mod config; pub mod secrets;` to `lib.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test config:: secrets:: 2>&1 | tail -20` (run them as `cargo test -- --test-threads=1` if the two keychain tests interfere).
Expected: 3 passed. These hit the real Credential Manager under the `-test` service name and clean up after themselves.

- [ ] **Step 4: Commit**

```bash
cargo fmt
git add -A && git commit -m "feat: config from env and keychain-backed secrets"
```

---

### Task 4: OAuth — PKCE, callback parsing, login, refresh, revoke, identity

**Files:**
- Create: `src-tauri/src/auth.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod auth;`)

**Interfaces:**
- Consumes: `config::Config`, `secrets::{Secrets, TOKENS}`.
- Produces: `TokenSet { access_token, refresh_token: Option<String>, instance_url, id: String }` (Serialize/Deserialize); `Identity { user_id, organization_id, username, display_name }` (Serialize/Clone); `pkce_challenge(verifier: &str) -> String`; `authorize_url(cfg, challenge, state) -> String`; `parse_callback(path_and_query, expected_state) -> Result<Option<String>, AuthError>` (Ok(None) = not the callback path); `async login(cfg, secrets) -> Result<(TokenSet, Identity)>`; `async refresh(cfg, secrets, &TokenSet) -> Result<TokenSet>`; `async revoke(cfg, &TokenSet)`; `async fetch_identity(&TokenSet) -> Result<Identity>`; `load_tokens(secrets) -> Result<Option<TokenSet>>`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/auth.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_appendix_b_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(pkce_challenge(verifier), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn authorize_url_contains_required_params() {
        let cfg = crate::config::Config::new("CID", "https://x.my.salesforce.com");
        let u = authorize_url(&cfg, "CHAL", "STATE");
        assert!(u.starts_with("https://x.my.salesforce.com/services/oauth2/authorize?"));
        for needle in [
            "response_type=code", "client_id=CID", "code_challenge=CHAL",
            "code_challenge_method=S256", "state=STATE",
            "redirect_uri=http%3A%2F%2Flocalhost%3A1717%2Fcallback",
            "scope=api+refresh_token+openid+id+profile+email",
        ] {
            assert!(u.contains(needle), "missing {needle} in {u}");
        }
    }

    #[test]
    fn parse_callback_returns_code_when_state_matches() {
        let r = parse_callback("/callback?code=abc.def&state=S1", "S1").unwrap();
        assert_eq!(r.as_deref(), Some("abc.def"));
    }

    #[test]
    fn parse_callback_rejects_wrong_or_missing_state() {
        assert!(matches!(parse_callback("/callback?code=x&state=BAD", "S1"), Err(AuthError::StateMismatch)));
        assert!(matches!(parse_callback("/callback?code=x", "S1"), Err(AuthError::StateMismatch)));
    }

    #[test]
    fn parse_callback_reports_provider_error_and_missing_code() {
        assert!(matches!(parse_callback("/callback?error=access_denied&state=S1", "S1"), Err(AuthError::Provider(_))));
        assert!(matches!(parse_callback("/callback?state=S1", "S1"), Err(AuthError::MissingCode)));
    }

    #[test]
    fn parse_callback_ignores_other_paths() {
        assert_eq!(parse_callback("/favicon.ico", "S1").unwrap(), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test auth:: 2>&1 | tail -5`
Expected: compile error — `pkce_challenge`, `authorize_url`, `parse_callback`, `AuthError` not found.

- [ ] **Step 3: Implement**

Prepend to `src-tauri/src/auth.rs` (above the test module):

```rust
//! Salesforce OAuth 2.0 authorization-code + PKCE for a public desktop client.
//! Tokens live only here and in the keychain — never in the webview.

use crate::config::Config;
use crate::secrets::{Secrets, TOKENS};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub const REDIRECT: &str = "http://localhost:1717/callback";
const LISTEN: &str = "127.0.0.1:1717";
const SCOPES: &str = "api refresh_token openid id profile email";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TokenSet {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub instance_url: String,
    /// Identity URL, e.g. https://login.salesforce.com/id/00D.../005...
    pub id: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Identity {
    pub user_id: String,
    pub organization_id: String,
    pub username: String,
    pub display_name: String,
}

#[derive(Debug)]
pub enum AuthError {
    StateMismatch,
    MissingCode,
    Provider(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::StateMismatch => write!(f, "login response did not match this app's request (state mismatch)"),
            AuthError::MissingCode => write!(f, "login response had no authorization code"),
            AuthError::Provider(e) => write!(f, "Salesforce declined the login: {e}"),
        }
    }
}
impl std::error::Error for AuthError {}

// ── pure helpers (unit-tested) ──────────────────────────────────────────────

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn random_b64(n: usize) -> Result<String> {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).context("os random")?;
    Ok(URL_SAFE_NO_PAD.encode(b))
}

pub fn authorize_url(cfg: &Config, challenge: &str, state: &str) -> String {
    let mut u = url::Url::parse(&format!("{}/services/oauth2/authorize", cfg.login_url)).expect("static url");
    u.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.client_id)
        .append_pair("redirect_uri", REDIRECT)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("scope", SCOPES);
    u.to_string()
}

/// Parse the loopback request. Ok(None) means "not the callback path" (e.g. /favicon.ico).
pub fn parse_callback(path_and_query: &str, expected_state: &str) -> Result<Option<String>, AuthError> {
    let u = url::Url::parse(&format!("http://localhost{path_and_query}"))
        .map_err(|_| AuthError::MissingCode)?;
    if u.path() != "/callback" {
        return Ok(None);
    }
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for (k, v) in u.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }
    if state.as_deref() != Some(expected_state) {
        return Err(AuthError::StateMismatch);
    }
    if let Some(e) = error {
        return Err(AuthError::Provider(e));
    }
    code.map(Some).ok_or(AuthError::MissingCode)
}

// ── token persistence ───────────────────────────────────────────────────────

pub fn load_tokens(secrets: &Secrets) -> Result<Option<TokenSet>> {
    Ok(match secrets.get(TOKENS)? {
        Some(json) => Some(serde_json::from_str(&json).context("stored tokens unreadable")?),
        None => None,
    })
}

pub fn save_tokens(secrets: &Secrets, t: &TokenSet) -> Result<()> {
    secrets.set(TOKENS, &serde_json::to_string(t)?)
}

// ── network flows ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    instance_url: String,
    id: String,
}

/// Full login: bind loopback FIRST, open browser, wait for one callback, exchange code.
pub async fn login(cfg: &Config, secrets: &Secrets) -> Result<(TokenSet, Identity)> {
    let verifier = random_b64(64)?;
    let challenge = pkce_challenge(&verifier);
    let state = random_b64(32)?;

    let server = tiny_http::Server::http(LISTEN)
        .map_err(|e| anyhow!("could not listen on {LISTEN} (is another copy of the app running?): {e}"))?;

    let auth_url = authorize_url(cfg, &challenge, &state);
    tauri_plugin_opener::open_url(&auth_url, None::<&str>).context("open browser")?;

    let code = tokio::task::spawn_blocking(move || wait_for_code(server, &state))
        .await
        .context("listener task")??;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/services/oauth2/token", cfg.login_url))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("client_id", cfg.client_id.as_str()),
            ("redirect_uri", REDIRECT),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .await
        .context("token request")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("token exchange failed ({status}): {body}"));
    }
    let tr: TokenResponse = serde_json::from_str(&body).context("token response")?;
    let tokens = TokenSet {
        access_token: tr.access_token,
        refresh_token: tr.refresh_token,
        instance_url: tr.instance_url,
        id: tr.id,
    };
    let identity = fetch_identity(&tokens).await?;
    save_tokens(secrets, &tokens)?;
    Ok((tokens, identity))
}

fn wait_for_code(server: tiny_http::Server, state: &str) -> Result<String> {
    let deadline = std::time::Instant::now() + LOGIN_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(anyhow!("timed out waiting for the browser login (5 minutes)"));
        }
        let Some(req) = server.recv_timeout(remaining).context("listener")? else {
            return Err(anyhow!("timed out waiting for the browser login (5 minutes)"));
        };
        match parse_callback(req.url(), state) {
            Ok(None) => {
                let _ = req.respond(tiny_http::Response::empty(404));
            }
            Ok(Some(code)) => {
                let _ = req.respond(html(200, "Connected. You can close this tab and return to the app."));
                return Ok(code);
            }
            Err(e) => {
                let _ = req.respond(html(400, "Login was not accepted. Return to the app and try again."));
                return Err(e.into());
            }
        }
    }
}

fn html(code: u16, body: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let page = format!(
        "<!doctype html><meta charset=utf-8><title>Emanuel Customer Intelligence</title>\
         <body style=\"font:16px system-ui;padding:48px;color:#1c1917\">{body}</body>"
    );
    tiny_http::Response::from_string(page)
        .with_status_code(code)
        .with_header(tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap())
}

pub async fn refresh(cfg: &Config, secrets: &Secrets, current: &TokenSet) -> Result<TokenSet> {
    let rt = current.refresh_token.as_deref().ok_or_else(|| anyhow!("no refresh token; please reconnect"))?;
    let resp = reqwest::Client::new()
        .post(format!("{}/services/oauth2/token", cfg.login_url))
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", rt),
            ("client_id", cfg.client_id.as_str()),
        ])
        .send()
        .await
        .context("refresh request")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("session refresh failed ({status}); please reconnect"));
    }
    let tr: TokenResponse = serde_json::from_str(&body).context("refresh response")?;
    let tokens = TokenSet {
        access_token: tr.access_token,
        refresh_token: tr.refresh_token.or_else(|| current.refresh_token.clone()),
        instance_url: tr.instance_url,
        id: tr.id,
    };
    save_tokens(secrets, &tokens)?;
    Ok(tokens)
}

/// Best-effort revoke at Salesforce; the caller clears the keychain regardless.
pub async fn revoke(cfg: &Config, tokens: &TokenSet) {
    let token = tokens.refresh_token.clone().unwrap_or_else(|| tokens.access_token.clone());
    let _ = reqwest::Client::new()
        .post(format!("{}/services/oauth2/revoke", cfg.login_url))
        .form(&[("token", token.as_str())])
        .send()
        .await;
}

pub async fn fetch_identity(tokens: &TokenSet) -> Result<Identity> {
    let resp = reqwest::Client::new()
        .get(&tokens.id)
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .context("identity request")?
        .error_for_status()
        .context("identity response")?;
    resp.json::<Identity>().await.context("identity json")
}
```

Add `pub mod auth;` to `lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test auth:: 2>&1 | tail -15`
Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add -A && git commit -m "feat(auth): PKCE + state loopback login, refresh, revoke, identity"
```

---

### Task 5: Salesforce REST client

**Files:**
- Create: `src-tauri/src/salesforce.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod salesforce;`)

**Interfaces:**
- Consumes: `auth::{TokenSet, refresh}`, `config::Config`, `secrets::Secrets`.
- Produces: `SObjectMeta { name, label, queryable, custom_setting, deprecated_and_hidden }`, `FieldMeta { name, field_type, label }`, `mirrorable(&SObjectMeta) -> bool`, `selectable(&FieldMeta) -> bool`, `SfClient::new(cfg, secrets, tokens)`, `async describe_global(&mut self) -> Result<Vec<SObjectMeta>>` (already filtered), `async describe_object(&mut self, &str) -> Result<Vec<FieldMeta>>` (already filtered), `async count(&mut self, &str) -> Result<i64>`, `async query_all(&mut self, soql: &str, on_page: &mut (dyn FnMut(usize) + Send)) -> Result<Vec<Row>>` (the `+ Send` bound is required so the async command futures stay `Send`) where `pub type Row = serde_json::Map<String, serde_json::Value>`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/salesforce.rs` with the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn obj(name: &str, queryable: bool, cs: bool, dep: bool) -> SObjectMeta {
        SObjectMeta { name: name.into(), label: name.into(), queryable, custom_setting: cs, deprecated_and_hidden: dep }
    }

    #[test]
    fn mirrorable_requires_queryable_and_excludes_settings_and_deprecated() {
        assert!(mirrorable(&obj("Account", true, false, false)));
        assert!(!mirrorable(&obj("Setting__c", true, true, false)));
        assert!(!mirrorable(&obj("Old__c", true, false, true)));
        assert!(!mirrorable(&obj("Feed", false, false, false)));
    }

    #[test]
    fn selectable_skips_compound_and_binary_fields() {
        let f = |t: &str| FieldMeta { name: "x".into(), field_type: t.into(), label: "x".into() };
        assert!(selectable(&f("string")));
        assert!(selectable(&f("textarea")));
        assert!(!selectable(&f("address")));
        assert!(!selectable(&f("location")));
        assert!(!selectable(&f("base64")));
    }

    #[test]
    fn describe_global_json_deserializes_with_defaults() {
        let g: GlobalDescribe = serde_json::from_str(
            r#"{"sobjects":[{"name":"Account","label":"Account","queryable":true}]}"#).unwrap();
        assert_eq!(g.sobjects.len(), 1);
        assert!(!g.sobjects[0].custom_setting);
        assert!(!g.sobjects[0].deprecated_and_hidden);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test salesforce:: 2>&1 | tail -5` — Expected: compile errors (types not defined).

- [ ] **Step 3: Implement**

Prepend to `src-tauri/src/salesforce.rs`:

```rust
//! Salesforce REST client. Every call goes through `get_json`, which refreshes
//! the token once on 401. Only reads; there is deliberately no POST/PATCH here.

use crate::auth::{self, TokenSet};
use crate::config::Config;
use crate::secrets::Secrets;
use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;

pub const API_VERSION: &str = "v62.0";
pub type Row = serde_json::Map<String, serde_json::Value>;

#[derive(Deserialize, Clone, Debug)]
pub struct SObjectMeta {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub queryable: bool,
    #[serde(rename = "customSetting", default)]
    pub custom_setting: bool,
    #[serde(rename = "deprecatedAndHidden", default)]
    pub deprecated_and_hidden: bool,
}

#[derive(Deserialize)]
pub struct GlobalDescribe {
    pub sobjects: Vec<SObjectMeta>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct FieldMeta {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Deserialize)]
struct ObjectDescribe {
    fields: Vec<FieldMeta>,
}

#[derive(Deserialize)]
struct QueryPage {
    #[serde(default)]
    records: Vec<Row>,
    #[serde(rename = "nextRecordsUrl")]
    next: Option<String>,
    #[serde(default)]
    done: bool,
    #[serde(rename = "totalSize", default)]
    total_size: i64,
}

pub fn mirrorable(o: &SObjectMeta) -> bool {
    o.queryable && !o.custom_setting && !o.deprecated_and_hidden
}

pub fn selectable(f: &FieldMeta) -> bool {
    !matches!(f.field_type.as_str(), "address" | "location" | "base64")
}

pub struct SfClient {
    http: reqwest::Client,
    cfg: Config,
    secrets: Secrets,
    tokens: TokenSet,
}

impl SfClient {
    pub fn new(cfg: Config, secrets: Secrets, tokens: TokenSet) -> SfClient {
        SfClient { http: reqwest::Client::new(), cfg, secrets, tokens }
    }

    pub fn tokens(&self) -> &TokenSet {
        &self.tokens
    }

    fn api(&self, path: &str) -> String {
        format!("{}/services/data/{API_VERSION}{path}", self.tokens.instance_url)
    }

    async fn get_json<T: DeserializeOwned>(&mut self, url: &str) -> Result<T> {
        for attempt in 0..2 {
            let resp = self
                .http
                .get(url)
                .bearer_auth(&self.tokens.access_token)
                .send()
                .await
                .context("salesforce request")?;
            if resp.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.tokens = auth::refresh(&self.cfg, &self.secrets, &self.tokens).await?;
                continue;
            }
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(anyhow!("salesforce {status}: {}", body.chars().take(300).collect::<String>()));
            }
            return serde_json::from_str(&body).context("salesforce json");
        }
        Err(anyhow!("unauthorized after refresh; please reconnect"))
    }

    pub async fn describe_global(&mut self) -> Result<Vec<SObjectMeta>> {
        let g: GlobalDescribe = self.get_json(&self.api("/sobjects/")).await?;
        Ok(g.sobjects.into_iter().filter(mirrorable).collect())
    }

    pub async fn describe_object(&mut self, object: &str) -> Result<Vec<FieldMeta>> {
        let d: ObjectDescribe = self.get_json(&self.api(&format!("/sobjects/{object}/describe"))).await?;
        Ok(d.fields.into_iter().filter(selectable).collect())
    }

    pub async fn count(&mut self, object: &str) -> Result<i64> {
        let q = format!("SELECT COUNT() FROM {object}");
        let url = format!("{}?q={}", self.api("/query"), urlencoded(&q));
        let p: QueryPage = self.get_json(&url).await?;
        Ok(p.total_size)
    }

    /// Follow nextRecordsUrl until done. `on_page` receives the running row count.
    pub async fn query_all(&mut self, soql: &str, on_page: &mut (dyn FnMut(usize) + Send)) -> Result<Vec<Row>> {
        let mut url = format!("{}?q={}", self.api("/query"), urlencoded(soql));
        let mut out: Vec<Row> = Vec::new();
        loop {
            let page: QueryPage = self.get_json(&url).await?;
            out.extend(page.records);
            on_page(out.len());
            match page.next {
                Some(n) if !page.done => url = format!("{}{}", self.tokens.instance_url, n),
                _ => break,
            }
        }
        Ok(out)
    }
}

fn urlencoded(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
```

Add `pub mod salesforce;` to `lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test salesforce:: 2>&1 | tail -10` — Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add -A && git commit -m "feat(salesforce): read-only REST client with describe, count, paginated query"
```

---

### Task 6: Store — catalog, selection, mirror tables, audit, status

**Files:**
- Modify: `src-tauri/src/store.rs`

**Interfaces:**
- Consumes: `salesforce::Row`.
- Produces (all on `impl Store`):
  - `upsert_object(&self, name, label, record_count: i64)`, `upsert_field(&self, object, field, sf_type, label, sensitive: bool)` (preserve `selected`/`withheld` on conflict)
  - `set_meta(&self, key, value)`, `get_meta(&self, key) -> Result<Option<String>>`
  - `list_objects(&self) -> Result<Vec<ObjectRow>>`, `set_object_selected(&self, name, bool)`, `selected_objects(&self) -> Result<Vec<String>>`
  - `list_fields(&self, object) -> Result<Vec<FieldRow>>`, `set_field_withheld(&self, object, field, bool) -> Result<bool>` (false if field not sensitive and asked to withhold=false → no-op), `sync_columns(&self, object) -> Result<Vec<String>>`
  - `replace_mirror(&mut self, object, cols: &[String], rows: &[Row]) -> Result<usize>`, `synced_objects(&self) -> Result<Vec<String>>`, `allowed_fields(&self, object) -> Result<HashSet<String>>`
  - `audit(&self, who: &Who, action, object: Option<&str>, detail: Option<serde_json::Value>)`, `list_audit(&self, limit, offset) -> Result<Vec<AuditRow>>`
  - `status(&self) -> Result<Status>`, `purge_mirror(&mut self)`
  - Types: `ObjectRow { name, label, record_count, selected: bool, last_synced_at: Option<String>, last_sync_rows: Option<i64> }`, `FieldRow { field, sf_type, label, sensitive, withheld, fill_rate: Option<f64>, distinct_count: Option<i64>, top_values: Option<String> }`, `Who { sf_user_id: Option<String>, sf_username: Option<String> }`, `AuditRow { id, at, sf_user_id, sf_username, action, object, detail }`, `Status { object_count, selected_count, synced_rows, last_scan_at: Option<String> }`. All `Serialize`.

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `store.rs`:

```rust
    /// Returns the TempDir too so it lives as long as the Store.
    fn mem() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = open(&dir.path().join("t.db"), KEY).unwrap();
        (dir, s)
    }

    fn who() -> Who {
        Who { sf_user_id: Some("005".into()), sf_username: Some("u@x".into()) }
    }

    #[test]
    fn rescan_preserves_selection_and_withheld_overrides() {
        let (_d, s) = mem();
        s.upsert_object("Account", "Account", 10).unwrap();
        s.upsert_field("Account", "Name", "string", "Name", false).unwrap();
        s.upsert_field("Account", "Notes__c", "textarea", "Notes", true).unwrap();
        s.set_object_selected("Account", true).unwrap();
        assert!(s.set_field_withheld("Account", "Notes__c", false).unwrap());
        // second scan with changed count/label
        s.upsert_object("Account", "Account (org)", 12).unwrap();
        s.upsert_field("Account", "Notes__c", "textarea", "Notes", true).unwrap();
        let o = &s.list_objects().unwrap()[0];
        assert!(o.selected);
        assert_eq!(o.record_count, 12);
        let f = s.list_fields("Account").unwrap();
        let notes = f.iter().find(|f| f.field == "Notes__c").unwrap();
        assert!(notes.sensitive);
        assert!(!notes.withheld, "override must survive rescan");
    }

    #[test]
    fn withheld_default_follows_sensitive_and_cannot_be_set_on_non_sensitive() {
        let (_d, s) = mem();
        s.upsert_object("Contact", "Contact", 1).unwrap();
        s.upsert_field("Contact", "Email", "email", "Email", false).unwrap();
        s.upsert_field("Contact", "Medical__c", "string", "Medical", true).unwrap();
        let f = s.list_fields("Contact").unwrap();
        assert!(!f.iter().find(|x| x.field == "Email").unwrap().withheld);
        assert!(f.iter().find(|x| x.field == "Medical__c").unwrap().withheld);
        assert_eq!(s.sync_columns("Contact").unwrap(), vec!["Email".to_string()]);
        assert!(!s.set_field_withheld("Contact", "Email", true).unwrap());
    }

    #[test]
    fn replace_mirror_creates_table_and_marks_synced() {
        let (_d, mut s) = mem();
        s.upsert_object("Campaign", "Campaign", 2).unwrap();
        s.upsert_field("Campaign", "Name", "string", "Name", false).unwrap();
        s.upsert_field("Campaign", "Status", "picklist", "Status", false).unwrap();
        let mk = |n: &str, st: &str| {
            let mut m = Row::new();
            m.insert("Name".into(), serde_json::Value::String(n.into()));
            m.insert("Status".into(), serde_json::Value::String(st.into()));
            m
        };
        let cols = s.sync_columns("Campaign").unwrap();
        let n = s.replace_mirror("Campaign", &cols, &[mk("A", "Planned"), mk("B", "Done")]).unwrap();
        assert_eq!(n, 2);
        let n2 = s.replace_mirror("Campaign", &cols, &[mk("C", "Done")]).unwrap();
        assert_eq!(n2, 1);
        let cnt: i64 = s.conn().query_row("SELECT COUNT(*) FROM \"Campaign\"", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 1, "full replace, not append");
        assert_eq!(s.synced_objects().unwrap(), vec!["Campaign".to_string()]);
        let o = &s.list_objects().unwrap()[0];
        assert_eq!(o.last_sync_rows, Some(1));
        assert!(o.last_synced_at.is_some());
        assert!(s.allowed_fields("Campaign").unwrap().contains("Name"));
        assert!(s.allowed_fields("Nope").unwrap().is_empty());
    }

    #[test]
    fn audit_appends_and_lists_newest_first() {
        let (_d, s) = mem();
        s.audit(&who(), "scan.run", None, None).unwrap();
        s.audit(&who(), "sync.object", Some("Account"), Some(serde_json::json!({"rows": 3}))).unwrap();
        let rows = s.list_audit(10, 0).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].action, "sync.object");
        assert_eq!(rows[0].object.as_deref(), Some("Account"));
        assert_eq!(rows[1].action, "scan.run");
    }

    #[test]
    fn status_and_purge() {
        let (_d, mut s) = mem();
        s.upsert_object("A", "A", 1).unwrap();
        s.upsert_field("A", "Id", "id", "Id", false).unwrap();
        s.set_object_selected("A", true).unwrap();
        let mut r = Row::new();
        r.insert("Id".into(), serde_json::Value::String("1".into()));
        s.replace_mirror("A", &["Id".to_string()], &[r]).unwrap();
        s.set_meta("last_scan_at", "2026-08-25T00:00:00Z").unwrap();
        let st = s.status().unwrap();
        assert_eq!((st.object_count, st.selected_count, st.synced_rows), (1, 1, 1));
        assert_eq!(st.last_scan_at.as_deref(), Some("2026-08-25T00:00:00Z"));
        s.purge_mirror().unwrap();
        assert_eq!(s.status().unwrap().synced_rows, 0);
        assert!(s.synced_objects().unwrap().is_empty());
        assert_eq!(s.list_objects().unwrap().len(), 1, "catalog survives purge");
    }
```

Also add `use crate::salesforce::Row;` and `use std::collections::HashSet;` at the top of the tests module (or file).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test store:: 2>&1 | tail -5` — Expected: compile errors for the missing methods/types.

- [ ] **Step 3: Implement**

Add to `store.rs` (below `impl Store { conn… }`):

```rust
use crate::salesforce::Row;
use rusqlite::params;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Serialize, Debug, Clone)]
pub struct ObjectRow {
    pub name: String,
    pub label: String,
    pub record_count: i64,
    pub selected: bool,
    pub last_synced_at: Option<String>,
    pub last_sync_rows: Option<i64>,
}

#[derive(Serialize, Debug, Clone)]
pub struct FieldRow {
    pub field: String,
    pub sf_type: String,
    pub label: String,
    pub sensitive: bool,
    pub withheld: bool,
    pub fill_rate: Option<f64>,
    pub distinct_count: Option<i64>,
    pub top_values: Option<String>,
}

#[derive(Serialize, Debug, Clone, Default)]
pub struct Who {
    pub sf_user_id: Option<String>,
    pub sf_username: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct AuditRow {
    pub id: i64,
    pub at: String,
    pub sf_user_id: Option<String>,
    pub sf_username: Option<String>,
    pub action: String,
    pub object: Option<String>,
    pub detail: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct Status {
    pub object_count: i64,
    pub selected_count: i64,
    pub synced_rows: i64,
    pub last_scan_at: Option<String>,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Store {
    // ── meta ────────────────────────────────────────────────────────────
    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO _meta(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM _meta WHERE key = ?1", params![key], |r| r.get(0))
            .optional()?)
    }

    // ── catalog (scan writes; user decisions preserved) ─────────────────
    pub fn upsert_object(&self, name: &str, label: &str, record_count: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO _objects(name, label, record_count) VALUES(?1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET label = excluded.label, record_count = excluded.record_count",
            params![name, label, record_count],
        )?;
        Ok(())
    }

    pub fn upsert_field(&self, object: &str, field: &str, sf_type: &str, label: &str, sensitive: bool) -> Result<()> {
        self.conn.execute(
            "INSERT INTO _fields(object, field, sf_type, label, sensitive, withheld)
             VALUES(?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(object, field) DO UPDATE SET
               sf_type = excluded.sf_type, label = excluded.label, sensitive = excluded.sensitive",
            params![object, field, sf_type, label, sensitive as i64],
        )?;
        Ok(())
    }

    pub fn list_objects(&self) -> Result<Vec<ObjectRow>> {
        let mut st = self.conn.prepare(
            "SELECT name, label, record_count, selected, last_synced_at, last_sync_rows FROM _objects ORDER BY name",
        )?;
        let rows = st.query_map([], |r| {
            Ok(ObjectRow {
                name: r.get(0)?,
                label: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                record_count: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                selected: r.get::<_, i64>(3)? != 0,
                last_synced_at: r.get(4)?,
                last_sync_rows: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn set_object_selected(&self, name: &str, selected: bool) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE _objects SET selected = ?2 WHERE name = ?1",
            params![name, selected as i64],
        )?;
        if n == 0 {
            return Err(anyhow!("unknown object: {name}"));
        }
        Ok(())
    }

    pub fn selected_objects(&self) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT name FROM _objects WHERE selected = 1 ORDER BY name")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn list_fields(&self, object: &str) -> Result<Vec<FieldRow>> {
        let mut st = self.conn.prepare(
            "SELECT f.field, f.sf_type, f.label, f.sensitive, f.withheld,
                    p.fill_rate, p.distinct_count, p.top_values
             FROM _fields f LEFT JOIN _profile p ON p.object = f.object AND p.field = f.field
             WHERE f.object = ?1
             ORDER BY COALESCE(p.fill_rate, -1) DESC, f.field",
        )?;
        let rows = st.query_map(params![object], |r| {
            Ok(FieldRow {
                field: r.get(0)?,
                sf_type: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                label: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                sensitive: r.get::<_, i64>(3)? != 0,
                withheld: r.get::<_, i64>(4)? != 0,
                fill_rate: r.get(5)?,
                distinct_count: r.get(6)?,
                top_values: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Returns true if a change was made. Withholding can only be toggled on
    /// sensitive fields; non-sensitive fields are always mirrored.
    pub fn set_field_withheld(&self, object: &str, field: &str, withheld: bool) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE _fields SET withheld = ?3 WHERE object = ?1 AND field = ?2 AND sensitive = 1",
            params![object, field, withheld as i64],
        )?;
        Ok(n > 0)
    }

    pub fn sync_columns(&self, object: &str) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT field FROM _fields WHERE object = ?1 AND withheld = 0 ORDER BY field")?;
        let rows = st.query_map(params![object], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ── mirror ──────────────────────────────────────────────────────────
    pub fn replace_mirror(&mut self, object: &str, cols: &[String], rows: &[Row]) -> Result<usize> {
        let tbl = ident(object)?;
        let qcols = cols.iter().map(|c| ident(c)).collect::<Result<Vec<_>>>()?;
        if qcols.is_empty() {
            return Err(anyhow!("{object}: no fields to mirror"));
        }
        let tx = self.conn.transaction()?;
        tx.execute_batch(&format!("DROP TABLE IF EXISTS {tbl}"))?;
        tx.execute_batch(&format!(
            "CREATE TABLE {tbl} ({})",
            qcols.iter().map(|c| format!("{c} TEXT")).collect::<Vec<_>>().join(", ")
        ))?;
        let placeholders = vec!["?"; qcols.len()].join(",");
        let sql = format!("INSERT INTO {tbl} ({}) VALUES ({placeholders})", qcols.join(","));
        let mut n = 0usize;
        {
            let mut st = tx.prepare(&sql)?;
            for r in rows {
                let vals: Vec<Option<String>> = cols
                    .iter()
                    .map(|c| match r.get(c) {
                        None | Some(serde_json::Value::Null) => None,
                        Some(serde_json::Value::String(s)) => Some(s.clone()),
                        Some(v) => Some(v.to_string()),
                    })
                    .collect();
                st.execute(rusqlite::params_from_iter(vals.iter()))?;
                n += 1;
            }
        }
        tx.execute(
            "UPDATE _objects SET last_synced_at = ?2, last_sync_rows = ?3 WHERE name = ?1",
            params![object, now_iso(), n as i64],
        )?;
        tx.commit()?;
        Ok(n)
    }

    pub fn synced_objects(&self) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT name FROM _objects WHERE last_synced_at IS NOT NULL ORDER BY name")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Fields a segment query may reference: object synced AND field not withheld.
    pub fn allowed_fields(&self, object: &str) -> Result<HashSet<String>> {
        let mut st = self.conn.prepare(
            "SELECT f.field FROM _fields f JOIN _objects o ON o.name = f.object
             WHERE f.object = ?1 AND f.withheld = 0 AND o.last_synced_at IS NOT NULL",
        )?;
        let rows = st.query_map(params![object], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn purge_mirror(&mut self) -> Result<()> {
        let names = self.synced_objects()?;
        let tx = self.conn.transaction()?;
        for n in names {
            tx.execute_batch(&format!("DROP TABLE IF EXISTS {}", ident(&n)?))?;
        }
        tx.execute_batch("DELETE FROM _profile; UPDATE _objects SET last_synced_at = NULL, last_sync_rows = NULL;")?;
        tx.commit()?;
        Ok(())
    }

    // ── audit (insert + read only; there is intentionally no update/delete) ──
    pub fn audit(&self, who: &Who, action: &str, object: Option<&str>, detail: Option<serde_json::Value>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO _audit(at, sf_user_id, sf_username, action, object, detail) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![now_iso(), who.sf_user_id, who.sf_username, action, object, detail.map(|d| d.to_string())],
        )?;
        Ok(())
    }

    pub fn list_audit(&self, limit: i64, offset: i64) -> Result<Vec<AuditRow>> {
        let mut st = self.conn.prepare(
            "SELECT id, at, sf_user_id, sf_username, action, object, detail FROM _audit
             ORDER BY id DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = st.query_map(params![limit, offset], |r| {
            Ok(AuditRow {
                id: r.get(0)?, at: r.get(1)?, sf_user_id: r.get(2)?, sf_username: r.get(3)?,
                action: r.get(4)?, object: r.get(5)?, detail: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ── status ──────────────────────────────────────────────────────────
    pub fn status(&self) -> Result<Status> {
        let object_count: i64 = self.conn.query_row("SELECT COUNT(*) FROM _objects", [], |r| r.get(0))?;
        let selected_count: i64 = self.conn.query_row("SELECT COUNT(*) FROM _objects WHERE selected = 1", [], |r| r.get(0))?;
        let synced_rows: i64 = self.conn.query_row("SELECT COALESCE(SUM(last_sync_rows), 0) FROM _objects", [], |r| r.get(0))?;
        Ok(Status { object_count, selected_count, synced_rows, last_scan_at: self.get_meta("last_scan_at")? })
    }
}
```

Add `use rusqlite::OptionalExtension;` to the file's imports (needed for `.optional()`).

- [ ] **Step 4: Run tests**

Run: `cargo test store:: 2>&1 | tail -15` — Expected: 9 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add -A && git commit -m "feat(store): catalog, allowlist selection, mirror replace, audit, status"
```

---

### Task 7: Profiler and sensitivity heuristic

**Files:**
- Create: `src-tauri/src/profile.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod profile;`)

**Interfaces:**
- Consumes: `store::{Store, ident}`.
- Produces: `is_sensitive(field: &str, sf_type: &str) -> bool`; `profile_object(store: &Store, object: &str) -> Result<()>`; `profile_all(store: &Store) -> Result<usize>` (objects profiled).

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/profile.rs` with the tests module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::salesforce::Row;
    use crate::store;

    #[test]
    fn sensitivity_heuristic_table() {
        let yes = [
            ("Pastoral_Notes__c", "string"), ("MedicalInfo__c", "string"), ("Description", "textarea"),
            ("Bio", "richtextarea"), ("Yahrzeit_Date__c", "date"), ("Deceased__c", "boolean"),
            ("Emergency_Contact__c", "string"), ("SSN__c", "encryptedstring"), ("Birthdate", "date"),
        ];
        let no = [("Name", "string"), ("Email", "email"), ("AnnualRevenue", "currency"), ("Status", "picklist")];
        for (f, t) in yes { assert!(is_sensitive(f, t), "{f}/{t} should be sensitive"); }
        for (f, t) in no { assert!(!is_sensitive(f, t), "{f}/{t} should NOT be sensitive"); }
    }

    #[test]
    fn profile_computes_fill_distinct_top_and_hides_sensitive_values() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store::open(&dir.path().join("p.db"), "00".repeat(32).as_str()).unwrap();
        s.upsert_object("Contact", "Contact", 3).unwrap();
        s.upsert_field("Contact", "City", "string", "City", false).unwrap();
        s.upsert_field("Contact", "Notes__c", "textarea", "Notes", true).unwrap();
        s.set_field_withheld("Contact", "Notes__c", false).unwrap(); // overridden → mirrored but values hidden
        let mk = |city: Option<&str>, notes: &str| {
            let mut m = Row::new();
            m.insert("City".into(), city.map(|c| serde_json::Value::String(c.into())).unwrap_or(serde_json::Value::Null));
            m.insert("Notes__c".into(), serde_json::Value::String(notes.into()));
            m
        };
        let cols = s.sync_columns("Contact").unwrap();
        s.replace_mirror("Contact", &cols, &[mk(Some("NYC"), "secret a"), mk(Some("NYC"), "secret b"), mk(None, "secret c")]).unwrap();
        assert_eq!(profile_all(&s).unwrap(), 1);
        let f = s.list_fields("Contact").unwrap();
        let city = f.iter().find(|x| x.field == "City").unwrap();
        assert!((city.fill_rate.unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(city.distinct_count, Some(1));
        assert_eq!(city.top_values.as_deref(), Some("NYC (2)"));
        let notes = f.iter().find(|x| x.field == "Notes__c").unwrap();
        assert_eq!(notes.top_values.as_deref(), Some("[hidden: sensitive]"));
        assert_eq!(notes.distinct_count, Some(3));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test profile:: 2>&1 | tail -5` — Expected: compile error.

- [ ] **Step 3: Implement**

Prepend to `profile.rs`:

```rust
//! Column profiler: which fields actually carry signal. Sensitive columns never
//! have their values materialised into _profile, even when a user overrode
//! withholding and mirrored them.

use crate::store::{ident, Store};
use anyhow::Result;
use rusqlite::params;

const NAME_HITS: &[&str] = &[
    "note", "medical", "health", "private", "confidential", "ssn", "dob", "birth", "diagnos",
    "pastoral", "counsel", "disab", "allerg", "emergency", "death", "deceased", "yahrzeit",
    "bereave", "hospital", "illness",
];

pub fn is_sensitive(field: &str, sf_type: &str) -> bool {
    let f = field.to_ascii_lowercase();
    NAME_HITS.iter().any(|k| f.contains(k)) || matches!(sf_type, "textarea" | "richtextarea" | "encryptedstring")
}

pub fn profile_object(store: &Store, object: &str) -> Result<()> {
    let conn = store.conn();
    let tbl = ident(object)?;
    let row_count: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {tbl}"), [], |r| r.get(0))?;
    for f in store.list_fields(object)? {
        if f.withheld {
            continue; // not on disk; nothing to profile
        }
        let col = ident(&f.field)?;
        let non_null: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {tbl} WHERE {col} IS NOT NULL AND {col} <> ''"), [], |r| r.get(0))?;
        let distinct: i64 = conn.query_row(
            &format!("SELECT COUNT(DISTINCT {col}) FROM {tbl} WHERE {col} IS NOT NULL AND {col} <> ''"), [], |r| r.get(0))?;
        let top_values = if f.sensitive {
            "[hidden: sensitive]".to_string()
        } else {
            let mut st = conn.prepare(&format!(
                "SELECT {col}, COUNT(*) c FROM {tbl} WHERE {col} IS NOT NULL AND {col} <> ''
                 GROUP BY {col} ORDER BY c DESC, {col} LIMIT 5"))?;
            let pairs = st
                .query_map([], |r| Ok(format!("{} ({})", r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            pairs.join(" | ")
        };
        let fill = if row_count > 0 { non_null as f64 / row_count as f64 } else { 0.0 };
        conn.execute(
            "INSERT INTO _profile(object, field, row_count, non_null, fill_rate, distinct_count, top_values, sensitive)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(object, field) DO UPDATE SET row_count = excluded.row_count, non_null = excluded.non_null,
               fill_rate = excluded.fill_rate, distinct_count = excluded.distinct_count,
               top_values = excluded.top_values, sensitive = excluded.sensitive",
            params![object, f.field, row_count, non_null, fill, distinct, top_values, f.sensitive as i64],
        )?;
    }
    Ok(())
}

pub fn profile_all(store: &Store) -> Result<usize> {
    let objects = store.synced_objects()?;
    for o in &objects {
        profile_object(store, o)?;
    }
    Ok(objects.len())
}
```

Add `pub mod profile;` to `lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test profile:: 2>&1 | tail -10` — Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add -A && git commit -m "feat(profile): column profiler with conservative sensitivity heuristic"
```

---

### Task 8: Segment query builder

**Files:**
- Create: `src-tauri/src/segment.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod segment;`)

**Interfaces:**
- Consumes: `store::{ident, Store}`.
- Produces: `Filter { field, op, value }` (Deserialize), `SegmentReq { object, filters: Vec<Filter>, group_by: Option<String> }` (Deserialize), `SegmentResult { count: i64, breakdown: Vec<(String, i64)> }` (Serialize), `Built { count_sql, breakdown_sql: Option<String>, binds: Vec<String> }`, `build(req: &SegmentReq, allowed: &HashSet<String>) -> Result<Built, String>`, `run(store: &Store, req: &SegmentReq) -> anyhow::Result<SegmentResult>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn allowed() -> HashSet<String> {
        ["Name", "Status", "Amount"].iter().map(|s| s.to_string()).collect()
    }
    fn req(filters: Vec<(&str, &str, &str)>, group_by: Option<&str>) -> SegmentReq {
        SegmentReq {
            object: "Campaign".into(),
            filters: filters.into_iter().map(|(f, o, v)| Filter { field: f.into(), op: o.into(), value: v.into() }).collect(),
            group_by: group_by.map(String::from),
        }
    }

    #[test]
    fn builds_bound_sql_with_and_filters_and_group_by() {
        let b = build(&req(vec![("Status", "=", "Done"), ("Name", "contains", "Gala")], Some("Status")), &allowed()).unwrap();
        assert_eq!(b.count_sql, "SELECT COUNT(*) FROM \"Campaign\" WHERE \"Status\" = ? AND \"Name\" LIKE ?");
        assert_eq!(b.binds, vec!["Done".to_string(), "%Gala%".to_string()]);
        assert_eq!(b.breakdown_sql.as_deref(), Some(
            "SELECT \"Status\", COUNT(*) c FROM \"Campaign\" WHERE \"Status\" = ? AND \"Name\" LIKE ? GROUP BY \"Status\" ORDER BY c DESC LIMIT 20"));
    }

    #[test]
    fn no_filters_means_no_where() {
        let b = build(&req(vec![], None), &allowed()).unwrap();
        assert_eq!(b.count_sql, "SELECT COUNT(*) FROM \"Campaign\"");
        assert!(b.breakdown_sql.is_none());
        assert!(b.binds.is_empty());
    }

    #[test]
    fn rejects_unknown_or_withheld_field_bad_op_and_bad_identifiers() {
        assert!(build(&req(vec![("Notes__c", "=", "x")], None), &allowed()).unwrap_err().contains("Notes__c"));
        assert!(build(&req(vec![("Name", "LIKE", "x")], None), &allowed()).unwrap_err().contains("LIKE"));
        assert!(build(&req(vec![], Some("Nope")), &allowed()).unwrap_err().contains("Nope"));
        let mut r = req(vec![], None);
        r.object = "Campaign\"; DROP TABLE _audit; --".into();
        assert!(build(&r, &allowed()).is_err());
        let mut evil = allowed();
        evil.insert("Name\" OR 1=1 --".into());
        assert!(build(&req(vec![("Name\" OR 1=1 --", "=", "x")], None), &evil).is_err());
    }

    #[test]
    fn values_are_never_interpolated() {
        let b = build(&req(vec![("Name", "=", "'; DROP TABLE _audit; --")], None), &allowed()).unwrap();
        assert!(!b.count_sql.contains("DROP"));
        assert_eq!(b.binds[0], "'; DROP TABLE _audit; --");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test segment:: 2>&1 | tail -5` — Expected: compile error.

- [ ] **Step 3: Implement**

Prepend to `segment.rs`:

```rust
//! Segment queries over the mirror. `build` is pure and fully unit-tested: it
//! is the injection and governance guard. Fields must be in `allowed` (synced
//! object, not withheld); ops come from an allowlist; values are always bound.

use crate::store::{ident, Store};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Deserialize, Debug, Clone)]
pub struct Filter {
    pub field: String,
    pub op: String,
    pub value: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SegmentReq {
    pub object: String,
    pub filters: Vec<Filter>,
    pub group_by: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct SegmentResult {
    pub count: i64,
    pub breakdown: Vec<(String, i64)>,
}

#[derive(Debug)]
pub struct Built {
    pub count_sql: String,
    pub breakdown_sql: Option<String>,
    pub binds: Vec<String>,
}

const ALLOWED_OPS: &[&str] = &["=", "!=", ">", "<", ">=", "<=", "contains"];

fn column(field: &str, allowed: &HashSet<String>) -> std::result::Result<String, String> {
    if !allowed.contains(field) {
        return Err(format!("field not available for segmenting: {field}"));
    }
    ident(field).map_err(|e| e.to_string())
}

pub fn build(req: &SegmentReq, allowed: &HashSet<String>) -> std::result::Result<Built, String> {
    let tbl = ident(&req.object).map_err(|e| e.to_string())?;
    let mut clauses = Vec::new();
    let mut binds = Vec::new();
    for f in &req.filters {
        let col = column(&f.field, allowed)?;
        if !ALLOWED_OPS.contains(&f.op.as_str()) {
            return Err(format!("operator not allowed: {}", f.op));
        }
        if f.op == "contains" {
            clauses.push(format!("{col} LIKE ?"));
            binds.push(format!("%{}%", f.value));
        } else {
            clauses.push(format!("{col} {} ?", f.op));
            binds.push(f.value.clone());
        }
    }
    let where_sql = if clauses.is_empty() { String::new() } else { format!(" WHERE {}", clauses.join(" AND ")) };
    let count_sql = format!("SELECT COUNT(*) FROM {tbl}{where_sql}");
    let breakdown_sql = match &req.group_by {
        Some(g) if !g.is_empty() => {
            let gcol = column(g, allowed)?;
            Some(format!("SELECT {gcol}, COUNT(*) c FROM {tbl}{where_sql} GROUP BY {gcol} ORDER BY c DESC LIMIT 20"))
        }
        _ => None,
    };
    Ok(Built { count_sql, breakdown_sql, binds })
}

pub fn run(store: &Store, req: &SegmentReq) -> Result<SegmentResult> {
    let allowed = store.allowed_fields(&req.object)?;
    if allowed.is_empty() {
        anyhow::bail!("object is not synced: {}", req.object);
    }
    let b = build(req, &allowed).map_err(anyhow::Error::msg)?;
    let conn = store.conn();
    let count: i64 = conn.query_row(&b.count_sql, rusqlite::params_from_iter(b.binds.iter()), |r| r.get(0))?;
    let mut breakdown = Vec::new();
    if let Some(sql) = &b.breakdown_sql {
        let mut st = conn.prepare(sql)?;
        let rows = st.query_map(rusqlite::params_from_iter(b.binds.iter()), |r| {
            Ok((r.get::<_, Option<String>>(0)?.unwrap_or_default(), r.get::<_, i64>(1)?))
        })?;
        breakdown = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    }
    Ok(SegmentResult { count, breakdown })
}
```

Add `pub mod segment;` to `lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test segment:: 2>&1 | tail -10` — Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add -A && git commit -m "feat(segment): validated, parameterised segment SQL builder"
```

---

### Task 9: Commands and app wiring

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs` (replace `run()`)

**Interfaces:**
- Consumes: everything above.
- Produces the invoke surface (names are the JS `invoke` strings):
  `get_status`, `connect`, `disconnect`, `scan`, `list_objects`, `set_object_selected {object, selected}`, `list_fields {object}`, `set_field_withheld {object, field, withheld}`, `sync_selected`, `profile_selected`, `query_segment {req}`, `get_audit {limit, offset}`, `purge_local_data`. Events: `scan:progress {done, total}`, `sync:progress {object, rows}`.
  `StatusView { connected: bool, identity: Option<Identity>, object_count, selected_count, synced_rows, last_scan_at }`, `ScanSummary { objects: usize, failed: Vec<String> }`, `SyncSummary { objects_synced: usize, rows: usize, failed: Vec<String> }`.

- [ ] **Step 1: Write commands.rs**

```rust
//! The command boundary — the only surface the webview can reach.
//! Every command: (1) never returns a token, (2) audits itself, (3) never holds
//! the store lock across an await.

use crate::auth::{self, Identity};
use crate::config::Config;
use crate::salesforce::SfClient;
use crate::secrets::{Secrets, TOKENS};
use crate::segment::{self, SegmentReq, SegmentResult};
use crate::store::{self, AuditRow, FieldRow, ObjectRow, Store, Who};
use crate::profile;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

pub struct AppState {
    pub cfg: Config,
    pub secrets: Secrets,
    pub db_path: PathBuf,
    pub store: Mutex<Option<Store>>,
    pub identity: Mutex<Option<Identity>>,
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    let s = e.to_string();
    tracing::warn!("{s}");
    s
}

/// Run `f` with the store, opening it lazily. Never call this while awaiting.
fn with_store<T>(state: &AppState, f: impl FnOnce(&mut Store) -> anyhow::Result<T>) -> CmdResult<T> {
    let mut guard = state.store.lock().map_err(|_| "store lock poisoned".to_string())?;
    if guard.is_none() {
        let key = state.secrets.db_key().map_err(err)?;
        *guard = Some(store::open(&state.db_path, &key).map_err(err)?);
    }
    f(guard.as_mut().expect("opened")).map_err(err)
}

fn who(state: &AppState) -> Who {
    match state.identity.lock().ok().and_then(|g| g.clone()) {
        Some(id) => Who { sf_user_id: Some(id.user_id), sf_username: Some(id.username) },
        None => Who::default(),
    }
}

async fn client(state: &AppState) -> CmdResult<SfClient> {
    let tokens = auth::load_tokens(&state.secrets).map_err(err)?.ok_or("Not connected to Salesforce")?;
    Ok(SfClient::new(state.cfg.clone(), state.secrets.clone(), tokens))
}

// ── status / auth ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct StatusView {
    pub connected: bool,
    pub identity: Option<Identity>,
    pub object_count: i64,
    pub selected_count: i64,
    pub synced_rows: i64,
    pub last_scan_at: Option<String>,
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> CmdResult<StatusView> {
    let tokens = auth::load_tokens(&state.secrets).map_err(err)?;
    let connected = tokens.is_some();
    if connected && state.identity.lock().map(|g| g.is_none()).unwrap_or(true) {
        // App restarted with a stored session: recover who we are (refreshes if needed).
        let mut c = client(&state).await?;
        let id = match auth::fetch_identity(c.tokens()).await {
            Ok(id) => Some(id),
            Err(_) => {
                let t = auth::refresh(&state.cfg, &state.secrets, c.tokens()).await.map_err(err)?;
                c = SfClient::new(state.cfg.clone(), state.secrets.clone(), t);
                Some(auth::fetch_identity(c.tokens()).await.map_err(err)?)
            }
        };
        *state.identity.lock().map_err(|_| "lock".to_string())? = id;
    }
    let identity = state.identity.lock().ok().and_then(|g| g.clone());
    let st = with_store(&state, |s| s.status())?;
    Ok(StatusView {
        connected,
        identity,
        object_count: st.object_count,
        selected_count: st.selected_count,
        synced_rows: st.synced_rows,
        last_scan_at: st.last_scan_at,
    })
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>) -> CmdResult<Identity> {
    let (_tokens, identity) = auth::login(&state.cfg, &state.secrets).await.map_err(err)?;
    *state.identity.lock().map_err(|_| "lock".to_string())? = Some(identity.clone());
    let w = who(&state);
    with_store(&state, |s| s.audit(&w, "auth.connect", None, Some(serde_json::json!({"org": identity.organization_id}))))?;
    Ok(identity)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> CmdResult<()> {
    let w = who(&state);
    if let Some(t) = auth::load_tokens(&state.secrets).map_err(err)? {
        auth::revoke(&state.cfg, &t).await;
    }
    state.secrets.delete(TOKENS).map_err(err)?;
    *state.identity.lock().map_err(|_| "lock".to_string())? = None;
    with_store(&state, |s| s.audit(&w, "auth.disconnect", None, None))
}

// ── scan ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
struct ScanProgress { done: usize, total: usize }

#[derive(Serialize)]
pub struct ScanSummary { pub objects: usize, pub failed: Vec<String> }

#[tauri::command]
pub async fn scan(app: AppHandle, state: State<'_, AppState>) -> CmdResult<ScanSummary> {
    let mut c = client(&state).await?;
    let objects = c.describe_global().await.map_err(err)?;
    let total = objects.len();
    let mut failed = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        let fields = match c.describe_object(&o.name).await {
            Ok(f) => f,
            Err(e) => { failed.push(format!("{}: {e}", o.name)); continue; }
        };
        let count = c.count(&o.name).await.unwrap_or(-1);
        let (name, label) = (o.name.clone(), o.label.clone());
        with_store(&state, |s| {
            s.upsert_object(&name, &label, count)?;
            for f in &fields {
                s.upsert_field(&name, &f.name, &f.field_type, &f.label, profile::is_sensitive(&f.name, &f.field_type))?;
            }
            Ok(())
        })?;
        let _ = app.emit("scan:progress", ScanProgress { done: i + 1, total });
    }
    let w = who(&state);
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    with_store(&state, |s| {
        s.set_meta("last_scan_at", &now)?;
        s.audit(&w, "scan.run", None, Some(serde_json::json!({"objects": total, "failed": failed.len()})))
    })?;
    Ok(ScanSummary { objects: total - failed.len(), failed })
}

// ── selection ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_objects(state: State<'_, AppState>) -> CmdResult<Vec<ObjectRow>> {
    with_store(&state, |s| s.list_objects())
}

#[tauri::command]
pub async fn set_object_selected(object: String, selected: bool, state: State<'_, AppState>) -> CmdResult<()> {
    let w = who(&state);
    with_store(&state, |s| {
        s.set_object_selected(&object, selected)?;
        s.audit(&w, if selected { "object.select" } else { "object.deselect" }, Some(&object), None)
    })
}

#[tauri::command]
pub async fn list_fields(object: String, state: State<'_, AppState>) -> CmdResult<Vec<FieldRow>> {
    with_store(&state, |s| s.list_fields(&object))
}

#[tauri::command]
pub async fn set_field_withheld(object: String, field: String, withheld: bool, state: State<'_, AppState>) -> CmdResult<()> {
    let w = who(&state);
    with_store(&state, |s| {
        if s.set_field_withheld(&object, &field, withheld)? {
            s.audit(&w, if withheld { "field.rewithhold" } else { "field.override" }, Some(&object),
                Some(serde_json::json!({"field": field})))?;
        }
        Ok(())
    })
}

// ── sync / profile ──────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
struct SyncProgress { object: String, rows: usize }

#[derive(Serialize)]
pub struct SyncSummary { pub objects_synced: usize, pub rows: usize, pub failed: Vec<String> }

#[tauri::command]
pub async fn sync_selected(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SyncSummary> {
    let mut c = client(&state).await?;
    let w = who(&state);
    let selected = with_store(&state, |s| s.selected_objects())?;
    let mut summary = SyncSummary { objects_synced: 0, rows: 0, failed: Vec::new() };
    for object in selected {
        let cols = with_store(&state, |s| s.sync_columns(&object))?;
        if cols.is_empty() {
            summary.failed.push(format!("{object}: every field is withheld"));
            continue;
        }
        let soql = format!("SELECT {} FROM {object}", cols.join(","));
        let app2 = app.clone();
        let obj2 = object.clone();
        let rows = match c.query_all(&soql, &mut |n| { let _ = app2.emit("sync:progress", SyncProgress { object: obj2.clone(), rows: n }); }).await {
            Ok(r) => r,
            Err(e) => {
                let msg = e.to_string();
                with_store(&state, |s| s.audit(&w, "sync.object_failed", Some(&object), Some(serde_json::json!({"error": msg}))))?;
                summary.failed.push(format!("{object}: {e}"));
                continue;
            }
        };
        let n = with_store(&state, |s| {
            let n = s.replace_mirror(&object, &cols, &rows)?;
            s.audit(&w, "sync.object", Some(&object), Some(serde_json::json!({"rows": n, "fields": cols.len()})))?;
            Ok(n)
        })?;
        summary.objects_synced += 1;
        summary.rows += n;
    }
    Ok(summary)
}

#[tauri::command]
pub async fn profile_selected(state: State<'_, AppState>) -> CmdResult<usize> {
    let w = who(&state);
    with_store(&state, |s| {
        let n = profile::profile_all(s)?;
        s.audit(&w, "profile.run", None, Some(serde_json::json!({"objects": n})))?;
        Ok(n)
    })
}

// ── segments / audit / purge ────────────────────────────────────────────────

#[tauri::command]
pub async fn query_segment(req: SegmentReq, state: State<'_, AppState>) -> CmdResult<SegmentResult> {
    let w = who(&state);
    with_store(&state, |s| {
        let r = segment::run(s, &req)?;
        let fields: Vec<&str> = req.filters.iter().map(|f| f.field.as_str()).collect();
        s.audit(&w, "segment.query", Some(&req.object),
            Some(serde_json::json!({"fields": fields, "group_by": req.group_by, "count": r.count})))?;
        Ok(r)
    })
}

#[tauri::command]
pub async fn get_audit(limit: i64, offset: i64, state: State<'_, AppState>) -> CmdResult<Vec<AuditRow>> {
    with_store(&state, |s| s.list_audit(limit.clamp(1, 500), offset.max(0)))
}

#[tauri::command]
pub async fn purge_local_data(state: State<'_, AppState>) -> CmdResult<()> {
    let w = who(&state);
    with_store(&state, |s| {
        s.purge_mirror()?;
        s.audit(&w, "data.purge", None, None)
    })
}
```

- [ ] **Step 2: Wire lib.rs**

Replace `src-tauri/src/lib.rs` entirely:

```rust
pub mod auth;
pub mod commands;
pub mod config;
pub mod profile;
pub mod salesforce;
pub mod secrets;
pub mod segment;
pub mod store;

use commands::AppState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
    ).init();

    let cfg = config::Config::from_env().expect("configuration: set SF_CLIENT_ID in .env");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let db_path = app.path().app_data_dir()?.join("mirror.db");
            app.manage(AppState {
                cfg: cfg.clone(),
                secrets: secrets::Secrets::default_service(),
                db_path,
                store: Mutex::new(None),
                identity: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::connect,
            commands::disconnect,
            commands::scan,
            commands::list_objects,
            commands::set_object_selected,
            commands::list_fields,
            commands::set_field_withheld,
            commands::sync_selected,
            commands::profile_selected,
            commands::query_segment,
            commands::get_audit,
            commands::purge_local_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Add `tracing-subscriber = { version = "0.3", features = ["env-filter"] }` (replace the plain line) in `Cargo.toml`. Delete the template's `greet` command if it still exists anywhere.

- [ ] **Step 3: Build and run all tests**

Run: `cargo build 2>&1 | grep -E "^(error|warning: unused)" -A5 | head -60` — fix any compile errors (typical: a missing `use`, `Option<&str>` vs `&String`). Then `cargo test 2>&1 | tail -15` — Expected: all previous tests still pass (24 total).

- [ ] **Step 4: Commit**

```bash
cargo fmt
git add -A && git commit -m "feat: command surface with audited scan/select/sync/profile/segment and app wiring"
```

---

### Task 10: Frontend API layer and app shell

**Files:**
- Create: `src/api.ts`, `src/api.test.ts`, `src/App.tsx` (replace), `src/pages/OverviewPage.tsx` (placeholder replaced in Task 11)
- Modify: `package.json` (add `"test": "vitest run"`, `"typecheck": "tsc --noEmit"`)

**Interfaces:**
- Produces: every function in `api.ts` below; `App` renders `SignedOut` or `AppFrame` with `nav` keys `overview | data | segments | audit`; pages receive `{ status, refresh }` where `refresh: () => Promise<void>` re-fetches status.

- [ ] **Step 1: Write the failing test**

`src/api.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import * as api from "./api";

describe("api wrappers map 1:1 to Rust commands", () => {
  beforeEach(() => invoke.mockReset());

  it("passes exact command names and args", async () => {
    invoke.mockResolvedValue(undefined);
    await api.getStatus();
    await api.setObjectSelected("Account", true);
    await api.setFieldWithheld("Account", "Notes__c", false);
    await api.getAudit(50, 100);
    await api.querySegment({ object: "Account", filters: [{ field: "Type", op: "=", value: "Member" }] });
    expect(invoke.mock.calls).toEqual([
      ["get_status"],
      ["set_object_selected", { object: "Account", selected: true }],
      ["set_field_withheld", { object: "Account", field: "Notes__c", withheld: false }],
      ["get_audit", { limit: 50, offset: 100 }],
      ["query_segment", { req: { object: "Account", filters: [{ field: "Type", op: "=", value: "Member" }] } }],
    ]);
  });

  it("exposes only the allowlisted operators", () => {
    expect([...api.OPS]).toEqual(["=", "!=", ">", "<", ">=", "<=", "contains"]);
  });
});
```

- [ ] **Step 2: Run to verify failure**

Add to `package.json` scripts: `"test": "vitest run"`, `"typecheck": "tsc --noEmit"`. Run `npm test` — Expected: fails, `./api` not found.

- [ ] **Step 3: Implement api.ts**

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// The ONLY way the UI talks to Salesforce or the local mirror. 1:1 with Rust
// commands. No token, no SQL, no network ever crosses into this layer.

export interface Identity { user_id: string; organization_id: string; username: string; display_name: string }
export interface StatusView {
  connected: boolean; identity: Identity | null;
  object_count: number; selected_count: number; synced_rows: number; last_scan_at: string | null;
}
export interface ObjectRow {
  name: string; label: string; record_count: number; selected: boolean;
  last_synced_at: string | null; last_sync_rows: number | null;
}
export interface FieldRow {
  field: string; sf_type: string; label: string; sensitive: boolean; withheld: boolean;
  fill_rate: number | null; distinct_count: number | null; top_values: string | null;
}
export interface ScanSummary { objects: number; failed: string[] }
export interface SyncSummary { objects_synced: number; rows: number; failed: string[] }
export interface Filter { field: string; op: string; value: string }
export interface SegmentReq { object: string; filters: Filter[]; group_by?: string }
export interface SegmentResult { count: number; breakdown: [string, number][] }
export interface AuditRow {
  id: number; at: string; sf_user_id: string | null; sf_username: string | null;
  action: string; object: string | null; detail: string | null;
}

export const OPS = ["=", "!=", ">", "<", ">=", "<=", "contains"] as const;

export const getStatus = () => invoke<StatusView>("get_status");
export const connect = () => invoke<Identity>("connect");
export const disconnect = () => invoke<void>("disconnect");
export const scan = () => invoke<ScanSummary>("scan");
export const listObjects = () => invoke<ObjectRow[]>("list_objects");
export const setObjectSelected = (object: string, selected: boolean) =>
  invoke<void>("set_object_selected", { object, selected });
export const listFields = (object: string) => invoke<FieldRow[]>("list_fields", { object });
export const setFieldWithheld = (object: string, field: string, withheld: boolean) =>
  invoke<void>("set_field_withheld", { object, field, withheld });
export const syncSelected = () => invoke<SyncSummary>("sync_selected");
export const profileSelected = () => invoke<number>("profile_selected");
export const querySegment = (req: SegmentReq) => invoke<SegmentResult>("query_segment", { req });
export const getAudit = (limit: number, offset: number) => invoke<AuditRow[]>("get_audit", { limit, offset });
export const purgeLocalData = () => invoke<void>("purge_local_data");

export const onScanProgress = (cb: (p: { done: number; total: number }) => void): Promise<UnlistenFn> =>
  listen<{ done: number; total: number }>("scan:progress", (e) => cb(e.payload));
export const onSyncProgress = (cb: (p: { object: string; rows: number }) => void): Promise<UnlistenFn> =>
  listen<{ object: string; rows: number }>("sync:progress", (e) => cb(e.payload));
```

Run `npm test` — Expected: 2 passed.

- [ ] **Step 4: App shell**

Replace `src/App.tsx`:

```tsx
import { useCallback, useEffect, useState } from "react";
import "./design-system/styles.css";
import { Alert, Button } from "./design-system";
import { AppFrame } from "./design-system/ui-kits/grant-management/chrome.jsx";
import logoUrl from "./assets/emanuel_logo.png";
import * as api from "./api";
import OverviewPage from "./pages/OverviewPage";
import DataPage from "./pages/DataPage";
import SegmentsPage from "./pages/SegmentsPage";
import AuditPage from "./pages/AuditPage";

export type PageKey = "overview" | "data" | "segments" | "audit";
export interface PageProps { status: api.StatusView; refresh: () => Promise<void> }

const NAV = [
  { key: "overview", icon: "layout-dashboard", label: "Overview" },
  { key: "data", icon: "database", label: "Data" },
  { key: "segments", icon: "chart-pie", label: "Segments" },
  { key: "audit", icon: "scroll-text", label: "Audit" },
];

function initials(name: string) {
  return name.split(/\s+/).filter(Boolean).slice(0, 2).map((p) => p[0]?.toUpperCase() ?? "").join("") || "?";
}

function SignedOut({ onConnected, error }: { onConnected: () => Promise<void>; error: string | null }) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(error);
  const go = async () => {
    setBusy(true); setErr(null);
    try { await api.connect(); await onConnected(); }
    catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };
  return (
    <div style={{ minHeight: "100vh", display: "flex", alignItems: "center", justifyContent: "center",
      background: "var(--gradient-brand)", fontFamily: "var(--font-body)", padding: "var(--space-6)" }}>
      <div style={{ background: "var(--bg-primary)", borderRadius: "var(--radius-2xl)", boxShadow: "var(--shadow-2xl)",
        padding: "var(--space-10)", maxWidth: 440, width: "100%", textAlign: "center" }}>
        <img src={logoUrl} alt="Temple Emanu-El" style={{ width: 72, height: 72, marginBottom: "var(--space-4)" }} />
        <h1 style={{ margin: 0, fontFamily: "var(--font-display)", fontSize: "var(--text-2xl)", fontWeight: "var(--font-semibold)",
          letterSpacing: "var(--tracking-tight)", color: "var(--text-primary)" }}>Temple Emanu-El</h1>
        <div style={{ color: "var(--text-accent)", fontSize: "var(--text-xs)", letterSpacing: "0.18em", textTransform: "uppercase",
          fontWeight: "var(--font-medium)", marginBottom: "var(--space-6)" }}>Customer Intelligence</div>
        <p style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)", margin: "0 0 var(--space-6)" }}>
          Sign in with your Salesforce account. The login opens in your browser; this app never sees your password.
        </p>
        {err && <Alert tone="error" style={{ marginBottom: "var(--space-4)", textAlign: "left" }}>{err}</Alert>}
        <Button fullWidth disabled={busy} onClick={go}>{busy ? "Waiting for browser…" : "Connect to Salesforce"}</Button>
      </div>
    </div>
  );
}

export default function App() {
  const [status, setStatus] = useState<api.StatusView | null>(null);
  const [page, setPage] = useState<PageKey>("overview");
  const [fatal, setFatal] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try { setStatus(await api.getStatus()); setFatal(null); }
    catch (e) { setFatal(String(e)); setStatus({ connected: false, identity: null, object_count: 0, selected_count: 0, synced_rows: 0, last_scan_at: null }); }
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);

  if (!status) return null;
  if (!status.connected || !status.identity) return <SignedOut onConnected={refresh} error={fatal} />;

  const user = { initials: initials(status.identity.display_name), name: status.identity.display_name, role: "Salesforce" };
  const props: PageProps = { status, refresh };
  return (
    <AppFrame nav={NAV} active={page} onNav={(k: string) => setPage(k as PageKey)} user={user}>
      {page === "overview" && <OverviewPage {...props} />}
      {page === "data" && <DataPage {...props} />}
      {page === "segments" && <SegmentsPage {...props} />}
      {page === "audit" && <AuditPage {...props} />}
    </AppFrame>
  );
}
```

Create minimal placeholders so it compiles (each replaced in its own task):

```tsx
// src/pages/OverviewPage.tsx, DataPage.tsx, SegmentsPage.tsx, AuditPage.tsx — same body, different name
import type { PageProps } from "../App";
export default function OverviewPage(_: PageProps) { return <div>Overview</div>; }
```

- [ ] **Step 5: Verify**

Run `npm run typecheck` — Expected: no errors. Run `npm run tauri dev` → the signed-out card appears → click **Connect to Salesforce** → browser opens Salesforce login → after login the tab says "Connected…" and the app shows the header with your name and the four nav items.

If Salesforce returns `invalid_client_id`, the ECA may still be propagating (up to 30 min) or the `.env` key is wrong. If the token exchange returns `invalid_client`, "Require Secret for Web Server Flow" is still checked in the ECA.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(ui): typed API layer, signed-out screen, AppFrame shell with live identity"
```

---

### Task 11: Overview page

**Files:**
- Replace: `src/pages/OverviewPage.tsx`

**Interfaces:**
- Consumes: `PageProps`, `api.{scan, syncSelected, profileSelected, disconnect, purgeLocalData, onScanProgress, onSyncProgress}`, design-system `Card, CardHeader, CardTitle, Button, Alert, Modal`, chrome `PageTitle, Stat`.

- [ ] **Step 1: Implement**

```tsx
import { useEffect, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Button, Card, CardHeader, CardTitle, Modal } from "../design-system";
import { PageTitle, Stat } from "../design-system/ui-kits/grant-management/chrome.jsx";

type Step = "scan" | "select" | "sync" | "profile" | "ready";

function nextStep(s: api.StatusView): Step {
  if (s.object_count === 0) return "scan";
  if (s.selected_count === 0) return "select";
  if (s.synced_rows === 0) return "sync";
  return "ready";
}

const STEP_COPY: Record<Step, { title: string; body: string; button: string | null }> = {
  scan: { title: "Scan your org", body: "Reads object and field names only. No records are copied.", button: "Scan Metadata" },
  select: { title: "Choose objects to mirror", body: "Nothing is copied until you select objects on the Data page.", button: null },
  sync: { title: "Sync selected objects", body: "Copies the selected objects into the encrypted local mirror, minus withheld fields.", button: "Sync Now" },
  profile: { title: "Profile columns", body: "Compute fill rates and top values so you can see which fields carry signal.", button: "Profile" },
  ready: { title: "Data is ready", body: "Re-sync any time to refresh the mirror. Profiling runs automatically after each sync.", button: "Sync Again" },
};

export default function OverviewPage({ status, refresh }: PageProps) {
  const [busy, setBusy] = useState<string | null>(null);
  const [progress, setProgress] = useState<string>("");
  const [notice, setNotice] = useState<{ tone: "success" | "warning" | "error"; text: string } | null>(null);
  const [confirmPurge, setConfirmPurge] = useState(false);

  useEffect(() => {
    const subs = [
      api.onScanProgress((p) => setProgress(`Scanning ${p.done} of ${p.total} objects`)),
      api.onSyncProgress((p) => setProgress(`${p.object}: ${p.rows.toLocaleString()} rows`)),
    ];
    return () => { subs.forEach((s) => s.then((un) => un())); };
  }, []);

  const run = async (label: string, fn: () => Promise<string>) => {
    setBusy(label); setNotice(null); setProgress("");
    try { setNotice({ tone: "success", text: await fn() }); }
    catch (e) { setNotice({ tone: "error", text: String(e) }); }
    finally { setBusy(null); setProgress(""); await refresh(); }
  };

  const doScan = () => run("scan", async () => {
    const r = await api.scan();
    return `Scanned ${r.objects} objects.${r.failed.length ? ` ${r.failed.length} could not be described.` : ""}`;
  });
  const doSync = () => run("sync", async () => {
    const r = await api.syncSelected();
    const n = await api.profileSelected();
    return `Synced ${r.rows.toLocaleString()} rows across ${r.objects_synced} objects; profiled ${n}.${r.failed.length ? ` Failed: ${r.failed.join("; ")}` : ""}`;
  });

  const step = nextStep(status);
  const copy = STEP_COPY[step];
  const onPrimary = step === "scan" ? doScan : doSync;

  return (
    <div style={{ maxWidth: 1100 }}>
      <PageTitle eyebrow="Customer Intelligence" title="Overview" actions={
        <Button variant="secondary" disabled={busy !== null} onClick={doScan}>Rescan Metadata</Button>
      } />

      {notice && <Alert tone={notice.tone} style={{ marginBottom: "var(--space-6)" }}>{notice.text}</Alert>}

      <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: "var(--space-4)", marginBottom: "var(--space-6)" }}>
        <Stat label="Objects scanned" value={status.object_count.toLocaleString()} icon="database"
          sub={status.last_scan_at ? `Last scan ${new Date(status.last_scan_at).toLocaleString()}` : "Not scanned yet"} />
        <Stat label="Objects selected" value={status.selected_count.toLocaleString()} icon="square-check" tone="accent" />
        <Stat label="Rows mirrored" value={status.synced_rows.toLocaleString()} icon="hard-drive" tone="success" />
        <Stat label="Connected as" value={status.identity?.display_name ?? "—"} icon="user" tone="neutral"
          sub={status.identity?.username} />
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "2fr 1fr", gap: "var(--space-4)" }}>
        <Card>
          <CardHeader><CardTitle>{copy.title}</CardTitle></CardHeader>
          <p style={{ color: "var(--text-secondary)", marginTop: 0 }}>{copy.body}</p>
          {progress && <div style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)", color: "var(--text-tertiary)", marginBottom: "var(--space-3)" }}>{progress}</div>}
          {copy.button && <Button disabled={busy !== null} onClick={onPrimary}>{busy ? "Working…" : copy.button}</Button>}
        </Card>

        <Card>
          <CardHeader><CardTitle>Session</CardTitle></CardHeader>
          <p style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)", marginTop: 0 }}>
            Tokens and the mirror's encryption key are held in Windows Credential Manager. Disconnecting revokes the session but keeps local data.
          </p>
          <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
            <Button variant="secondary" disabled={busy !== null} onClick={() => run("disconnect", async () => { await api.disconnect(); return "Disconnected."; })}>Disconnect</Button>
            <Button variant="danger" disabled={busy !== null || status.synced_rows === 0} onClick={() => setConfirmPurge(true)}>Purge Local Data</Button>
          </div>
        </Card>
      </div>

      <Modal open={confirmPurge} onClose={() => setConfirmPurge(false)} title="Purge local data?" size="sm"
        footer={<>
          <Button variant="secondary" onClick={() => setConfirmPurge(false)}>Cancel</Button>
          <Button variant="danger" onClick={() => { setConfirmPurge(false); void run("purge", async () => { await api.purgeLocalData(); return "Local mirror deleted. Catalog and audit log kept."; }); }}>Purge</Button>
        </>}>
        <p style={{ margin: 0, color: "var(--text-secondary)" }}>
          This deletes every mirrored row and profile from this computer. Your object selections and the audit log are kept. This action is recorded.
        </p>
      </Modal>
    </div>
  );
}
```

`Button` variants `secondary` and `danger` exist in the design system (verified). Icon names must be lucide *canonical* names in kebab-case — the `Icon` wrapper looks them up in lucide's `icons` map, which excludes aliases (`pie-chart` and `check-square` render nothing; `chart-pie` and `square-check` work).

- [ ] **Step 2: Verify**

`npm run typecheck` clean. `npm run tauri dev` → Overview shows four stat tiles and "Scan your org". Click **Scan Metadata** → progress line ticks through objects (a few hundred describe calls; 1–3 minutes) → success alert with the count. Objects-scanned tile updates.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat(ui): overview page with pipeline status, scan/sync/profile actions, purge confirm"
```

---

### Task 12: Data page — allowlist and explorer

**Files:**
- Replace: `src/pages/DataPage.tsx`

- [ ] **Step 1: Implement**

```tsx
import { useCallback, useEffect, useMemo, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Badge, Button, Card, EmptyState, Input, Table } from "../design-system";
import { PageTitle } from "../design-system/ui-kits/grant-management/chrome.jsx";

function FillBar({ rate }: { rate: number | null }) {
  if (rate === null) return <span style={{ color: "var(--text-tertiary)" }}>—</span>;
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)" }}>
      <span style={{ width: 72, height: 6, background: "var(--color-neutral-200)", borderRadius: "var(--radius-full)", overflow: "hidden", display: "inline-block" }}>
        <span style={{ display: "block", height: "100%", width: `${Math.round(rate * 100)}%`, background: "var(--color-success-500)" }} />
      </span>
      <span style={{ fontVariantNumeric: "tabular-nums" }}>{Math.round(rate * 100)}%</span>
    </span>
  );
}

export default function DataPage({ status, refresh }: PageProps) {
  const [objects, setObjects] = useState<api.ObjectRow[]>([]);
  const [fields, setFields] = useState<api.FieldRow[]>([]);
  const [current, setCurrent] = useState<string>("");
  const [search, setSearch] = useState("");
  const [onlyPopulated, setOnlyPopulated] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const loadObjects = useCallback(async () => {
    try { const o = await api.listObjects(); setObjects(o); if (!current && o[0]) setCurrent(o[0].name); }
    catch (e) { setErr(String(e)); }
  }, [current]);
  const loadFields = useCallback(async (name: string) => {
    if (!name) return;
    try { setFields(await api.listFields(name)); } catch (e) { setErr(String(e)); }
  }, []);

  useEffect(() => { void loadObjects(); }, [loadObjects]);
  useEffect(() => { void loadFields(current); }, [current, loadFields]);

  const toggleObject = async (o: api.ObjectRow) => {
    await api.setObjectSelected(o.name, !o.selected);
    await loadObjects(); await refresh();
  };
  const toggleWithheld = async (f: api.FieldRow) => {
    await api.setFieldWithheld(current, f.field, !f.withheld);
    await loadFields(current);
  };

  const visibleObjects = useMemo(() => {
    const q = search.trim().toLowerCase();
    return objects.filter((o) => !q || o.name.toLowerCase().includes(q) || o.label.toLowerCase().includes(q));
  }, [objects, search]);
  const visibleFields = useMemo(
    () => (onlyPopulated ? fields.filter((f) => (f.fill_rate ?? 0) > 0) : fields),
    [fields, onlyPopulated]);
  const currentObj = objects.find((o) => o.name === current);

  if (status.object_count === 0) {
    return (
      <div>
        <PageTitle eyebrow="Customer Intelligence" title="Data" />
        <EmptyState icon="database" title="Nothing scanned yet" message="Run a metadata scan from the Overview page to list the objects you can mirror." />
      </div>
    );
  }

  return (
    <div>
      <PageTitle eyebrow="Customer Intelligence" title="Data" />
      {err && <Alert tone="error" style={{ marginBottom: "var(--space-4)" }}>{err}</Alert>}
      <div style={{ display: "grid", gridTemplateColumns: "340px 1fr", gap: "var(--space-4)", alignItems: "start" }}>
        <Card padded={false}>
          <div style={{ padding: "var(--space-3)", borderBottom: "1px solid var(--border-default)" }}>
            <Input placeholder="Search objects" value={search} onChange={(e: React.ChangeEvent<HTMLInputElement>) => setSearch(e.target.value)} />
          </div>
          <div style={{ maxHeight: "calc(100vh - 320px)", overflowY: "auto" }}>
            {visibleObjects.map((o) => (
              <div key={o.name} onClick={() => setCurrent(o.name)}
                style={{ display: "flex", alignItems: "center", gap: "var(--space-3)", padding: "var(--space-2) var(--space-3)", cursor: "pointer",
                  background: o.name === current ? "var(--color-primary-50)" : "transparent", borderBottom: "1px solid var(--color-neutral-100)" }}>
                <input type="checkbox" checked={o.selected} onChange={() => void toggleObject(o)} onClick={(e) => e.stopPropagation()} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{o.name}</div>
                  <div style={{ fontSize: "var(--text-2xs)", color: "var(--text-tertiary)" }}>
                    {o.record_count < 0 ? "count unavailable" : `${o.record_count.toLocaleString()} records`}
                    {o.last_synced_at ? ` · mirrored ${o.last_sync_rows?.toLocaleString()}` : ""}
                  </div>
                </div>
                {o.last_synced_at && <Badge tone="success">synced</Badge>}
              </div>
            ))}
          </div>
        </Card>

        <Card>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "var(--space-4)" }}>
            <div>
              <div style={{ fontFamily: "var(--font-mono)", fontWeight: "var(--font-semibold)" }}>{current}</div>
              <div style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>
                {currentObj?.label} · {fields.length} fields · {fields.filter((f) => f.withheld).length} withheld
              </div>
            </div>
            <label style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
              <input type="checkbox" checked={onlyPopulated} onChange={(e) => setOnlyPopulated(e.target.checked)} /> Only populated
            </label>
          </div>
          <Table
            getRowKey={(r: api.FieldRow) => r.field}
            rows={visibleFields}
            empty="No fields to show."
            columns={[
              { key: "field", header: "Field", render: (r: api.FieldRow) => (
                <span>
                  <span style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}>{r.field}</span>
                  {r.sensitive && <Badge tone={r.withheld ? "error" : "warning"} style={{ marginLeft: "var(--space-2)" }}>{r.withheld ? "withheld" : "sensitive · mirrored"}</Badge>}
                </span>) },
              { key: "type", header: "Type", render: (r: api.FieldRow) => <span style={{ color: "var(--text-tertiary)", fontSize: "var(--text-xs)" }}>{r.sf_type}</span> },
              { key: "fill", header: "Fill", render: (r: api.FieldRow) => <FillBar rate={r.fill_rate} /> },
              { key: "distinct", header: "Distinct", align: "right", render: (r: api.FieldRow) => r.distinct_count ?? "—" },
              { key: "top", header: "Top values", render: (r: api.FieldRow) => (
                <span style={{ color: "var(--text-secondary)", fontSize: "var(--text-xs)", display: "inline-block", maxWidth: 360, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{r.top_values ?? "—"}</span>) },
              { key: "gov", header: "", render: (r: api.FieldRow) => r.sensitive ? (
                <Button size="sm" variant="secondary" onClick={() => void toggleWithheld(r)}>{r.withheld ? "Include" : "Withhold"}</Button>) : null },
            ]}
          />
        </Card>
      </div>
    </div>
  );
}
```

Add `import type React from "react";` at the top if `React.ChangeEvent` is flagged.

- [ ] **Step 2: Verify**

`npm run typecheck` clean. In the app: Data page lists objects with counts; tick `Campaign` (or another small object); the header tile on Overview shows 1 selected. Fields table for the object shows sensitive fields badged **withheld** with an **Include** button; clicking it flips to "sensitive · mirrored" and Audit will show `field.override`. Back on Overview, **Sync Now** → rows mirrored → the Data page fill bars and top values populate.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat(ui): data page with object allowlist, field profile, withhold overrides"
```

---

### Task 13: Segments page

**Files:**
- Replace: `src/pages/SegmentsPage.tsx`

- [ ] **Step 1: Implement**

```tsx
import { useEffect, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Button, Card, EmptyState, Input, Select } from "../design-system";
import { PageTitle } from "../design-system/ui-kits/grant-management/chrome.jsx";

export default function SegmentsPage({ status }: PageProps) {
  const [objects, setObjects] = useState<string[]>([]);
  const [object, setObject] = useState("");
  const [fields, setFields] = useState<api.FieldRow[]>([]);
  const [filters, setFilters] = useState<api.Filter[]>([]);
  const [groupBy, setGroupBy] = useState("");
  const [result, setResult] = useState<api.SegmentResult | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    api.listObjects().then((o) => {
      const synced = o.filter((x) => x.last_synced_at).map((x) => x.name);
      setObjects(synced); if (!object && synced[0]) setObject(synced[0]);
    }).catch((e) => setErr(String(e)));
  }, [object]);
  useEffect(() => {
    if (!object) return;
    api.listFields(object).then((f) => setFields(f.filter((x) => !x.withheld && (x.fill_rate ?? 0) > 0))).catch((e) => setErr(String(e)));
  }, [object]);

  const names = fields.map((f) => f.field);
  const fieldOptions = names.map((n) => ({ value: n, label: n }));
  const add = () => setFilters([...filters, { field: names[0] ?? "", op: "=", value: "" }]);
  const patch = (i: number, p: Partial<api.Filter>) => setFilters(filters.map((f, j) => (j === i ? { ...f, ...p } : f)));
  const remove = (i: number) => setFilters(filters.filter((_, j) => j !== i));

  const run = async () => {
    setErr(null);
    try { setResult(await api.querySegment({ object, filters, group_by: groupBy || undefined })); }
    catch (e) { setErr(String(e)); setResult(null); }
  };

  if (status.synced_rows === 0) {
    return (<div><PageTitle eyebrow="Customer Intelligence" title="Segments" />
      <EmptyState icon="chart-pie" title="No mirrored data" message="Select objects on the Data page and sync them before building segments." /></div>);
  }
  const max = result?.breakdown.reduce((m, [, n]) => Math.max(m, n), 0) || 1;

  return (
    <div style={{ maxWidth: 1000 }}>
      <PageTitle eyebrow="Customer Intelligence" title="Segments" />
      {err && <Alert tone="error" style={{ marginBottom: "var(--space-4)" }}>{err}</Alert>}
      <Card style={{ marginBottom: "var(--space-4)" }}>
        <div style={{ display: "grid", gridTemplateColumns: "200px 1fr", gap: "var(--space-3)", alignItems: "center", marginBottom: "var(--space-4)" }}>
          <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>Base object</span>
          <Select value={object} options={objects.map((o) => ({ value: o, label: o }))}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => { setObject(e.target.value); setFilters([]); setGroupBy(""); setResult(null); }} />
        </div>
        {filters.map((f, i) => (
          <div key={i} style={{ display: "grid", gridTemplateColumns: "1fr 140px 1fr 40px", gap: "var(--space-2)", marginBottom: "var(--space-2)" }}>
            <Select value={f.field} options={fieldOptions} onChange={(e: React.ChangeEvent<HTMLSelectElement>) => patch(i, { field: e.target.value })} />
            <Select value={f.op} options={api.OPS.map((o) => ({ value: o, label: o }))} onChange={(e: React.ChangeEvent<HTMLSelectElement>) => patch(i, { op: e.target.value })} />
            <Input value={f.value} placeholder="Value" onChange={(e: React.ChangeEvent<HTMLInputElement>) => patch(i, { value: e.target.value })} />
            <Button variant="secondary" size="sm" onClick={() => remove(i)}>×</Button>
          </div>
        ))}
        <div style={{ display: "flex", gap: "var(--space-3)", alignItems: "center", marginTop: "var(--space-3)" }}>
          <Button variant="secondary" size="sm" onClick={add}>Add Filter</Button>
          <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>Group by</span>
          <div style={{ width: 260 }}>
            <Select value={groupBy} options={[{ value: "", label: "(none)" }, ...fieldOptions]} onChange={(e: React.ChangeEvent<HTMLSelectElement>) => setGroupBy(e.target.value)} />
          </div>
          <div style={{ flex: 1 }} />
          <Button onClick={run}>Run</Button>
        </div>
      </Card>

      {result && (
        <Card>
          <div style={{ fontFamily: "var(--font-display)", fontSize: "var(--text-3xl)", fontWeight: "var(--font-semibold)", letterSpacing: "var(--tracking-tight)" }}>
            {result.count.toLocaleString()} <span style={{ fontSize: "var(--text-sm)", color: "var(--text-tertiary)", fontWeight: "var(--font-normal)" }}>records match</span>
          </div>
          {result.breakdown.length > 0 && (
            <div style={{ marginTop: "var(--space-4)" }}>
              {result.breakdown.map(([label, n]) => (
                <div key={label} style={{ display: "grid", gridTemplateColumns: "200px 1fr 60px", alignItems: "center", gap: "var(--space-3)", marginBottom: "var(--space-1)" }}>
                  <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{label || "(blank)"}</div>
                  <div style={{ height: 14, borderRadius: "var(--radius-sm)", background: "var(--color-primary-600)", width: `${Math.max(1, (n / max) * 100)}%` }} />
                  <div style={{ fontSize: "var(--text-sm)", textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{n.toLocaleString()}</div>
                </div>
              ))}
            </div>
          )}
        </Card>
      )}
    </div>
  );
}
```

Replace the literal `×` button label with `<Icon name="x" size={14} />` imported from the design system, per the no-Unicode-glyph rule.

- [ ] **Step 2: Verify**

`npm run typecheck` clean. In the app: choose the synced object, add a filter (`Status = Completed` or similar from the top values you saw), group by a picklist field, **Run** → count and bars. A withheld field is absent from the dropdowns.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat(ui): segment builder over the local mirror"
```

---

### Task 14: Audit page

**Files:**
- Replace: `src/pages/AuditPage.tsx`

- [ ] **Step 1: Implement**

```tsx
import { useCallback, useEffect, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Badge, Button, Card, Table } from "../design-system";
import { PageTitle } from "../design-system/ui-kits/grant-management/chrome.jsx";

const PAGE = 50;
const TONE: Record<string, "primary" | "success" | "warning" | "error" | "info" | "neutral"> = {
  "auth.connect": "success", "auth.disconnect": "neutral", "scan.run": "info",
  "object.select": "primary", "object.deselect": "neutral", "field.override": "warning", "field.rewithhold": "success",
  "sync.object": "success", "sync.object_failed": "error", "profile.run": "info", "segment.query": "primary", "data.purge": "error",
};

export default function AuditPage(_: PageProps) {
  const [rows, setRows] = useState<api.AuditRow[]>([]);
  const [offset, setOffset] = useState(0);
  const load = useCallback(async (o: number) => { setRows(await api.getAudit(PAGE, o)); setOffset(o); }, []);
  useEffect(() => { void load(0); }, [load]);

  return (
    <div>
      <PageTitle eyebrow="Customer Intelligence" title="Audit" actions={
        <>
          <Button variant="secondary" size="sm" disabled={offset === 0} onClick={() => void load(Math.max(0, offset - PAGE))}>Newer</Button>
          <Button variant="secondary" size="sm" disabled={rows.length < PAGE} onClick={() => void load(offset + PAGE)}>Older</Button>
        </>
      } />
      <Card padded={false}>
        <Table
          getRowKey={(r: api.AuditRow) => r.id}
          rows={rows}
          empty="No activity recorded yet."
          columns={[
            { key: "at", header: "When", width: 190, render: (r: api.AuditRow) => <span style={{ fontVariantNumeric: "tabular-nums", fontSize: "var(--text-xs)" }}>{new Date(r.at).toLocaleString()}</span> },
            { key: "who", header: "Who", render: (r: api.AuditRow) => <span style={{ fontSize: "var(--text-xs)" }}>{r.sf_username ?? "—"}</span> },
            { key: "action", header: "Action", render: (r: api.AuditRow) => <Badge tone={TONE[r.action] ?? "neutral"}>{r.action}</Badge> },
            { key: "object", header: "Object", render: (r: api.AuditRow) => <span style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}>{r.object ?? ""}</span> },
            { key: "detail", header: "Detail", render: (r: api.AuditRow) => <span style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-2xs)", color: "var(--text-tertiary)" }}>{r.detail ?? ""}</span> },
          ]}
        />
      </Card>
    </div>
  );
}
```

- [ ] **Step 2: Verify**

`npm run typecheck` clean. Audit page lists connect, scan, selections, overrides, syncs, profile, and segment queries newest-first with badges.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat(ui): paged audit log page"
```

---

### Task 15: End-to-end verification, README, and wrap-up

**Files:**
- Create: `README.md`
- Modify: `package.json` (optional `"verify": "npm run typecheck && npm test && cd src-tauri && cargo test"`)

- [ ] **Step 1: Full automated check**

```bash
npm run typecheck && npm test && (cd src-tauri && cargo test)
```
Expected: TypeScript clean, 2 Vitest tests pass, all Rust tests pass.

- [ ] **Step 2: Manual E2E against the production org (read-only)** — record the outcome of each in the final report:

1. `npm run tauri dev` → signed-out card → **Connect to Salesforce** → browser login → header shows display name.
2. Overview → **Scan Metadata** → progress → objects-scanned tile > 0.
3. Data → tick one small object (e.g. `Campaign`) → a sensitive field shows **withheld**.
4. Overview → **Sync Now** → rows mirrored > 0; Data page shows fill bars and top values; withheld field has no values.
5. Segments → filter + group-by → count and bars.
6. Audit → every step above present, newest first.
7. Close the app, reopen → still connected (identity recovered from keychain, no re-login), data still present.
8. Overview → **Disconnect** → signed-out; reconnect → objects and data still there without rescan.
9. `%APPDATA%\org.emanuelnyc.customerintelligence\mirror.db` exists; opening it with a hex viewer shows no `SQLite format 3` header.
10. Windows Credential Manager (Control Panel → Credential Manager → Windows Credentials) shows `emanuel-customer-intelligence` entries for `salesforce_tokens` and `db_key`.

- [ ] **Step 3: README**

Create `README.md`:

```markdown
# Emanuel Customer Intelligence

Desktop app (Tauri v2, Rust + React) that mirrors a governed, user-selected subset of
Temple Emanu-El's Salesforce data into an encrypted local database, profiles it, and
lets staff build simple segments. Read-only against Salesforce. No server, no cloud copy.

Design: `docs/superpowers/specs/2026-08-25-customer-intelligence-v1-design.md`

## Setup
1. Salesforce admin: External Client App with callback `http://localhost:1717/callback`,
   scopes `api refresh_token openid id profile email`, PKCE required, **Require Secret
   for Web Server Flow and Refresh Token Flow both OFF**.
2. Copy `.env.example` to `.env`; set `SF_CLIENT_ID` to the Consumer Key and
   `SF_LOGIN_URL` to the org's My Domain URL.
3. `npm install` then `npm run tauri dev`.

## Governance model
- The webview can only call named Rust commands (`src-tauri/src/commands.rs`).
- Scan copies metadata only. Rows are mirrored only for objects you select, and only for
  fields not withheld. Fields that look sensitive are withheld by default; overriding
  one is recorded in the audit log.
- Tokens and the database key live in Windows Credential Manager. The mirror
  (`%APPDATA%\org.emanuelnyc.customerintelligence\mirror.db`) is SQLCipher-encrypted.
- `_audit` is append-only: there is no code path that edits or deletes it.

## Verify
`npm run typecheck && npm test && (cd src-tauri && cargo test)`
```

- [ ] **Step 4: Commit and report**

```bash
git add -A && git commit -m "docs: README with setup and governance model"
```

Report to the user: which E2E steps passed, which (if any) failed with the exact error, and the object you used for the first sync.
