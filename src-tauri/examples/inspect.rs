//! Read-only diagnostic: open the encrypted mirror with the real keychain key
//! and print object/audit/mirror-table COUNTS only (never record values).
//! Usage: cargo run --example inspect -- "<path-to-mirror.db>"

use rusqlite::{Connection, OpenFlags};

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("pass the mirror.db path as arg 1");
    let key = keyring::v1::Entry::new("emanuel-customer-intelligence", "db_key")?
        .get_password()?;

    // READ-ONLY so we never write while the app holds the DB open.
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.pragma_update(None, "key", format!("x'{key}'"))?;

    println!("== selected objects ==");
    {
        let mut stmt = conn.prepare(
            "SELECT name, record_count, last_synced_at, last_sync_rows \
             FROM _objects WHERE selected = 1 ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })?;
        for row in rows {
            let (name, sf_count, synced_at, mirrored) = row?;
            println!(
                "  {name}: sf_total={sf_count}  mirrored_rows={:?}  last_synced={:?}",
                mirrored, synced_at
            );
        }
    }

    println!("\n== last 15 audit entries ==");
    {
        let mut stmt = conn.prepare(
            "SELECT at, action, object, detail FROM _audit ORDER BY id DESC LIMIT 15",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        for row in rows {
            let (at, action, object, detail) = row?;
            println!("  {at}  {action}  {:?}  {:?}", object, detail);
        }
    }

    println!("\n== mirror tables (row counts only) ==");
    {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' \
             AND name NOT LIKE '\\_%' ESCAPE '\\' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        let names: Vec<String> =
            stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        if names.is_empty() {
            println!("  (no mirror tables exist yet)");
        }
        for t in names {
            let count: i64 =
                conn.query_row(&format!("SELECT COUNT(*) FROM \"{t}\""), [], |r| r.get(0))?;
            println!("  {t}: {count} rows");
        }
    }

    Ok(())
}
