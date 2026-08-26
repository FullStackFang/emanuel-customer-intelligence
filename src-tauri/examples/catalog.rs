//! Read-only diagnostic: search the scanned object catalog (_objects) by
//! keyword and print name, label, Salesforce record count, and sync state.
//! Usage: cargo run --example catalog -- "<mirror.db>" kw1|kw2|...

use rusqlite::{Connection, OpenFlags};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("arg1: mirror.db path");
    let kws: Vec<String> = args
        .next()
        .unwrap_or_default()
        .split('|')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect();

    let key = keyring::v1::Entry::new("emanuel-customer-intelligence", "db_key")?.get_password()?;
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.pragma_update(None, "key", format!("x'{key}'"))?;

    let mut stmt = conn.prepare(
        "SELECT name, label, record_count, selected, last_sync_rows \
         FROM _objects ORDER BY record_count DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)? != 0,
            r.get::<_, Option<i64>>(4)?,
        ))
    })?;
    let mut n = 0;
    for row in rows {
        let (name, label, count, selected, synced) = row?;
        let l = format!("{name} {label}").to_lowercase();
        if !kws.is_empty() && !kws.iter().any(|k| l.contains(k.as_str())) {
            continue;
        }
        n += 1;
        let state = match (selected, synced) {
            (_, Some(r)) => format!("synced {r}"),
            (true, None) => "selected".into(),
            _ => String::new(),
        };
        println!("{count:>8}  {name}  \"{label}\"  {state}");
    }
    println!("\n{n} objects");
    Ok(())
}
