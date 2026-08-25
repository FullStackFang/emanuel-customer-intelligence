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
    // NOTE: the brief's `cipher_memory_security` PRAGMA is intentionally omitted here.
    // On this environment (Windows + rusqlite 0.40 bundled-sqlcipher-vendored-openssl,
    // vendored OpenSSL 3.6.3) enabling it triggers VirtualLock() failures
    // (ERROR_WORKING_SET_QUOTA) whose fallback path corrupts process state, causing a
    // later `open()` call in the same process to crash with a stack overflow. Verified
    // by isolating: reproduced with the pragma present (deterministic crash across the
    // 4-test run), gone with it removed (4/4 pass, no special thread/stack tuning
    // needed). See task-2-report.md for the full repro. Flagging as a deviation from
    // the brief's verbatim code rather than silently carrying a real crash forward.
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
        assert_eq!(
            ident("npsp__Household__c").unwrap(),
            "\"npsp__Household__c\""
        );
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
            s.conn()
                .execute("INSERT INTO _meta(key, value) VALUES('a','1')", [])
                .unwrap();
        }
        let s = open(&path, KEY).unwrap();
        let v: String = s
            .conn()
            .query_row("SELECT value FROM _meta WHERE key='a'", [], |r| r.get(0))
            .unwrap();
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
        assert!(
            !bytes.starts_with(b"SQLite format 3"),
            "header must be encrypted"
        );
    }
}
