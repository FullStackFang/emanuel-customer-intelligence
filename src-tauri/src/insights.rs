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
    let allowed = store.allowed_fields("Account")?;
    let have = |c: &str| present.iter().any(|p| p == c) && allowed.contains(c);
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

// ── views ───────────────────────────────────────────────────────────────────

const FIRST_TREND_FY: i32 = 2005;
const FIRST_COHORT_FY: i32 = 2010;
const MAX_K: i32 = 8;
const CHANNEL_MIN_N: usize = 20;

#[derive(Serialize, Debug, Clone)]
pub struct TrendRow {
    pub fy: i32,
    pub joins: i64,
    pub resigns: i64,
    pub active_end_of_fy: i64,
}
#[derive(Serialize, Debug, Clone)]
pub struct CohortYear1 {
    pub cohort: i32,
    pub n: i64,
    pub pct_retained: f64,
}
#[derive(Serialize, Debug, Clone)]
pub struct CohortCell {
    pub cohort: i32,
    pub n: i64,
    pub k: i32,
    pub pct_retained: f64,
}
#[derive(Serialize, Debug, Clone)]
pub struct ChannelRow {
    pub key: String,
    pub label: String,
    pub n: i64,
    pub still_members: i64,
    pub pct: f64,
    pub avg_tenure: f64,
    pub left_within_2y: i64,
}
#[derive(Serialize, Debug, Clone)]
pub struct SchoolRow {
    pub group: String,
    pub n: i64,
    pub still_members: i64,
    pub pct: f64,
}
#[derive(Serialize, Debug, Clone)]
pub struct ReasonCell {
    pub fy: i32,
    pub reason: String,
    pub n: i64,
}
#[derive(Serialize, Debug, Clone)]
pub struct Kpis {
    pub members_now: i64,
    pub net_vs_prior_fy: i64,
    pub joins_this_fy: i64,
    pub resigns_this_fy: i64,
    pub year1_cohort: i32,
    pub year1_pct: f64,
    pub year1_baseline_pct: f64,
    pub at_risk_count: i64,
}
#[derive(Serialize, Debug, Clone)]
pub struct AtRiskRow {
    pub account_id: String,
    pub name: String,
    pub tier: Option<String>,
    pub join_fy: Option<i32>,
    pub rules: Vec<String>,
}
#[derive(Serialize, Debug, Clone)]
pub struct Insights {
    pub built_at: Option<String>,
    pub current_fy: i32,
    pub unavailable: Vec<String>,
    pub kpis: Kpis,
    pub trend: Vec<TrendRow>,
    pub year1: Vec<CohortYear1>,
    pub cohort_matrix: Vec<CohortCell>,
    pub channels: Vec<ChannelRow>,
    pub school: Vec<SchoolRow>,
    pub reasons: Vec<ReasonCell>,
}

/// Spell rule: a member in `fy` if joined by then and not resigned before/in it.
/// A resigned household with no resign date counts only in its join year.
pub fn member_in(h: &Hh, fy: i32) -> bool {
    let Some(j) = h.join_fy else { return false };
    if j > fy {
        return false;
    }
    if h.resigned_unknown_date {
        return fy == j;
    }
    match h.resign_fy {
        Some(r) => r > fy,
        None => true,
    }
}

fn pct(num: i64, den: i64) -> f64 {
    if den == 0 {
        0.0
    } else {
        (1000.0 * num as f64 / den as f64).round() / 10.0
    }
}

pub fn trend(hh: &[Hh], cur: i32) -> Vec<TrendRow> {
    (FIRST_TREND_FY..=cur)
        .map(|fy| TrendRow {
            fy,
            joins: hh.iter().filter(|h| h.join_fy == Some(fy)).count() as i64,
            resigns: hh.iter().filter(|h| h.resign_fy == Some(fy)).count() as i64,
            active_end_of_fy: hh.iter().filter(|h| member_in(h, fy)).count() as i64,
        })
        .collect()
}

pub fn year1(hh: &[Hh], cur: i32) -> Vec<CohortYear1> {
    (FIRST_COHORT_FY..cur)
        .filter_map(|c| {
            let cohort: Vec<&Hh> = hh.iter().filter(|h| h.join_fy == Some(c)).collect();
            if cohort.is_empty() {
                return None;
            }
            let kept = cohort.iter().filter(|h| member_in(h, c + 1)).count() as i64;
            Some(CohortYear1 {
                cohort: c,
                n: cohort.len() as i64,
                pct_retained: pct(kept, cohort.len() as i64),
            })
        })
        .collect()
}

pub fn cohort_matrix(hh: &[Hh], cur: i32) -> Vec<CohortCell> {
    let mut out = Vec::new();
    for c in FIRST_COHORT_FY..cur {
        let cohort: Vec<&Hh> = hh.iter().filter(|h| h.join_fy == Some(c)).collect();
        if cohort.is_empty() {
            continue;
        }
        for k in 1..=MAX_K {
            if c + k > cur {
                break;
            }
            let kept = cohort.iter().filter(|h| member_in(h, c + k)).count() as i64;
            out.push(CohortCell {
                cohort: c,
                n: cohort.len() as i64,
                k,
                pct_retained: pct(kept, cohort.len() as i64),
            });
        }
    }
    out
}

/// Joiners old enough to judge: at least four full membership years, at most twelve.
fn judgeable(h: &Hh, cur: i32) -> bool {
    matches!(h.join_fy, Some(j) if j >= cur - 12 && j <= cur - 4)
}

fn tenure_years(h: &Hh, cur: i32) -> f64 {
    let j = h.join_fy.unwrap_or(cur) as f64;
    if h.is_current {
        cur as f64 - j
    } else if h.resigned_unknown_date {
        1.0
    } else {
        h.resign_fy.map(|r| r as f64 - j).unwrap_or(1.0)
    }
}

pub fn channels(hh: &[Hh], cur: i32) -> Vec<ChannelRow> {
    let base: Vec<&Hh> = hh
        .iter()
        .filter(|h| judgeable(h, cur) && h.join_reason.is_some())
        .collect();
    let mut out: Vec<ChannelRow> = CHANNELS
        .iter()
        .enumerate()
        .filter_map(|(i, (key, _))| {
            let members: Vec<&&Hh> = base.iter().filter(|h| h.ch[i]).collect();
            if members.len() < CHANNEL_MIN_N {
                return None;
            }
            let n = members.len() as i64;
            let still = members.iter().filter(|h| h.is_current).count() as i64;
            let tenure: f64 = members.iter().map(|h| tenure_years(h, cur)).sum::<f64>() / n as f64;
            let left2 = members
                .iter()
                .filter(|h| !h.is_current && tenure_years(h, cur) <= 2.0)
                .count() as i64;
            Some(ChannelRow {
                key: key.to_string(),
                label: channel_label(key),
                n,
                still_members: still,
                pct: pct(still, n),
                avg_tenure: (tenure * 10.0).round() / 10.0,
                left_within_2y: left2,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.pct
            .partial_cmp(&a.pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

fn channel_label(key: &str) -> String {
    match key {
        "religious_school" => "Religious school",
        "nursery_school" => "Nursery school",
        "affiliation" => "Affiliation",
        "life_cycle" => "Life cycle event",
        "family" => "To be with family",
        "young_professionals" => "Young professionals",
        "community" => "Community",
        "hhd_tickets" => "High Holy Day tickets",
        "streicker" => "Streicker",
        "clergy" => "Clergy",
        "worship" => "Worship services",
        "move" => "Move or relocation",
        other => other,
    }
    .to_string()
}

pub fn school(hh: &[Hh], cur: i32) -> Vec<SchoolRow> {
    const GROUPS: [&str; 4] = [
        "Both nursery and religious school",
        "Religious school family",
        "Nursery school family",
        "No school history",
    ];
    let group_of = |h: &Hh| match (h.rs_family, h.ns_family) {
        (true, true) => GROUPS[0],
        (true, false) => GROUPS[1],
        (false, true) => GROUPS[2],
        (false, false) => GROUPS[3],
    };
    let base: Vec<&Hh> = hh.iter().filter(|h| judgeable(h, cur)).collect();
    GROUPS
        .iter()
        .map(|g| {
            let m: Vec<&&Hh> = base.iter().filter(|h| group_of(h) == *g).collect();
            let n = m.len() as i64;
            let still = m.iter().filter(|h| h.is_current).count() as i64;
            SchoolRow {
                group: g.to_string(),
                n,
                still_members: still,
                pct: pct(still, n),
            }
        })
        .collect()
}

pub fn reasons(hh: &[Hh], cur: i32) -> Vec<ReasonCell> {
    let mut counts: std::collections::BTreeMap<(i32, String), i64> = Default::default();
    for h in hh.iter().filter(|h| !h.is_current && h.is_resigned) {
        if let Some(fy) = h.resign_fy {
            if fy >= cur - 5 && fy <= cur {
                *counts
                    .entry((fy, h.resign_reason_group.clone()))
                    .or_default() += 1;
            }
        }
    }
    counts
        .into_iter()
        .map(|((fy, reason), n)| ReasonCell { fy, reason, n })
        .collect()
}

pub fn kpis(hh: &[Hh], cur: i32, at_risk_count: i64) -> Kpis {
    let active = |fy: i32| hh.iter().filter(|h| member_in(h, fy)).count() as i64;
    let y1 = year1(hh, cur);
    // The current-FY cohort (cur-1) is still mid-first-year; only present a cohort
    // whose first year is fully complete as the headline "first-year retention".
    let latest = y1.iter().filter(|r| r.cohort <= cur - 2).last();
    let baseline: Vec<&CohortYear1> = y1.iter().filter(|r| r.cohort <= cur - 3).collect();
    let baseline_pct = if baseline.is_empty() {
        0.0
    } else {
        (10.0 * baseline.iter().map(|r| r.pct_retained).sum::<f64>() / baseline.len() as f64)
            .round()
            / 10.0
    };
    Kpis {
        members_now: hh.iter().filter(|h| h.is_current).count() as i64,
        net_vs_prior_fy: active(cur) - active(cur - 1),
        joins_this_fy: hh.iter().filter(|h| h.join_fy == Some(cur)).count() as i64,
        resigns_this_fy: hh.iter().filter(|h| h.resign_fy == Some(cur)).count() as i64,
        year1_cohort: latest.map(|r| r.cohort).unwrap_or(cur - 1),
        year1_pct: latest.map(|r| r.pct_retained).unwrap_or(0.0),
        year1_baseline_pct: baseline_pct,
        at_risk_count,
    }
}

// ── at-risk rules (fixed in code; tuning is a code change on purpose) ───────

const INTRO_TIERS: [&str; 3] = ["Young Adult Member", "Young Professionals", "Downtown"];

pub fn at_risk_rows(hh: &[Hh], cur: i32) -> Vec<AtRiskRow> {
    let idx = |k: &str| {
        CHANNELS
            .iter()
            .position(|(key, _)| *key == k)
            .expect("channel")
    };
    let (ns, rs) = (idx("nursery_school"), idx("religious_school"));
    let mut out: Vec<AtRiskRow> = hh
        .iter()
        .filter(|h| h.is_current)
        .filter_map(|h| {
            let mut rules = Vec::new();
            if h.join_fy == Some(cur - 1) {
                rules.push("first_year");
            }
            if matches!(h.join_fy, Some(j) if j >= cur - 2) && h.ch[ns] && !h.ch[rs] && !h.rs_family
            {
                rules.push("new_ns_only");
            }
            if h.tier
                .as_deref()
                .map_or(false, |t| INTRO_TIERS.contains(&t))
                && matches!(h.join_fy, Some(j) if cur - j >= 2)
            {
                rules.push("intro_tier_aging");
            }
            if h.rs_family
                && h.active_rs_students == 0
                && matches!(h.last_rs_year, Some(y) if y >= cur - 2 && y <= cur - 1)
            {
                rules.push("rs_ended");
            }
            if rules.is_empty() {
                return None;
            }
            Some(AtRiskRow {
                account_id: h.account_id.clone(),
                name: h.name.clone().unwrap_or_default(),
                tier: h.tier.clone(),
                join_fy: h.join_fy,
                rules: rules.into_iter().map(String::from).collect(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.rules.len().cmp(&a.rules.len()).then(a.name.cmp(&b.name)));
    out
}

pub fn at_risk(store: &Store, cur: i32) -> Result<Vec<AtRiskRow>> {
    Ok(at_risk_rows(&load(store)?, cur))
}

// ── CSV ─────────────────────────────────────────────────────────────────────

pub const VIEWS: [&str; 7] = [
    "trend",
    "year1",
    "cohort_matrix",
    "channels",
    "school",
    "reasons",
    "at_risk",
];

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv(header: &[&str], rows: Vec<Vec<String>>) -> (String, usize) {
    let mut out = header.join(",");
    out.push('\n');
    let n = rows.len();
    for r in rows {
        out.push_str(&r.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    (out, n)
}

/// Render one view as CSV text. Returns (text, number of data rows).
pub fn to_csv(view: &str, ins: &Insights, at_risk: &[AtRiskRow]) -> Result<(String, usize)> {
    let s = |v: &dyn std::fmt::Display| v.to_string();
    Ok(match view {
        "trend" => csv(
            &["fy", "joins", "resigns", "active_end_of_fy"],
            ins.trend
                .iter()
                .map(|r| vec![s(&r.fy), s(&r.joins), s(&r.resigns), s(&r.active_end_of_fy)])
                .collect(),
        ),
        "year1" => csv(
            &["cohort", "n", "pct_retained_1y"],
            ins.year1
                .iter()
                .map(|r| vec![s(&r.cohort), s(&r.n), s(&r.pct_retained)])
                .collect(),
        ),
        "cohort_matrix" => csv(
            &["cohort", "n", "years_after", "pct_retained"],
            ins.cohort_matrix
                .iter()
                .map(|r| vec![s(&r.cohort), s(&r.n), s(&r.k), s(&r.pct_retained)])
                .collect(),
        ),
        "channels" => csv(
            &[
                "channel",
                "households",
                "still_members",
                "pct",
                "avg_tenure_years",
                "left_within_2y",
            ],
            ins.channels
                .iter()
                .map(|r| {
                    vec![
                        r.label.clone(),
                        s(&r.n),
                        s(&r.still_members),
                        s(&r.pct),
                        s(&r.avg_tenure),
                        s(&r.left_within_2y),
                    ]
                })
                .collect(),
        ),
        "school" => csv(
            &["group", "households", "still_members", "pct"],
            ins.school
                .iter()
                .map(|r| vec![r.group.clone(), s(&r.n), s(&r.still_members), s(&r.pct)])
                .collect(),
        ),
        "reasons" => csv(
            &["fy", "reason", "n"],
            ins.reasons
                .iter()
                .map(|r| vec![s(&r.fy), r.reason.clone(), s(&r.n)])
                .collect(),
        ),
        "at_risk" => csv(
            &["account_id", "name", "tier", "join_fy", "rules"],
            at_risk
                .iter()
                .map(|r| {
                    vec![
                        r.account_id.clone(),
                        r.name.clone(),
                        r.tier.clone().unwrap_or_default(),
                        r.join_fy.map(|v| v.to_string()).unwrap_or_default(),
                        r.rules.join(";"),
                    ]
                })
                .collect(),
        ),
        other => anyhow::bail!("unknown insights view: {other}"),
    })
}

pub fn views(store: &Store, cur: i32) -> Result<Insights> {
    let hh = load(store)?;
    let unavailable: Vec<String> = store
        .get_meta("insights_unavailable")?
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let at_risk = at_risk_rows(&hh, cur).len() as i64;
    Ok(Insights {
        built_at: store.get_meta("insights_built_at")?,
        current_fy: cur,
        unavailable,
        kpis: kpis(&hh, cur, at_risk),
        trend: trend(&hh, cur),
        year1: year1(&hh, cur),
        cohort_matrix: cohort_matrix(&hh, cur),
        channels: channels(&hh, cur),
        school: school(&hh, cur),
        reasons: reasons(&hh, cur),
    })
}

/// True only if `path` exists and canonicalizes to a location inside `dir`.
pub fn path_is_inside(path: &std::path::Path, dir: &std::path::Path) -> bool {
    match (std::fs::canonicalize(path), std::fs::canonicalize(dir)) {
        (Ok(p), Ok(d)) => p.starts_with(&d) && p != d,
        _ => false,
    }
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
    fn rebuild_honors_withheld_fields_not_just_synced_columns() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        s.conn()
            .execute(
                "UPDATE _fields SET withheld = 1 WHERE object='Account' AND field='Join_Reason__c'",
                [],
            )
            .unwrap();
        let info = rebuild(&mut s).unwrap();
        assert_eq!(info.unavailable, vec!["Join_Reason__c".to_string()]);
        assert!(load(&s).unwrap().iter().all(|h| h.join_reason.is_none()));
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

    fn h(id: &str, current: bool, join: Option<i32>, resign: Option<i32>) -> Hh {
        Hh {
            account_id: id.into(),
            is_current: current,
            is_resigned: !current,
            join_fy: join,
            cohort_fy: join,
            resign_fy: if current { None } else { resign },
            resigned_unknown_date: !current && resign.is_none(),
            resign_reason_group: "(not coded)".into(),
            ..Default::default()
        }
    }

    #[test]
    fn membership_spell_rules() {
        let cur = h("a", true, Some(2015), None);
        assert!(member_in(&cur, 2015) && member_in(&cur, 2026));
        assert!(!member_in(&cur, 2014));
        let gone = h("b", false, Some(2015), Some(2020));
        assert!(member_in(&gone, 2019) && !member_in(&gone, 2020));
        let unknown = h("c", false, Some(2018), None);
        assert!(
            member_in(&unknown, 2018) && !member_in(&unknown, 2019),
            "unknown resign date = lost after year 1"
        );
        let nojoin = h("d", true, None, None);
        assert!(!member_in(&nojoin, 2026));
    }

    #[test]
    fn trend_counts_joins_resigns_and_active() {
        let hh = vec![
            h("a", true, Some(2020), None),
            h("b", false, Some(2020), Some(2022)),
            h("c", false, Some(2021), Some(2022)),
            h("d", true, Some(2022), None),
        ];
        let t = trend(&hh, 2023);
        let row = |fy: i32| t.iter().find(|r| r.fy == fy).unwrap().clone();
        assert_eq!(
            (
                row(2020).joins,
                row(2020).resigns,
                row(2020).active_end_of_fy
            ),
            (2, 0, 2)
        );
        assert_eq!((row(2021).joins, row(2021).active_end_of_fy), (1, 3));
        assert_eq!(
            (
                row(2022).joins,
                row(2022).resigns,
                row(2022).active_end_of_fy
            ),
            (1, 2, 2)
        );
        assert_eq!(t.first().unwrap().fy, 2005);
        assert_eq!(t.last().unwrap().fy, 2023);
    }

    #[test]
    fn year1_and_cohort_matrix() {
        let hh = vec![
            h("a", true, Some(2020), None),
            h("b", false, Some(2020), Some(2021)), // lost in year 1
            h("c", false, Some(2020), Some(2023)), // lost in year 3
            h("d", true, Some(2021), None),
        ];
        let y = year1(&hh, 2024);
        let c2020 = y.iter().find(|r| r.cohort == 2020).unwrap();
        assert_eq!((c2020.n, c2020.pct_retained), (3, 66.7));
        let m = cohort_matrix(&hh, 2024);
        let cell = |c: i32, k: i32| {
            m.iter()
                .find(|x| x.cohort == c && x.k == k)
                .unwrap()
                .pct_retained
        };
        assert_eq!(cell(2020, 1), 66.7);
        assert_eq!(cell(2020, 2), 66.7);
        assert_eq!(cell(2020, 3), 33.3);
        assert_eq!(cell(2020, 4), 33.3);
        assert!(m.iter().all(|x| x.cohort + x.k <= 2024), "no future cells");
        assert!(m.iter().any(|x| x.cohort == 2021 && x.k == 3));
        assert!(!m.iter().any(|x| x.cohort == 2021 && x.k == 4));
    }

    #[test]
    fn channels_window_flags_and_threshold() {
        let mut hh = Vec::new();
        for i in 0..25 {
            let mut x = h(&format!("rs{i}"), i % 5 != 0, Some(2016), Some(2019));
            x.join_reason = Some("Religious School".into());
            x.ch = channel_flags(x.join_reason.as_deref());
            hh.push(x);
        }
        // too recent to count (joined within 3 years)
        let mut recent = h("r", true, Some(2025), None);
        recent.join_reason = Some("Religious School".into());
        recent.ch = channel_flags(recent.join_reason.as_deref());
        hh.push(recent);
        // below the 20-household threshold
        let mut c = h("c", true, Some(2016), None);
        c.join_reason = Some("Clergy".into());
        c.ch = channel_flags(c.join_reason.as_deref());
        hh.push(c);
        let ch = channels(&hh, 2026);
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].key, "religious_school");
        assert_eq!((ch[0].n, ch[0].still_members, ch[0].pct), (25, 20, 80.0));
        assert_eq!(ch[0].left_within_2y, 0);
        assert!((ch[0].avg_tenure - (20.0 * 10.0 + 5.0 * 3.0) / 25.0).abs() < 0.01);
    }

    #[test]
    fn school_groups_and_reasons() {
        let mut a = h("a", true, Some(2016), None);
        a.rs_family = true;
        a.ns_family = true;
        let mut b = h("b", false, Some(2017), Some(2024));
        b.rs_family = true;
        b.resign_reason_group = "Moved".into();
        let c = h("c", false, Some(2018), Some(2025));
        let hh = vec![a, b, c.clone()];
        let s = school(&hh, 2026);
        let g = |name: &str| s.iter().find(|r| r.group == name).unwrap();
        assert_eq!(
            (
                g("Both nursery and religious school").n,
                g("Both nursery and religious school").pct
            ),
            (1, 100.0)
        );
        assert_eq!(
            (
                g("Religious school family").n,
                g("Religious school family").pct
            ),
            (1, 0.0)
        );
        assert_eq!(g("No school history").n, 1);
        let r = reasons(&hh, 2026);
        assert!(r
            .iter()
            .any(|x| x.fy == 2024 && x.reason == "Moved" && x.n == 1));
        assert!(r
            .iter()
            .any(|x| x.fy == 2025 && x.reason == "(not coded)" && x.n == 1));
    }

    #[test]
    fn kpis_summarize_the_latest_year() {
        // a: current, joined 2010. b: current, joined 2024. c: resigned, joined 2024,
        // resigned FY2025. d: resigned, joined 2011, resigned FY2026 (this FY, in progress).
        let hh = vec![
            h("a", true, Some(2010), None),
            h("b", true, Some(2024), None),
            h("c", false, Some(2024), Some(2025)),
            h("d", false, Some(2011), Some(2026)),
        ];
        let k = kpis(&hh, 2026, 7);
        assert_eq!(k.members_now, 2, "a, b are current");
        assert_eq!(k.joins_this_fy, 0);
        assert_eq!(k.resigns_this_fy, 1, "only d resigned in FY2026");
        assert_eq!(
            k.net_vs_prior_fy, -1,
            "active 2026={{a,b}}=2, active 2025={{a,b,d}}=3"
        );
        assert_eq!(
            (k.year1_cohort, k.year1_pct),
            (2024, 50.0),
            "the FY2026 cohort (none here) is still mid-first-year; latest complete cohort is 2024"
        );
        assert_eq!(k.at_risk_count, 7);
    }

    #[test]
    fn views_end_to_end_from_the_mart() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        rebuild(&mut s).unwrap();
        let v = views(&s, 2026).unwrap();
        assert_eq!(v.current_fy, 2026);
        assert!(v.built_at.is_some());
        assert_eq!(v.kpis.members_now, 4);
        assert!(!v.trend.is_empty());
        assert!(v.year1.iter().any(|r| r.cohort == 2015 && r.n == 2));
    }

    #[test]
    fn at_risk_rules_fire_with_reasons() {
        let cur = 2026;
        let mut ns_only = h("ns", true, Some(2025), None);
        ns_only.name = Some("NS Only".into());
        ns_only.ch = channel_flags(Some("Nursery School"));
        let mut intro = h("yp", true, Some(2023), None);
        intro.name = Some("Young Pro".into());
        intro.tier = Some("Young Professionals".into());
        let mut rs_done = h("rs", true, Some(2012), None);
        rs_done.name = Some("RS Done".into());
        rs_done.rs_family = true;
        rs_done.active_rs_students = 0;
        rs_done.last_rs_year = Some(2025);
        let mut safe = h("ok", true, Some(2012), None);
        safe.name = Some("Safe".into());
        let mut gone = h("gone", false, Some(2025), Some(2026));
        gone.name = Some("Gone".into());
        let rows = at_risk_rows(&[ns_only, intro, rs_done, safe, gone], cur);
        let get = |id: &str| {
            rows.iter()
                .find(|r| r.account_id == id)
                .map(|r| r.rules.clone())
        };
        assert_eq!(
            get("ns"),
            Some(vec!["first_year".to_string(), "new_ns_only".to_string()])
        );
        assert_eq!(get("yp"), Some(vec!["intro_tier_aging".to_string()]));
        assert_eq!(get("rs"), Some(vec!["rs_ended".to_string()]));
        assert_eq!(get("ok"), None);
        assert_eq!(get("gone"), None, "only current members can be at risk");
        assert_eq!(rows[0].account_id, "ns", "most rules first");
    }

    #[test]
    fn csv_renders_every_view_with_a_header() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        rebuild(&mut s).unwrap();
        let ins = views(&s, 2026).unwrap();
        let ar = at_risk(&s, 2026).unwrap();
        for v in VIEWS {
            let (text, n) = to_csv(v, &ins, &ar).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            assert!(lines.len() >= 1, "{v} has a header");
            assert_eq!(lines.len() - 1, n, "{v} row count matches");
        }
        let (t, _) = to_csv("trend", &ins, &ar).unwrap();
        assert!(t.starts_with("fy,joins,resigns,active_end_of_fy\n"));
        assert!(to_csv("nope", &ins, &ar).is_err());
        let (t, n) = to_csv("at_risk", &ins, &ar).unwrap();
        assert!(
            n >= 1 && t.contains("Green"),
            "NS-only recent joiner is at risk"
        );
    }

    #[test]
    fn export_path_guard_only_accepts_files_inside_the_exports_dir() {
        let dir = tempfile::tempdir().unwrap();
        let exports = dir.path().join("exports");
        std::fs::create_dir_all(&exports).unwrap();
        let ok = exports.join("insights-trend-20260825-1200.csv");
        std::fs::write(&ok, "x").unwrap();
        assert!(path_is_inside(&ok, &exports));
        let outside = dir.path().join("mirror.db");
        std::fs::write(&outside, "x").unwrap();
        assert!(!path_is_inside(&outside, &exports));
        let sneaky = exports.join("..").join("mirror.db");
        assert!(!path_is_inside(&sneaky, &exports));
        assert!(
            !path_is_inside(&exports.join("missing.csv"), &exports),
            "must exist"
        );
    }
}
