//! Read-only diagnostic: value distributions for chosen columns of a mirror
//! table. Aggregate counts only — never dumps individual records.
//! Usage: cargo run --example dist -- "<mirror.db>" <Object> [--by Col] Field[:year] ...
//!   Field       -> GROUP BY Field, top 15 values
//!   Field:year  -> GROUP BY substr(Field,1,4) (for date/datetime columns), all years
//!   --by Col    -> additionally split every distribution by Col's values

use rusqlite::{Connection, OpenFlags};

fn ident(s: &str) -> String {
    assert!(
        !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "bad identifier {s:?}"
    );
    format!("\"{s}\"")
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("arg1: mirror.db path");
    let object = ident(&args.next().expect("arg2: object"));
    let mut by: Option<String> = None;
    let mut specs: Vec<String> = Vec::new();
    while let Some(a) = args.next() {
        if a == "--by" {
            by = Some(ident(&args.next().expect("--by needs a column")));
        } else {
            specs.push(a);
        }
    }

    let key = keyring::v1::Entry::new("emanuel-customer-intelligence", "db_key")?
        .get_password()?;
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.pragma_update(None, "key", format!("x'{key}'"))?;

    let total: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {object}"), [], |r| r.get(0))?;
    println!("{object}: {total} rows{}", by.as_ref().map(|b| format!("  (split by {b})")).unwrap_or_default());

    for spec in specs {
        let (field, year) = match spec.strip_suffix(":year") {
            Some(f) => (ident(f), true),
            None => (ident(&spec), false),
        };
        let expr = if year { format!("substr({field},1,4)") } else { field.clone() };
        // Top-N per group (window function), so a --by split never lets one
        // group's long tail crowd out the other group's top values.
        let per_group = if year { 400 } else { 15 };
        let grp_col = by.clone().unwrap_or_else(|| "NULL".to_string());
        let rank_ord = if year { "v" } else { "n DESC" };
        let sql = format!(
            "WITH agg AS (SELECT {grp_col} AS g, {expr} AS v, COUNT(*) AS n FROM {object} GROUP BY g, v), \
                  ranked AS (SELECT g, v, n, ROW_NUMBER() OVER (PARTITION BY g ORDER BY {rank_ord}) AS rk FROM agg) \
             SELECT g, v, n FROM ranked WHERE rk <= {per_group} ORDER BY g, {rank_ord}"
        );
        println!("\n-- {spec} --");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (b, v, n) = row?;
            let v = v.map(|s| s.chars().take(60).collect::<String>()).unwrap_or("<null>".into());
            match b {
                Some(b) => println!("  [{b}] {v}: {n}"),
                None => println!("  {v}: {n}"),
            }
        }
    }
    Ok(())
}
