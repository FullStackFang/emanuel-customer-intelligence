//! Read-only diagnostic: list fields for an object from the encrypted mirror
//! catalog, with profile stats (fill rate, distinct count, top values).
//! Aggregate/profile data only — never dumps individual records.
//! Usage: cargo run --example fields -- "<mirror.db>" <Object> [kw1|kw2|...]
//! Keywords are case-insensitive substrings matched against field name or label.

use rusqlite::{Connection, OpenFlags};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("arg1: mirror.db path");
    let object = args.next().expect("arg2: object API name");
    let kws: Vec<String> = args
        .next()
        .unwrap_or_default()
        .split('|')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect();
    let matches = |s: &str| {
        let l = s.to_lowercase();
        kws.is_empty() || kws.iter().any(|k| l.contains(k.as_str()))
    };

    let key = keyring::v1::Entry::new("emanuel-customer-intelligence", "db_key")?
        .get_password()?;
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.pragma_update(None, "key", format!("x'{key}'"))?;

    let mut stmt = conn.prepare(
        "SELECT f.field, f.sf_type, f.label, f.sensitive, f.withheld,
                p.fill_rate, p.distinct_count, p.top_values
         FROM _fields f LEFT JOIN _profile p ON p.object = f.object AND p.field = f.field
         WHERE f.object = ?1 ORDER BY f.field",
    )?;
    let rows = stmt.query_map([&object], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)? != 0,
            r.get::<_, i64>(4)? != 0,
            r.get::<_, Option<f64>>(5)?,
            r.get::<_, Option<i64>>(6)?,
            r.get::<_, Option<String>>(7)?,
        ))
    })?;

    let mut n = 0;
    for row in rows {
        let (field, ty, label, sensitive, withheld, fill, distinct, top) = row?;
        if !(matches(&field) || matches(&label)) {
            continue;
        }
        n += 1;
        let fill_s = fill.map(|f| format!("{:.0}%", f * 100.0)).unwrap_or("-".into());
        let dist_s = distinct.map(|d| d.to_string()).unwrap_or("-".into());
        let flag = if withheld { " [WITHHELD]" } else if sensitive { " [sensitive]" } else { "" };
        let top_s = top
            .map(|t| t.chars().take(160).collect::<String>())
            .unwrap_or_default();
        println!("{field}  ({ty})  \"{label}\"{flag}\n    fill={fill_s} distinct={dist_s}  top={top_s}");
    }
    println!("\n{n} matching fields on {object}");
    Ok(())
}
