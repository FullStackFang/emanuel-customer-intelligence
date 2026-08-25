//! Read-only diagnostic: run a SELECT (from a file) against the encrypted
//! mirror and print the result as tab-separated rows. The connection is opened
//! READ_ONLY, so nothing here can modify the mirror.
//! Usage: cargo run --example sql -- "<mirror.db>" query.sql

use rusqlite::{types::ValueRef, Connection, OpenFlags};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("arg1: mirror.db path");
    let file = args.next().expect("arg2: .sql file containing one SELECT");
    let sql = std::fs::read_to_string(&file)?;

    let key = keyring::v1::Entry::new("emanuel-customer-intelligence", "db_key")?
        .get_password()?;
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.pragma_update(None, "key", format!("x'{key}'"))?;

    let mut stmt = conn.prepare(&sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    println!("{}", names.join("\t"));
    let ncol = names.len();
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let mut cells = Vec::with_capacity(ncol);
        for i in 0..ncol {
            let v = match r.get_ref(i)? {
                ValueRef::Null => String::new(),
                ValueRef::Integer(n) => n.to_string(),
                ValueRef::Real(f) => format!("{f}"),
                ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                ValueRef::Blob(_) => "<blob>".into(),
            };
            cells.push(v);
        }
        println!("{}", cells.join("\t"));
    }
    Ok(())
}
