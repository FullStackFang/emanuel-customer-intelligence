//! Local encrypted mirror: schema, catalog, selection, mirror tables, audit.
//! Everything is TEXT — a faithful mirror; the profiler infers meaning.

use crate::salesforce::Row;
use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashSet;
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
            .query_row(
                "SELECT value FROM _meta WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
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

    pub fn upsert_field(
        &self,
        object: &str,
        field: &str,
        sf_type: &str,
        label: &str,
        sensitive: bool,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO _fields(object, field, sf_type, label, sensitive, withheld)
             VALUES(?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(object, field) DO UPDATE SET
               sf_type = excluded.sf_type, label = excluded.label, sensitive = excluded.sensitive,
               withheld = CASE WHEN _fields.sensitive = 0 AND excluded.sensitive = 1 THEN 1 ELSE _fields.withheld END",
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
        let mut st = self
            .conn
            .prepare("SELECT name FROM _objects WHERE selected = 1 ORDER BY name")?;
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
        let mut st = self.conn.prepare(
            "SELECT field FROM _fields WHERE object = ?1 AND withheld = 0 ORDER BY field",
        )?;
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
            qcols
                .iter()
                .map(|c| format!("{c} TEXT"))
                .collect::<Vec<_>>()
                .join(", ")
        ))?;
        let placeholders = vec!["?"; qcols.len()].join(",");
        let sql = format!(
            "INSERT INTO {tbl} ({}) VALUES ({placeholders})",
            qcols.join(",")
        );
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
        let mut st = self
            .conn
            .prepare("SELECT name FROM _objects WHERE last_synced_at IS NOT NULL ORDER BY name")?;
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

    /// Read every row of a mirror table as (column -> value) maps. Empty if the table
    /// is absent. Column order follows `mirror_columns`.
    pub fn mirror_rows(
        &self,
        object: &str,
    ) -> Result<Vec<std::collections::HashMap<String, Option<String>>>> {
        let cols = self.mirror_columns(object)?;
        if cols.is_empty() {
            return Ok(Vec::new());
        }
        let tbl = ident(object)?;
        let select = cols
            .iter()
            .map(|c| ident(c))
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        let mut st = self.conn.prepare(&format!("SELECT {select} FROM {tbl}"))?;
        let rows = st.query_map([], |r| {
            let mut m = std::collections::HashMap::with_capacity(cols.len());
            for (i, c) in cols.iter().enumerate() {
                m.insert(c.clone(), r.get::<_, Option<String>>(i)?);
            }
            Ok(m)
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Column names of a mirror table (empty if the table does not exist).
    pub fn mirror_columns(&self, object: &str) -> Result<Vec<String>> {
        if !self.table_exists(object)? {
            return Ok(Vec::new());
        }
        let mut st = self
            .conn
            .prepare(&format!("PRAGMA table_info({})", ident(object)?))?;
        let rows = st.query_map([], |r| r.get::<_, String>(1))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn table_exists(&self, name: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// The most recent `last_synced_at` across all objects.
    pub fn newest_sync_at(&self) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT MAX(last_synced_at) FROM _objects", [], |r| r.get(0))?)
    }

    pub fn purge_mirror(&mut self) -> Result<()> {
        let names = self.synced_objects()?;
        let tx = self.conn.transaction()?;
        for n in names {
            tx.execute_batch(&format!("DROP TABLE IF EXISTS {}", ident(&n)?))?;
        }
        tx.execute_batch(
            "DROP TABLE IF EXISTS _m_household;
             DROP TABLE IF EXISTS _m_household_fy;
             DELETE FROM _profile;
             DELETE FROM _meta WHERE key IN ('insights_built_at', 'insights_unavailable');
             UPDATE _objects SET last_synced_at = NULL, last_sync_rows = NULL;",
        )?;
        tx.commit()?;
        Ok(())
    }

    // ── audit (insert + read only; there is intentionally no update/delete) ──
    pub fn audit(
        &self,
        who: &Who,
        action: &str,
        object: Option<&str>,
        detail: Option<serde_json::Value>,
    ) -> Result<()> {
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
                id: r.get(0)?,
                at: r.get(1)?,
                sf_user_id: r.get(2)?,
                sf_username: r.get(3)?,
                action: r.get(4)?,
                object: r.get(5)?,
                detail: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ── status ──────────────────────────────────────────────────────────
    pub fn status(&self) -> Result<Status> {
        let object_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM _objects", [], |r| r.get(0))?;
        let selected_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM _objects WHERE selected = 1",
            [],
            |r| r.get(0),
        )?;
        let synced_rows: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(last_sync_rows), 0) FROM _objects",
            [],
            |r| r.get(0),
        )?;
        Ok(Status {
            object_count,
            selected_count,
            synced_rows,
            last_scan_at: self.get_meta("last_scan_at")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    const OTHER: &str = "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100";

    /// Returns the TempDir too so it lives as long as the Store.
    fn mem() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = open(&dir.path().join("t.db"), KEY).unwrap();
        (dir, s)
    }

    fn who() -> Who {
        Who {
            sf_user_id: Some("005".into()),
            sf_username: Some("u@x".into()),
        }
    }

    #[test]
    fn rescan_preserves_selection_and_withheld_overrides() {
        let (_d, s) = mem();
        s.upsert_object("Account", "Account", 10).unwrap();
        s.upsert_field("Account", "Name", "string", "Name", false)
            .unwrap();
        s.upsert_field("Account", "Notes__c", "textarea", "Notes", true)
            .unwrap();
        s.set_object_selected("Account", true).unwrap();
        assert!(s.set_field_withheld("Account", "Notes__c", false).unwrap());
        // second scan with changed count/label
        s.upsert_object("Account", "Account (org)", 12).unwrap();
        s.upsert_field("Account", "Notes__c", "textarea", "Notes", true)
            .unwrap();
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
        s.upsert_field("Contact", "Email", "email", "Email", false)
            .unwrap();
        s.upsert_field("Contact", "Medical__c", "string", "Medical", true)
            .unwrap();
        let f = s.list_fields("Contact").unwrap();
        assert!(!f.iter().find(|x| x.field == "Email").unwrap().withheld);
        assert!(f.iter().find(|x| x.field == "Medical__c").unwrap().withheld);
        assert_eq!(
            s.sync_columns("Contact").unwrap(),
            vec!["Email".to_string()]
        );
        assert!(!s.set_field_withheld("Contact", "Email", true).unwrap());
    }

    #[test]
    fn upsert_field_rewithholds_on_non_sensitive_to_sensitive_transition() {
        let (_d, s) = mem();
        s.upsert_object("Contact", "Contact", 1).unwrap();
        // First scan: field is non-sensitive, so withheld defaults to 0.
        s.upsert_field("Contact", "Mobile__c", "string", "Mobile", false)
            .unwrap();
        let f = s.list_fields("Contact").unwrap();
        assert!(!f.iter().find(|x| x.field == "Mobile__c").unwrap().withheld);
        // Second scan: same field is now classified sensitive.
        s.upsert_field("Contact", "Mobile__c", "string", "Mobile", true)
            .unwrap();
        let f = s.list_fields("Contact").unwrap();
        let mobile = f.iter().find(|x| x.field == "Mobile__c").unwrap();
        assert!(mobile.sensitive);
        assert!(
            mobile.withheld,
            "must re-withhold on non-sensitive -> sensitive transition"
        );
    }

    #[test]
    fn replace_mirror_creates_table_and_marks_synced() {
        let (_d, mut s) = mem();
        s.upsert_object("Campaign", "Campaign", 2).unwrap();
        s.upsert_field("Campaign", "Name", "string", "Name", false)
            .unwrap();
        s.upsert_field("Campaign", "Status", "picklist", "Status", false)
            .unwrap();
        let mk = |n: &str, st: &str| {
            let mut m = Row::new();
            m.insert("Name".into(), serde_json::Value::String(n.into()));
            m.insert("Status".into(), serde_json::Value::String(st.into()));
            m
        };
        let cols = s.sync_columns("Campaign").unwrap();
        let n = s
            .replace_mirror("Campaign", &cols, &[mk("A", "Planned"), mk("B", "Done")])
            .unwrap();
        assert_eq!(n, 2);
        let n2 = s
            .replace_mirror("Campaign", &cols, &[mk("C", "Done")])
            .unwrap();
        assert_eq!(n2, 1);
        let cnt: i64 = s
            .conn()
            .query_row("SELECT COUNT(*) FROM \"Campaign\"", [], |r| r.get(0))
            .unwrap();
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
        s.audit(
            &who(),
            "sync.object",
            Some("Account"),
            Some(serde_json::json!({"rows": 3})),
        )
        .unwrap();
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
        assert_eq!(
            (st.object_count, st.selected_count, st.synced_rows),
            (1, 1, 1)
        );
        assert_eq!(st.last_scan_at.as_deref(), Some("2026-08-25T00:00:00Z"));
        s.purge_mirror().unwrap();
        assert_eq!(s.status().unwrap().synced_rows, 0);
        assert!(s.synced_objects().unwrap().is_empty());
        assert_eq!(s.list_objects().unwrap().len(), 1, "catalog survives purge");
    }

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
