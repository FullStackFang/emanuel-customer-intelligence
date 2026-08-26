//! Membership insights: fiscal-year math, the household mart, and the views
//! the Insights page renders. Reads the mirror only; never Salesforce.

use crate::store::{ident, Store};
use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

/// First month of the fiscal year (June). FY is labeled by the calendar year it ends in.
pub const FY_START_MONTH: u32 = 6;
/// Dates outside this window are placeholders (2199-…, 2991-…) or garbage.
const MIN_YEAR: i32 = 1900;
const MAX_YEAR: i32 = 2035;

pub fn fy_from_ymd(y: i32, m: u32) -> i32 {
    if m >= FY_START_MONTH {
        y + 1
    } else {
        y
    }
}

/// Fiscal year of a Salesforce date/datetime string (`YYYY-MM-DD…`), or None if
/// unparsable or outside the plausible window.
pub fn fy_of(date: &str) -> Option<i32> {
    let d = date.get(0..10)?;
    let nd = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
    let y = chrono::Datelike::year(&nd);
    if !(MIN_YEAR..=MAX_YEAR).contains(&y) {
        return None;
    }
    Some(fy_from_ymd(y, chrono::Datelike::month(&nd)))
}

pub fn current_fy() -> i32 {
    let today = chrono::Utc::now().date_naive();
    fy_from_ymd(
        chrono::Datelike::year(&today),
        chrono::Datelike::month(&today),
    )
}

/// (mart column key, phrase to look for in `Join_Reason__c`). Order is stable: it is
/// the index into `channel_flags` and the mart's `ch_*` columns.
pub const CHANNELS: [(&str, &str); 12] = [
    ("religious_school", "religious school"),
    ("nursery_school", "nursery school"),
    ("affiliation", "affiliation"),
    ("life_cycle", "life cycle event"),
    ("family", "to be with family"),
    ("young_professionals", "young professionals"),
    ("community", "community"),
    ("hhd_tickets", "high holy day"),
    ("streicker", "streicker"),
    ("clergy", "clergy"),
    ("worship", "worship"),
    ("move", "move"),
];

pub fn channel_flags(join_reason: Option<&str>) -> [bool; 12] {
    let mut out = [false; 12];
    if let Some(jr) = join_reason {
        let l = jr.to_lowercase();
        for (i, (_, phrase)) in CHANNELS.iter().enumerate() {
            out[i] = l.contains(phrase);
        }
    }
    out
}

/// Coded resign reason -> display group. First match wins, in this order.
pub fn reason_group(raw: Option<&str>) -> &'static str {
    let Some(r) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return "(not coded)";
    };
    let l = r.to_lowercase();
    const RULES: [(&str, &str); 9] = [
        ("moved", "Moved"),
        ("non-payment", "Non-payment"),
        ("no longer engaged", "No longer engaged"),
        ("deceased", "Deceased"),
        ("aged out", "Young-adult tier aged out"),
        ("another synagogue", "Joined another synagogue"),
        ("elderly", "Elderly / ill"),
        ("financial", "Financial hardship"),
        ("displeased", "Displeased"),
    ];
    for (needle, group) in RULES {
        if l.contains(needle) {
            return group;
        }
    }
    "Other"
}

/// `LastYearAttendedRS__c` is "2025-2026" or "2007"; take the last 4-digit year.
pub fn parse_rs_year(s: Option<&str>) -> Option<i32> {
    let s = s?.trim();
    if s.is_empty() {
        return None;
    }
    s.rsplit('-').next()?.trim().parse::<i32>().ok()
}

pub const MART: &str = "_m_household";

/// Account columns the mart derives from. A missing one nulls what depends on it and is
/// reported in `RebuildInfo::unavailable`; `Type` and `IsATempleMember__c` are mandatory.
pub const REQUIRED_COLUMNS: [&str; 16] = [
    "Id",
    "Name",
    "Type",
    "IsATempleMember__c",
    "IsResigned__c",
    "Join_Date__c",
    "OriginalJoinDate__c",
    "LastResignDate__c",
    "Sub_Type__c",
    "Member_Category__c",
    "Join_Reason__c",
    "Resign_Reason__c",
    "FormerReligiousSchoolStudents__c",
    "ActiveReligiousSchoolStudents__c",
    "WasEverNSAffiliated__c",
    "LastYearAttendedRS__c",
];

#[derive(Serialize, Debug, Clone)]
pub struct RebuildInfo {
    pub households: usize,
    pub unavailable: Vec<String>,
}

/// One household from the mart. Everything the views need, nothing else.
#[derive(Debug, Clone, Default)]
pub struct Hh {
    pub account_id: String,
    pub name: Option<String>,
    pub is_current: bool,
    pub is_resigned: bool,
    pub join_fy: Option<i32>,
    pub cohort_fy: Option<i32>,
    pub resign_fy: Option<i32>,
    pub resigned_unknown_date: bool,
    pub bad_join_date: bool,
    pub rejoined: bool,
    pub tier: Option<String>,
    pub category: Option<String>,
    pub join_reason: Option<String>,
    pub ch: [bool; 12],
    pub rs_family: bool,
    pub ns_family: bool,
    pub active_rs_students: i64,
    pub last_rs_year: Option<i32>,
    pub resign_reason_group: String,
}

fn mart_ddl() -> String {
    let flags = CHANNELS
        .iter()
        .map(|(k, _)| format!("ch_{k} INTEGER NOT NULL DEFAULT 0"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "CREATE TABLE {MART}(
           account_id TEXT PRIMARY KEY, name TEXT,
           is_current INTEGER NOT NULL, is_resigned INTEGER NOT NULL,
           join_fy INTEGER, cohort_fy INTEGER, resign_fy INTEGER,
           resigned_unknown_date INTEGER NOT NULL, bad_join_date INTEGER NOT NULL, rejoined INTEGER NOT NULL,
           tier TEXT, category TEXT, join_reason TEXT, {flags},
           rs_family INTEGER NOT NULL, ns_family INTEGER NOT NULL, active_rs_students INTEGER NOT NULL,
           last_rs_year INTEGER, resign_reason_group TEXT NOT NULL)"
    )
}

fn as_bool(v: &Option<String>) -> bool {
    matches!(v.as_deref(), Some("true") | Some("1"))
}
fn as_num(v: &Option<String>) -> f64 {
    v.as_deref()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Derive one mart row from the raw Account values (positional per REQUIRED_COLUMNS).
fn derive(raw: &[Option<String>; 16]) -> Hh {
    let [id, name, _ty, is_member, is_resigned, join, orig, resign, tier, category, reason, resign_reason, former_rs, active_rs, ever_ns, last_rs] =
        raw;
    let is_current = as_bool(is_member);
    let is_resigned = as_bool(is_resigned);
    let join_fy = join.as_deref().and_then(fy_of);
    let bad_join_date = join.is_some() && join_fy.is_none();
    let orig_fy = orig.as_deref().and_then(fy_of);
    let cohort_fy = orig_fy.or(join_fy);
    let resign_fy_raw = resign.as_deref().and_then(fy_of);
    // A current member's LastResignDate is a past spell, not a resignation.
    let resign_fy = if is_current { None } else { resign_fy_raw };
    let resigned_unknown_date = !is_current && is_resigned && resign_fy.is_none();
    // ISO dates compare lexically, so string order is date order.
    let rejoined = matches!((orig.as_deref(), join.as_deref()), (Some(o), Some(j)) if o < j);
    Hh {
        account_id: id.clone().unwrap_or_default(),
        name: name.clone(),
        is_current,
        is_resigned,
        join_fy,
        cohort_fy,
        resign_fy,
        resigned_unknown_date,
        bad_join_date,
        rejoined,
        tier: tier.clone(),
        category: category.clone(),
        join_reason: reason.clone().filter(|s| !s.trim().is_empty()),
        ch: channel_flags(reason.as_deref()),
        rs_family: as_num(former_rs) > 0.0 || as_num(active_rs) > 0.0,
        ns_family: as_bool(ever_ns),
        active_rs_students: as_num(active_rs) as i64,
        last_rs_year: parse_rs_year(last_rs.as_deref()),
        resign_reason_group: reason_group(resign_reason.as_deref()).to_string(),
    }
}

/// Rebuild `_m_household` from the Account mirror. One transaction; drop + create + insert.
pub fn rebuild(store: &mut Store) -> Result<RebuildInfo> {
    let present = store.mirror_columns("Account")?;
    if present.is_empty() {
        anyhow::bail!("Account is not synced; sync it before building insights");
    }
    let have = |c: &str| present.iter().any(|p| p == c);
    for mandatory in ["Type", "IsATempleMember__c"] {
        if !have(mandatory) {
            anyhow::bail!("Account mirror is missing {mandatory}, which insights require");
        }
    }
    let unavailable: Vec<String> = REQUIRED_COLUMNS
        .iter()
        .filter(|c| !have(c))
        .map(|c| c.to_string())
        .collect();

    // SELECT every required column, substituting NULL for absent ones, so `derive`
    // always sees the same positional shape. Identifiers are validated by `ident`.
    let select_list = REQUIRED_COLUMNS
        .iter()
        .map(|c| {
            if have(c) {
                ident(c)
            } else {
                Ok(format!("NULL AS {}", ident(c)?))
            }
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let sql = format!("SELECT {select_list} FROM \"Account\" WHERE \"Type\" = 'Member Family'");

    let rows: Vec<Hh> = {
        let mut st = store.conn().prepare(&sql)?;
        let it = st.query_map([], |r| {
            let mut raw: [Option<String>; 16] = Default::default();
            for (i, slot) in raw.iter_mut().enumerate() {
                *slot = r.get::<_, Option<String>>(i)?;
            }
            Ok(derive(&raw))
        })?;
        it.collect::<std::result::Result<_, _>>()?
    };

    let tx = store.conn_mut().transaction()?;
    tx.execute_batch(&format!("DROP TABLE IF EXISTS {MART}"))?;
    tx.execute_batch(&mart_ddl())?;
    {
        let flag_cols = CHANNELS
            .iter()
            .map(|(k, _)| format!("ch_{k}"))
            .collect::<Vec<_>>()
            .join(", ");
        let flag_marks = vec!["?"; 12].join(", ");
        let mut st = tx.prepare(&format!(
            "INSERT INTO {MART}(account_id, name, is_current, is_resigned, join_fy, cohort_fy, resign_fy,
               resigned_unknown_date, bad_join_date, rejoined, tier, category, join_reason, {flag_cols},
               rs_family, ns_family, active_rs_students, last_rs_year, resign_reason_group)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,{flag_marks},?,?,?,?,?)"
        ))?;
        for h in &rows {
            let mut vals: Vec<rusqlite::types::Value> = vec![
                h.account_id.clone().into(),
                h.name.clone().into(),
                (h.is_current as i64).into(),
                (h.is_resigned as i64).into(),
                h.join_fy.into(),
                h.cohort_fy.into(),
                h.resign_fy.into(),
                (h.resigned_unknown_date as i64).into(),
                (h.bad_join_date as i64).into(),
                (h.rejoined as i64).into(),
                h.tier.clone().into(),
                h.category.clone().into(),
                h.join_reason.clone().into(),
            ];
            vals.extend(h.ch.iter().map(|b| rusqlite::types::Value::from(*b as i64)));
            vals.extend([
                (h.rs_family as i64).into(),
                (h.ns_family as i64).into(),
                h.active_rs_students.into(),
                h.last_rs_year.into(),
                h.resign_reason_group.clone().into(),
            ]);
            st.execute(rusqlite::params_from_iter(vals.iter()))?;
        }
    }
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    tx.execute(
        "INSERT INTO _meta(key, value) VALUES('insights_built_at', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![now],
    )?;
    tx.execute(
        "INSERT INTO _meta(key, value) VALUES('insights_unavailable', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![serde_json::to_string(&unavailable)?],
    )?;
    tx.commit()?;
    Ok(RebuildInfo {
        households: rows.len(),
        unavailable,
    })
}

/// Read the mart back into memory (the views are computed from this).
pub fn load(store: &Store) -> Result<Vec<Hh>> {
    let flag_cols = CHANNELS
        .iter()
        .map(|(k, _)| format!("ch_{k}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut st = store.conn().prepare(&format!(
        "SELECT account_id, name, is_current, is_resigned, join_fy, cohort_fy, resign_fy,
                resigned_unknown_date, bad_join_date, rejoined, tier, category, join_reason, {flag_cols},
                rs_family, ns_family, active_rs_students, last_rs_year, resign_reason_group
         FROM {MART}"
    ))?;
    let rows = st.query_map([], |r| {
        let mut ch = [false; 12];
        for (i, f) in ch.iter_mut().enumerate() {
            *f = r.get::<_, i64>(13 + i)? != 0;
        }
        Ok(Hh {
            account_id: r.get(0)?,
            name: r.get(1)?,
            is_current: r.get::<_, i64>(2)? != 0,
            is_resigned: r.get::<_, i64>(3)? != 0,
            join_fy: r.get(4)?,
            cohort_fy: r.get(5)?,
            resign_fy: r.get(6)?,
            resigned_unknown_date: r.get::<_, i64>(7)? != 0,
            bad_join_date: r.get::<_, i64>(8)? != 0,
            rejoined: r.get::<_, i64>(9)? != 0,
            tier: r.get(10)?,
            category: r.get(11)?,
            join_reason: r.get(12)?,
            ch,
            rs_family: r.get::<_, i64>(25)? != 0,
            ns_family: r.get::<_, i64>(26)? != 0,
            active_rs_students: r.get(27)?,
            last_rs_year: r.get(28)?,
            resign_reason_group: r.get(29)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiscal_year_starts_june_first_and_is_labeled_by_end_year() {
        assert_eq!(fy_from_ymd(2024, 5), 2024); // May 2024 -> FY2024 (Jun 2023 – May 2024)
        assert_eq!(fy_from_ymd(2024, 6), 2025); // Jun 2024 -> FY2025
        assert_eq!(fy_of("2024-05-31"), Some(2024));
        assert_eq!(fy_of("2024-06-01"), Some(2025));
        assert_eq!(fy_of("2001-03-28T00:00:00Z"), Some(2001));
    }

    #[test]
    fn fy_of_rejects_placeholder_and_garbage_dates() {
        assert_eq!(fy_of("2199-06-02"), None);
        assert_eq!(fy_of("2991-01-01"), None);
        assert_eq!(fy_of("1899-12-31"), None);
        assert_eq!(fy_of(""), None);
        assert_eq!(fy_of("not a date"), None);
        assert_eq!(fy_of("2024-13-01"), None);
    }

    #[test]
    fn channel_flags_split_the_multipicklist_case_insensitively() {
        let f = channel_flags(Some(
            "Nursery School and Religious School;To be with Family",
        ));
        let idx = |k: &str| CHANNELS.iter().position(|(key, _)| *key == k).unwrap();
        assert!(f[idx("religious_school")]);
        assert!(f[idx("nursery_school")]);
        assert!(f[idx("family")]);
        assert!(!f[idx("clergy")]);
        assert_eq!(channel_flags(None), [false; 12]);
        assert!(channel_flags(Some("high holy day tickets"))[idx("hhd_tickets")]);
    }

    #[test]
    fn reason_group_buckets_in_priority_order() {
        assert_eq!(reason_group(Some("Moved;No Longer Engaged")), "Moved");
        assert_eq!(reason_group(Some("Non-payment")), "Non-payment");
        assert_eq!(
            reason_group(Some("CJM / AM Aged Out")),
            "Young-adult tier aged out"
        );
        assert_eq!(
            reason_group(Some("Joined Another Synagogue")),
            "Joined another synagogue"
        );
        assert_eq!(reason_group(Some("Elderly / Ill")), "Elderly / ill");
        assert_eq!(reason_group(Some("Something new")), "Other");
        assert_eq!(reason_group(Some("")), "(not coded)");
        assert_eq!(reason_group(None), "(not coded)");
    }

    #[test]
    fn parse_rs_year_takes_the_end_year_of_a_school_year() {
        assert_eq!(parse_rs_year(Some("2025-2026")), Some(2026));
        assert_eq!(parse_rs_year(Some("2007")), Some(2007));
        assert_eq!(parse_rs_year(Some("")), None);
        assert_eq!(parse_rs_year(None), None);
    }

    use crate::salesforce::Row;
    use crate::store::{open, Store};

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    pub(super) fn mem() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = open(&dir.path().join("t.db"), KEY).unwrap();
        (dir, s)
    }

    /// Column order for the synthetic Account mirror used by these tests.
    pub(super) const ACCT_COLS: [&str; 16] = [
        "Id",
        "Name",
        "Type",
        "IsATempleMember__c",
        "IsResigned__c",
        "Join_Date__c",
        "OriginalJoinDate__c",
        "LastResignDate__c",
        "Sub_Type__c",
        "Member_Category__c",
        "Join_Reason__c",
        "Resign_Reason__c",
        "FormerReligiousSchoolStudents__c",
        "ActiveReligiousSchoolStudents__c",
        "WasEverNSAffiliated__c",
        "LastYearAttendedRS__c",
    ];

    /// Build one Account row. `vals` is positional per ACCT_COLS; "" means NULL.
    pub(super) fn acct(vals: [&str; 16]) -> Row {
        let mut m = Row::new();
        for (c, v) in ACCT_COLS.iter().zip(vals.iter()) {
            if !v.is_empty() {
                m.insert((*c).into(), serde_json::Value::String((*v).into()));
            }
        }
        m
    }

    pub(super) fn seed_account(s: &mut Store, rows: &[Row], cols: &[&str]) {
        s.upsert_object("Account", "Account", rows.len() as i64)
            .unwrap();
        for c in cols {
            s.upsert_field("Account", c, "string", c, false).unwrap();
        }
        let cols: Vec<String> = cols.iter().map(|c| c.to_string()).collect();
        s.replace_mirror("Account", &cols, rows).unwrap();
    }

    fn fixture() -> Vec<Row> {
        vec![
            // current voting member, joined FY2015 (Sept 2014), RS family, reason RS
            acct([
                "001A",
                "Cohen",
                "Member Family",
                "true",
                "false",
                "2014-09-01",
                "2014-09-01",
                "",
                "Voting Member",
                "MAIN",
                "Religious School",
                "",
                "2",
                "0",
                "false",
                "2023-2024",
            ]),
            // resigned FY2020 (Aug 2019), joined FY2015, reason Nursery+RS, non-payment
            acct([
                "001B",
                "Levy",
                "Member Family",
                "false",
                "true",
                "2014-08-15",
                "2014-08-15",
                "2019-08-01",
                "Voting Member",
                "MAIN",
                "Nursery School and Religious School",
                "Non-payment",
                "1",
                "0",
                "true",
                "2019-2020",
            ]),
            // rejoiner: original 2005, left 2010, rejoined FY2023 (Jul 2022), current
            acct([
                "001C",
                "Adler",
                "Member Family",
                "true",
                "false",
                "2022-07-10",
                "2005-01-01",
                "2010-05-31",
                "Young Professionals",
                "Young Professionals Introductory Membership",
                "Community;Young Professionals",
                "",
                "0",
                "0",
                "false",
                "",
            ]),
            // resigned, date unknown, joined FY2018
            acct([
                "001D",
                "Roth",
                "Member Family",
                "false",
                "true",
                "2017-10-01",
                "2017-10-01",
                "",
                "Voting Member",
                "MAIN",
                "",
                "Moved",
                "0",
                "0",
                "false",
                "",
            ]),
            // placeholder join date -> bad_join_date
            acct([
                "001E",
                "Katz",
                "Member Family",
                "true",
                "false",
                "2199-06-02",
                "2199-06-02",
                "",
                "Voting Member",
                "MAIN",
                "",
                "",
                "0",
                "0",
                "false",
                "",
            ]),
            // not a member family at all -> excluded
            acct([
                "001F",
                "Some Vendor",
                "Vendor",
                "false",
                "false",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
            ]),
            // current, joined last FY (FY2026 = Jun 2025) via nursery school only
            acct([
                "001G",
                "Green",
                "Member Family",
                "true",
                "false",
                "2025-07-01",
                "2025-07-01",
                "",
                "Voting Member",
                "MAIN",
                "Nursery School",
                "",
                "0",
                "0",
                "true",
                "",
            ]),
        ]
    }

    #[test]
    fn rebuild_derives_the_mart_and_reports_unavailable_columns() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        let info = rebuild(&mut s).unwrap();
        assert_eq!(info.households, 6, "vendor row excluded");
        assert!(info.unavailable.is_empty());
        let hh = load(&s).unwrap();
        let by = |id: &str| hh.iter().find(|h| h.account_id == id).unwrap().clone();

        let a = by("001A");
        assert!(a.is_current && !a.is_resigned);
        assert_eq!(
            (a.join_fy, a.cohort_fy, a.resign_fy),
            (Some(2015), Some(2015), None)
        );
        assert!(a.ch[0], "religious_school flag");
        assert!(a.rs_family && !a.ns_family);
        assert_eq!(a.last_rs_year, Some(2024));
        assert_eq!(a.resign_reason_group, "(not coded)");

        let b = by("001B");
        assert_eq!(b.resign_fy, Some(2020));
        assert!(b.ch[0] && b.ch[1]);
        assert_eq!(b.resign_reason_group, "Non-payment");
        assert!(b.ns_family);

        let c = by("001C");
        assert!(c.rejoined, "original join before latest join");
        assert_eq!(
            c.resign_fy, None,
            "a current member's old resign date is not a resignation"
        );
        assert_eq!(c.cohort_fy, Some(2005));
        assert_eq!(c.join_fy, Some(2023));

        let d = by("001D");
        assert!(d.resigned_unknown_date);
        assert_eq!(d.resign_fy, None);

        let e = by("001E");
        assert!(e.bad_join_date);
        assert_eq!(e.join_fy, None);

        assert!(s.get_meta("insights_built_at").unwrap().is_some());
        assert!(s.table_exists(MART).unwrap());
    }

    #[test]
    fn rebuild_without_join_reason_column_marks_channels_unavailable() {
        let (_d, mut s) = mem();
        let cols: Vec<&str> = ACCT_COLS
            .iter()
            .copied()
            .filter(|c| *c != "Join_Reason__c")
            .collect();
        let rows: Vec<Row> = fixture()
            .into_iter()
            .map(|mut r| {
                r.remove("Join_Reason__c");
                r
            })
            .collect();
        seed_account(&mut s, &rows, &cols);
        let info = rebuild(&mut s).unwrap();
        assert_eq!(info.unavailable, vec!["Join_Reason__c".to_string()]);
        assert!(load(&s)
            .unwrap()
            .iter()
            .all(|h| h.ch == [false; 12] && h.join_reason.is_none()));
    }

    #[test]
    fn rebuild_fails_cleanly_when_account_is_not_synced() {
        let (_d, mut s) = mem();
        let err = rebuild(&mut s).unwrap_err().to_string();
        assert!(err.contains("Account"), "{err}");
    }

    #[test]
    fn purge_drops_the_mart() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        rebuild(&mut s).unwrap();
        s.purge_mirror().unwrap();
        assert!(!s.table_exists(MART).unwrap());
    }
}
