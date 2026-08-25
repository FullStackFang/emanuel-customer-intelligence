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
            let sql_op = match f.op.as_str() {
                "=" => "=",
                "!=" => "!=",
                ">" => ">",
                "<" => "<",
                ">=" => ">=",
                "<=" => "<=",
                _ => unreachable!("operator already validated against ALLOWED_OPS"),
            };
            clauses.push(format!("{col} {sql_op} ?"));
            binds.push(f.value.clone());
        }
    }
    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let count_sql = format!("SELECT COUNT(*) FROM {tbl}{where_sql}");
    let breakdown_sql = match &req.group_by {
        Some(g) if !g.is_empty() => {
            let gcol = column(g, allowed)?;
            Some(format!("SELECT {gcol}, COUNT(*) c FROM {tbl}{where_sql} GROUP BY {gcol} ORDER BY c DESC LIMIT 20"))
        }
        _ => None,
    };
    Ok(Built {
        count_sql,
        breakdown_sql,
        binds,
    })
}

pub fn run(store: &Store, req: &SegmentReq) -> Result<SegmentResult> {
    let allowed = store.allowed_fields(&req.object)?;
    if allowed.is_empty() {
        anyhow::bail!("object is not synced: {}", req.object);
    }
    let b = build(req, &allowed).map_err(anyhow::Error::msg)?;
    let conn = store.conn();
    let count: i64 = conn.query_row(
        &b.count_sql,
        rusqlite::params_from_iter(b.binds.iter()),
        |r| r.get(0),
    )?;
    let mut breakdown = Vec::new();
    if let Some(sql) = &b.breakdown_sql {
        let mut st = conn.prepare(sql)?;
        let rows = st.query_map(rusqlite::params_from_iter(b.binds.iter()), |r| {
            Ok((
                r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                r.get::<_, i64>(1)?,
            ))
        })?;
        breakdown = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    }
    Ok(SegmentResult { count, breakdown })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn allowed() -> HashSet<String> {
        ["Name", "Status", "Amount"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
    fn req(filters: Vec<(&str, &str, &str)>, group_by: Option<&str>) -> SegmentReq {
        SegmentReq {
            object: "Campaign".into(),
            filters: filters
                .into_iter()
                .map(|(f, o, v)| Filter {
                    field: f.into(),
                    op: o.into(),
                    value: v.into(),
                })
                .collect(),
            group_by: group_by.map(String::from),
        }
    }

    #[test]
    fn builds_bound_sql_with_and_filters_and_group_by() {
        let b = build(
            &req(
                vec![("Status", "=", "Done"), ("Name", "contains", "Gala")],
                Some("Status"),
            ),
            &allowed(),
        )
        .unwrap();
        assert_eq!(
            b.count_sql,
            "SELECT COUNT(*) FROM \"Campaign\" WHERE \"Status\" = ? AND \"Name\" LIKE ?"
        );
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
        assert!(build(&req(vec![("Notes__c", "=", "x")], None), &allowed())
            .unwrap_err()
            .contains("Notes__c"));
        assert!(build(&req(vec![("Name", "LIKE", "x")], None), &allowed())
            .unwrap_err()
            .contains("LIKE"));
        assert!(build(&req(vec![], Some("Nope")), &allowed())
            .unwrap_err()
            .contains("Nope"));
        let mut r = req(vec![], None);
        r.object = "Campaign\"; DROP TABLE _audit; --".into();
        assert!(build(&r, &allowed()).is_err());
        let mut evil = allowed();
        evil.insert("Name\" OR 1=1 --".into());
        assert!(build(&req(vec![("Name\" OR 1=1 --", "=", "x")], None), &evil).is_err());
    }

    #[test]
    fn every_comparison_operator_maps_to_its_own_sql_fragment() {
        for op in ["=", "!=", ">", "<", ">=", "<="] {
            let b = build(&req(vec![("Amount", op, "42")], None), &allowed()).unwrap();
            assert!(
                b.count_sql.contains(&format!("\"Amount\" {op} ?")),
                "op {op:?}: expected fragment in {:?}",
                b.count_sql
            );
            assert_eq!(
                b.binds,
                vec!["42".to_string()],
                "op {op:?}: value must be bound, not inlined"
            );
            assert!(
                !b.count_sql.contains("42"),
                "op {op:?}: value must not appear in SQL"
            );
        }
    }

    #[test]
    fn values_are_never_interpolated() {
        let b = build(
            &req(vec![("Name", "=", "'; DROP TABLE _audit; --")], None),
            &allowed(),
        )
        .unwrap();
        assert!(!b.count_sql.contains("DROP"));
        assert_eq!(b.binds[0], "'; DROP TABLE _audit; --");
    }
}
