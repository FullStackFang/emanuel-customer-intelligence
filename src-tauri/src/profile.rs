//! Column profiler: which fields actually carry signal. Sensitive columns never
//! have their values materialised into _profile, even when a user overrode
//! withholding and mirrored them.

use crate::store::{ident, Store};
use anyhow::Result;
use rusqlite::params;

const NAME_HITS: &[&str] = &[
    "note",
    "medical",
    "health",
    "private",
    "confidential",
    "ssn",
    "dob",
    "birth",
    "diagnos",
    "pastoral",
    "counsel",
    "disab",
    "allerg",
    "emergency",
    "death",
    "deceased",
    "yahrzeit",
    "bereave",
    "hospital",
    "illness",
];

pub fn is_sensitive(field: &str, sf_type: &str) -> bool {
    let f = field.to_ascii_lowercase();
    NAME_HITS.iter().any(|k| f.contains(k))
        || matches!(sf_type, "textarea" | "richtextarea" | "encryptedstring")
}

pub fn profile_object(store: &Store, object: &str) -> Result<()> {
    let conn = store.conn();
    let tbl = ident(object)?;
    let row_count: i64 =
        conn.query_row(&format!("SELECT COUNT(*) FROM {tbl}"), [], |r| r.get(0))?;
    for f in store.list_fields(object)? {
        if f.withheld {
            continue; // not on disk; nothing to profile
        }
        let col = ident(&f.field)?;
        let non_null: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {tbl} WHERE {col} IS NOT NULL AND {col} <> ''"),
            [],
            |r| r.get(0),
        )?;
        let distinct: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(DISTINCT {col}) FROM {tbl} WHERE {col} IS NOT NULL AND {col} <> ''"
            ),
            [],
            |r| r.get(0),
        )?;
        let top_values = if f.sensitive {
            "[hidden: sensitive]".to_string()
        } else {
            let mut st = conn.prepare(&format!(
                "SELECT {col}, COUNT(*) c FROM {tbl} WHERE {col} IS NOT NULL AND {col} <> ''
                 GROUP BY {col} ORDER BY c DESC, {col} LIMIT 5"
            ))?;
            let pairs = st
                .query_map([], |r| {
                    Ok(format!(
                        "{} ({})",
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            pairs.join(" | ")
        };
        let fill = if row_count > 0 {
            non_null as f64 / row_count as f64
        } else {
            0.0
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::salesforce::Row;
    use crate::store;

    #[test]
    fn sensitivity_heuristic_table() {
        let yes = [
            ("Pastoral_Notes__c", "string"),
            ("MedicalInfo__c", "string"),
            ("Description", "textarea"),
            ("Bio", "richtextarea"),
            ("Yahrzeit_Date__c", "date"),
            ("Deceased__c", "boolean"),
            ("Emergency_Contact__c", "string"),
            ("SSN__c", "encryptedstring"),
            ("Birthdate", "date"),
        ];
        let no = [
            ("Name", "string"),
            ("Email", "email"),
            ("AnnualRevenue", "currency"),
            ("Status", "picklist"),
        ];
        for (f, t) in yes {
            assert!(is_sensitive(f, t), "{f}/{t} should be sensitive");
        }
        for (f, t) in no {
            assert!(!is_sensitive(f, t), "{f}/{t} should NOT be sensitive");
        }
    }

    #[test]
    fn profile_computes_fill_distinct_top_and_hides_sensitive_values() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store::open(&dir.path().join("p.db"), "00".repeat(32).as_str()).unwrap();
        s.upsert_object("Contact", "Contact", 3).unwrap();
        s.upsert_field("Contact", "City", "string", "City", false)
            .unwrap();
        s.upsert_field("Contact", "Notes__c", "textarea", "Notes", true)
            .unwrap();
        s.set_field_withheld("Contact", "Notes__c", false).unwrap(); // overridden → mirrored but values hidden
        let mk = |city: Option<&str>, notes: &str| {
            let mut m = Row::new();
            m.insert(
                "City".into(),
                city.map(|c| serde_json::Value::String(c.into()))
                    .unwrap_or(serde_json::Value::Null),
            );
            m.insert("Notes__c".into(), serde_json::Value::String(notes.into()));
            m
        };
        let cols = s.sync_columns("Contact").unwrap();
        s.replace_mirror(
            "Contact",
            &cols,
            &[
                mk(Some("NYC"), "secret a"),
                mk(Some("NYC"), "secret b"),
                mk(None, "secret c"),
            ],
        )
        .unwrap();
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
