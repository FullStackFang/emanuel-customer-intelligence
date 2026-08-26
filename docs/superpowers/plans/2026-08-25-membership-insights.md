# Membership Insights Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A live, in-app "Insights" page that turns the local encrypted mirror into membership answers — trend, cohort retention, join-channel stickiness, school-history stickiness, resignation reasons, and an audited at-risk list — rebuilt automatically after every sync.

**Architecture:** A new Rust module `insights.rs` builds one narrow household "mart" table (`_m_household`) from the synced `Account` mirror after profiling, then computes every view in memory from that table (13k rows; milliseconds). Three new audited `#[tauri::command]`s expose the bundle, the at-risk list, and CSV export; a fourth reveals an export in Explorer through the Rust-side opener with a path guard. The React `InsightsPage` renders the bundle with Recharts (bundled, offline) and a CSS-grid heatmap, using the existing design system.

**Tech Stack:** Rust (rusqlite/SQLCipher, chrono, serde) · Tauri 2 · React 19 + TypeScript · Recharts 3 · Vitest · existing Emanuel design system.

**Spec:** `docs/superpowers/specs/2026-08-25-membership-insights-design.md` — read it first; this plan implements it section by section. It builds on `docs/superpowers/specs/2026-08-25-customer-intelligence-v1-design.md`.

## Global Constraints

- Project root: `C:\Users\Stephen.Fang\OneDrive\Documents\workspace\github.com\fullstackfang\emanuel-customer-intelligence` (branch `main`; create a feature branch `feat/insights` first — see Task 1 Step 0). All paths are relative to it.
- Run cargo from `src-tauri/`. The machine-local `.cargo/config.toml` (gitignored) already sets `target-dir = C:/ct` and points the vendored-OpenSSL build at Strawberry Perl; if a cargo command fails in `openssl-sys`, prefix it with `OPENSSL_SRC_PERL="C:/Strawberry/perl/bin/perl.exe" CARGO_TARGET_DIR="C:/ct"`. cargo 1.97 accepts ONE test-name filter per invocation. Benign `LNK4099` linker warnings (vendored OpenSSL PDB) are expected; your own code must be warning-free.
- Do NOT run `npm run tauri dev` in an agent session (blocking GUI). Verify Rust with `cargo test`/`cargo build`, frontend with `npx tsc --noEmit`, `npx vitest run`, `npm run build`.
- Webview capability stays `core:default` only. No fs/shell/http/opener JS plugin permissions. Charts are bundled via npm, never fetched.
- Insights read only synced, non-withheld columns; a missing column makes a view *unavailable* with a reason, never an error or a guess.
- All insights SQL is static text in Rust; the only parameters are integers/dates bound with `params!`. No user string reaches an insights query.
- `_audit` is insert-only. Aggregate views are not audited; `insights.rebuild`, `insights.at_risk`, `insights.export` are.
- Fiscal year: **June 1 – May 31, labeled by the calendar year in which it ends**; `FY_START_MONTH = 6` is the single constant.
- Never hold the store `Mutex` across an `.await`. Commands never return tokens.
- UI copy: Title Case nav/page titles, sentence case elsewhere, no emoji, no gold text under 18px, `--font-mono` for API names only. Text never wears a series color.
- Chart palette (validated with `dataviz/scripts/validate_palette.js`, light mode, white surface): series order `#3b6eb8, #d97706, #0284c7, #dc2626, #059669, #ca8a04` (= `--color-primary-500, --color-warning-500, --color-info-500, --color-error-500, --color-success-500, --color-accent-600`); de-emphasis/"Other" `#a8a29e` (`--color-neutral-400`); sequential heatmap ramp `--color-primary-100…700`. Do not reorder or substitute without re-running the validator.
- Commit after every task with a conventional-commit message (`cargo fmt` first for Rust). Stage only the files the task touches — never `git add -A` (the tree contains untracked tooling dirs). Do not commit `.env`.

---

## File map

| Path | Responsibility |
|---|---|
| `src-tauri/src/insights.rs` | fiscal-year math, parsing/bucketing helpers, mart rebuild, in-memory views, at-risk rules, CSV rendering, tests |
| `src-tauri/src/store.rs` | + `mirror_columns()`, `table_exists()`, `newest_sync_at()`; `purge_mirror` drops the mart |
| `src-tauri/src/commands.rs` | + `get_insights`, `get_at_risk`, `export_insights_csv`, `reveal_export`; `profile_selected` triggers rebuild |
| `src-tauri/src/lib.rs` | `pub mod insights;` + command registration |
| `src/api.ts` | insight types + 4 typed wrappers |
| `src/api.test.ts` | + wrapper-name test |
| `src/pages/insights/format.ts` | pure helpers: FY label, heat color step, "so what" sentences |
| `src/pages/insights/format.test.ts` | tests for the above |
| `src/pages/insights/charts.tsx` | Recharts wrappers + CSS heatmap + palette constants + `TypedTable` |
| `src/pages/InsightsPage.tsx` | the page: KPIs, cards, at-risk list, export |
| `src/App.tsx` | fifth nav item + page switch |
| `package.json` | + `recharts` |

---

### Task 1: Fiscal-year math and parsing helpers (`insights.rs` core)

**Files:**
- Create: `src-tauri/src/insights.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod insights;`)

**Interfaces:**
- Produces: `pub const FY_START_MONTH: u32 = 6`; `pub fn fy_from_ymd(y: i32, m: u32) -> i32`; `pub fn fy_of(date: &str) -> Option<i32>` (accepts `YYYY-MM-DD…`, rejects years outside 1900..=2035); `pub fn current_fy() -> i32`; `pub const CHANNELS: [(&str, &str); 12]` (key, phrase); `pub fn channel_flags(join_reason: Option<&str>) -> [bool; 12]`; `pub fn reason_group(raw: Option<&str>) -> &'static str`; `pub fn parse_rs_year(s: Option<&str>) -> Option<i32>`.

- [ ] **Step 0: Branch**

```bash
git checkout -b feat/insights
```

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/insights.rs` containing only:

```rust
//! Membership insights: fiscal-year math, the household mart, and the views
//! the Insights page renders. Reads the mirror only; never Salesforce.

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
        let f = channel_flags(Some("Nursery School and Religious School;To be with Family"));
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
        assert_eq!(reason_group(Some("CJM / AM Aged Out")), "Young-adult tier aged out");
        assert_eq!(reason_group(Some("Joined Another Synagogue")), "Joined another synagogue");
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
}
```

Add `pub mod insights;` to `src-tauri/src/lib.rs` (alphabetical, after `pub mod config;`).

- [ ] **Step 2: Run tests to verify they fail**

Run (from `src-tauri/`): `cargo test insights:: 2>&1 | tail -8`
Expected: compile error — `fy_from_ymd`, `fy_of`, `CHANNELS`, `channel_flags`, `reason_group`, `parse_rs_year` not found.

- [ ] **Step 3: Implement**

Prepend to `src-tauri/src/insights.rs` (above the test module):

```rust
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
```

- [ ] **Step 4: Run tests**

Run: `cargo test insights:: 2>&1 | tail -12`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add src-tauri/src/insights.rs src-tauri/src/lib.rs
git commit -m "feat(insights): fiscal-year math and join-reason/resign-reason parsing"
```

---

### Task 2: Store helpers and the household mart rebuild

**Files:**
- Modify: `src-tauri/src/store.rs` (add `mirror_columns`, `table_exists`, `newest_sync_at`; `purge_mirror` drops the mart)
- Modify: `src-tauri/src/insights.rs`

**Interfaces:**
- Consumes: `store::{Store, ident}`, `Store::conn()/conn_mut()`, `Store::replace_mirror` (tests), Task 1 helpers.
- Produces: `Store::mirror_columns(&self, object) -> Result<Vec<String>>` (PRAGMA table_info; empty if table missing); `Store::table_exists(&self, name) -> Result<bool>`; `Store::newest_sync_at(&self) -> Result<Option<String>>`; `insights::MART = "_m_household"`; `insights::REQUIRED_COLUMNS`; `pub struct RebuildInfo { households: usize, unavailable: Vec<String> }` (Serialize); `pub fn rebuild(store: &mut Store) -> Result<RebuildInfo>`; `pub struct Hh {…}` + `pub fn load(store: &Store) -> Result<Vec<Hh>>`.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `src-tauri/src/insights.rs`:

```rust
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
        "Id", "Name", "Type", "IsATempleMember__c", "IsResigned__c", "Join_Date__c",
        "OriginalJoinDate__c", "LastResignDate__c", "Sub_Type__c", "Member_Category__c",
        "Join_Reason__c", "Resign_Reason__c", "FormerReligiousSchoolStudents__c",
        "ActiveReligiousSchoolStudents__c", "WasEverNSAffiliated__c", "LastYearAttendedRS__c",
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
        s.upsert_object("Account", "Account", rows.len() as i64).unwrap();
        for c in cols {
            s.upsert_field("Account", c, "string", c, false).unwrap();
        }
        let cols: Vec<String> = cols.iter().map(|c| c.to_string()).collect();
        s.replace_mirror("Account", &cols, rows).unwrap();
    }

    fn fixture() -> Vec<Row> {
        vec![
            // current voting member, joined FY2015 (Sept 2014), RS family, reason RS
            acct(["001A", "Cohen", "Member Family", "true", "false", "2014-09-01", "2014-09-01", "", "Voting Member", "MAIN", "Religious School", "", "2", "0", "false", "2023-2024"]),
            // resigned FY2020 (Aug 2019), joined FY2015, reason Nursery+RS, non-payment
            acct(["001B", "Levy", "Member Family", "false", "true", "2014-08-15", "2014-08-15", "2019-08-01", "Voting Member", "MAIN", "Nursery School and Religious School", "Non-payment", "1", "0", "true", "2019-2020"]),
            // rejoiner: original 2005, left 2010, rejoined FY2023 (Jul 2022), current
            acct(["001C", "Adler", "Member Family", "true", "false", "2022-07-10", "2005-01-01", "2010-05-31", "Young Professionals", "Young Professionals Introductory Membership", "Community;Young Professionals", "", "0", "0", "false", ""]),
            // resigned, date unknown, joined FY2018
            acct(["001D", "Roth", "Member Family", "false", "true", "2017-10-01", "2017-10-01", "", "Voting Member", "MAIN", "", "Moved", "0", "0", "false", ""]),
            // placeholder join date -> bad_join_date
            acct(["001E", "Katz", "Member Family", "true", "false", "2199-06-02", "2199-06-02", "", "Voting Member", "MAIN", "", "", "0", "0", "false", ""]),
            // not a member family at all -> excluded
            acct(["001F", "Some Vendor", "Vendor", "false", "false", "", "", "", "", "", "", "", "", "", "", ""]),
            // current, joined last FY (FY2026 = Jun 2025) via nursery school only
            acct(["001G", "Green", "Member Family", "true", "false", "2025-07-01", "2025-07-01", "", "Voting Member", "MAIN", "Nursery School", "", "0", "0", "true", ""]),
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
        assert_eq!((a.join_fy, a.cohort_fy, a.resign_fy), (Some(2015), Some(2015), None));
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
        assert_eq!(c.resign_fy, None, "a current member's old resign date is not a resignation");
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
        let cols: Vec<&str> = ACCT_COLS.iter().copied().filter(|c| *c != "Join_Reason__c").collect();
        let rows: Vec<Row> = fixture().into_iter().map(|mut r| { r.remove("Join_Reason__c"); r }).collect();
        seed_account(&mut s, &rows, &cols);
        let info = rebuild(&mut s).unwrap();
        assert_eq!(info.unavailable, vec!["Join_Reason__c".to_string()]);
        assert!(load(&s).unwrap().iter().all(|h| h.ch == [false; 12] && h.join_reason.is_none()));
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
```

Note: `Hh` must derive `Clone` for `by()`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test insights:: 2>&1 | tail -8`
Expected: compile error — `rebuild`, `load`, `MART`, `table_exists` not found.

- [ ] **Step 3: Store helpers**

In `src-tauri/src/store.rs`, inside the second `impl Store` block, after `allowed_fields`:

```rust
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
        Ok(self.conn.query_row(
            "SELECT MAX(last_synced_at) FROM _objects",
            [],
            |r| r.get(0),
        )?)
    }
```

Change `purge_mirror` so the mart goes too — replace its body with:

```rust
    pub fn purge_mirror(&mut self) -> Result<()> {
        let names = self.synced_objects()?;
        let tx = self.conn.transaction()?;
        for n in names {
            tx.execute_batch(&format!("DROP TABLE IF EXISTS {}", ident(&n)?))?;
        }
        tx.execute_batch(
            "DROP TABLE IF EXISTS _m_household;
             DELETE FROM _profile;
             DELETE FROM _meta WHERE key IN ('insights_built_at', 'insights_unavailable');
             UPDATE _objects SET last_synced_at = NULL, last_sync_rows = NULL;",
        )?;
        tx.commit()?;
        Ok(())
    }
```

- [ ] **Step 4: The mart**

In `src-tauri/src/insights.rs`, add these imports at the very top of the file (above the Task 1 code), then append the rest below the Task 1 helpers (above the test module):

```rust
use crate::store::{ident, Store};
use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

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
    v.as_deref().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
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
```

`rusqlite::types::Value` implements `From<Option<i32>>`? It implements `From<Option<T>>` where `T: Into<Value>`, and `From<i32>`, `From<i64>`, `From<String>`, `From<Option<String>>` — all used above compile with rusqlite 0.40. If `From<Option<i32>>` does not resolve, map `h.join_fy.map(|v| v as i64).into()` instead.

- [ ] **Step 5: Run tests**

Run: `cargo test insights:: 2>&1 | tail -15`
Expected: 9 passed (5 from Task 1 + 4 new). Then `cargo test store:: 2>&1 | tail -5` — all store tests still pass.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add src-tauri/src/insights.rs src-tauri/src/store.rs
git commit -m "feat(insights): household mart rebuilt from the Account mirror"
```

---

### Task 3: The views — trend, cohorts, channels, school, reasons, KPIs

**Files:**
- Modify: `src-tauri/src/insights.rs`

**Interfaces:**
- Consumes: `Hh`, `load`, `current_fy`.
- Produces (all `Serialize`): `TrendRow { fy, joins, resigns, active_end_of_fy }`, `CohortYear1 { cohort, n, pct_retained }`, `CohortCell { cohort, n, k, pct_retained }`, `ChannelRow { key, label, n, still_members, pct, avg_tenure, left_within_2y }`, `SchoolRow { group, n, still_members, pct }`, `ReasonCell { fy, reason, n }`, `Kpis { members_now, net_vs_prior_fy, joins_this_fy, resigns_this_fy, year1_cohort, year1_pct, year1_baseline_pct, at_risk_count }`, `Insights { built_at, current_fy, unavailable, kpis, trend, year1, cohort_matrix, channels, school, reasons }`; `pub fn member_in(h: &Hh, fy: i32) -> bool`; pure fns `trend(&[Hh], cur)`, `year1(&[Hh], cur)`, `cohort_matrix(&[Hh], cur)`, `channels(&[Hh], cur)`, `school(&[Hh], cur)`, `reasons(&[Hh], cur)`, `kpis(&[Hh], cur, at_risk_count)`; `pub fn views(store: &Store, cur: i32) -> Result<Insights>` (Task 4 fills `at_risk_count`; here it is computed via `at_risk_rows(&hh, cur).len()`, so Task 3 defines a stub `pub fn at_risk_rows(hh: &[Hh], cur: i32) -> Vec<AtRiskRow>` returning `Vec::new()` and the `AtRiskRow` struct, which Task 4 completes).

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests`:

```rust
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
        assert!(member_in(&unknown, 2018) && !member_in(&unknown, 2019), "unknown resign date = lost after year 1");
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
        assert_eq!((row(2020).joins, row(2020).resigns, row(2020).active_end_of_fy), (2, 0, 2));
        assert_eq!((row(2021).joins, row(2021).active_end_of_fy), (1, 3));
        assert_eq!((row(2022).joins, row(2022).resigns, row(2022).active_end_of_fy), (1, 2, 2));
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
        let cell = |c: i32, k: i32| m.iter().find(|x| x.cohort == c && x.k == k).unwrap().pct_retained;
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
        assert_eq!((g("Both nursery and religious school").n, g("Both nursery and religious school").pct), (1, 100.0));
        assert_eq!((g("Religious school family").n, g("Religious school family").pct), (1, 0.0));
        assert_eq!(g("No school history").n, 1);
        let r = reasons(&hh, 2026);
        assert!(r.iter().any(|x| x.fy == 2024 && x.reason == "Moved" && x.n == 1));
        assert!(r.iter().any(|x| x.fy == 2025 && x.reason == "(not coded)" && x.n == 1));
    }

    #[test]
    fn kpis_summarize_the_latest_year() {
        let hh = vec![
            h("a", true, Some(2010), None),
            h("b", true, Some(2025), None),
            h("c", false, Some(2025), Some(2026)),
            h("d", false, Some(2011), Some(2026)),
        ];
        let k = kpis(&hh, 2026, 7);
        assert_eq!(k.members_now, 2);
        assert_eq!(k.joins_this_fy, 0);
        assert_eq!(k.resigns_this_fy, 2);
        assert_eq!(k.net_vs_prior_fy, -2);
        assert_eq!((k.year1_cohort, k.year1_pct), (2025, 50.0));
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test insights:: 2>&1 | tail -6`
Expected: compile error — `member_in`, `trend`, `year1`, … not found.

- [ ] **Step 3: Implement the views**

Append to `src-tauri/src/insights.rs` (above the test module):

```rust
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

/// Joiners old enough to judge: at least three full membership years, at most twelve.
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
    out.sort_by(|a, b| b.pct.partial_cmp(&a.pct).unwrap_or(std::cmp::Ordering::Equal));
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
                *counts.entry((fy, h.resign_reason_group.clone())).or_default() += 1;
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
    let latest = y1.last();
    let baseline: Vec<&CohortYear1> = y1.iter().filter(|r| r.cohort <= cur - 3).collect();
    let baseline_pct = if baseline.is_empty() {
        0.0
    } else {
        (10.0 * baseline.iter().map(|r| r.pct_retained).sum::<f64>() / baseline.len() as f64).round() / 10.0
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

/// Completed in Task 4; the stub keeps `views` compiling.
pub fn at_risk_rows(_hh: &[Hh], _cur: i32) -> Vec<AtRiskRow> {
    Vec::new()
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
```

- [ ] **Step 4: Run tests**

Run: `cargo test insights:: 2>&1 | tail -20`
Expected: 16 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add src-tauri/src/insights.rs
git commit -m "feat(insights): trend, cohort retention, channel and school stickiness, reasons, KPIs"
```

---

### Task 4: At-risk rules and CSV rendering

**Files:**
- Modify: `src-tauri/src/insights.rs`

**Interfaces:**
- Consumes: `Hh`, `Insights`, view fns.
- Produces: real `at_risk_rows(hh, cur) -> Vec<AtRiskRow>` (replaces the stub); `pub fn at_risk(store, cur) -> Result<Vec<AtRiskRow>>`; `pub const VIEWS: [&str; 7]`; `pub fn to_csv(view: &str, ins: &Insights, at_risk: &[AtRiskRow]) -> Result<(String, usize)>` returning `(csv_text, data_rows)`.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests`:

```rust
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
        let get = |id: &str| rows.iter().find(|r| r.account_id == id).map(|r| r.rules.clone());
        assert_eq!(get("ns"), Some(vec!["first_year".to_string(), "new_ns_only".to_string()]));
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
        assert!(n >= 1 && t.contains("Green"), "NS-only recent joiner is at risk");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test insights::tests::at_risk_rules_fire_with_reasons 2>&1 | tail -6`
Expected: FAIL (stub returns no rows) — and `csv_renders…` fails to compile (`VIEWS`, `to_csv`, `at_risk` missing).

- [ ] **Step 3: Implement**

Replace the `at_risk_rows` stub with:

```rust
// ── at-risk rules (fixed in code; tuning is a code change on purpose) ───────

const INTRO_TIERS: [&str; 3] = ["Young Adult Member", "Young Professionals", "Downtown"];

pub fn at_risk_rows(hh: &[Hh], cur: i32) -> Vec<AtRiskRow> {
    let idx = |k: &str| CHANNELS.iter().position(|(key, _)| *key == k).expect("channel");
    let (ns, rs) = (idx("nursery_school"), idx("religious_school"));
    let mut out: Vec<AtRiskRow> = hh
        .iter()
        .filter(|h| h.is_current)
        .filter_map(|h| {
            let mut rules = Vec::new();
            if h.join_fy == Some(cur - 1) {
                rules.push("first_year");
            }
            if matches!(h.join_fy, Some(j) if j >= cur - 2) && h.ch[ns] && !h.ch[rs] && !h.rs_family {
                rules.push("new_ns_only");
            }
            if h.tier.as_deref().map_or(false, |t| INTRO_TIERS.contains(&t))
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
            ins.trend.iter().map(|r| vec![s(&r.fy), s(&r.joins), s(&r.resigns), s(&r.active_end_of_fy)]).collect(),
        ),
        "year1" => csv(
            &["cohort", "n", "pct_retained_1y"],
            ins.year1.iter().map(|r| vec![s(&r.cohort), s(&r.n), s(&r.pct_retained)]).collect(),
        ),
        "cohort_matrix" => csv(
            &["cohort", "n", "years_after", "pct_retained"],
            ins.cohort_matrix.iter().map(|r| vec![s(&r.cohort), s(&r.n), s(&r.k), s(&r.pct_retained)]).collect(),
        ),
        "channels" => csv(
            &["channel", "households", "still_members", "pct", "avg_tenure_years", "left_within_2y"],
            ins.channels
                .iter()
                .map(|r| vec![r.label.clone(), s(&r.n), s(&r.still_members), s(&r.pct), s(&r.avg_tenure), s(&r.left_within_2y)])
                .collect(),
        ),
        "school" => csv(
            &["group", "households", "still_members", "pct"],
            ins.school.iter().map(|r| vec![r.group.clone(), s(&r.n), s(&r.still_members), s(&r.pct)]).collect(),
        ),
        "reasons" => csv(
            &["fy", "reason", "n"],
            ins.reasons.iter().map(|r| vec![s(&r.fy), r.reason.clone(), s(&r.n)]).collect(),
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
```

- [ ] **Step 4: Run tests**

Run: `cargo test insights:: 2>&1 | tail -22`
Expected: 18 passed.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add src-tauri/src/insights.rs
git commit -m "feat(insights): at-risk rules and CSV rendering"
```

---

### Task 5: Commands, rebuild trigger, export/reveal with path guard

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs` (register 4 commands)
- Modify: `src-tauri/src/insights.rs` (add `exports_dir_ok` path guard + test)

**Interfaces:**
- Consumes: `insights::{rebuild, views, at_risk, to_csv, current_fy, VIEWS, MART, Insights, AtRiskRow}`, `Store::{table_exists, newest_sync_at, get_meta}`.
- Produces commands: `get_insights() -> Insights`; `get_at_risk() -> Vec<AtRiskRow>`; `export_insights_csv(view: String) -> String` (absolute path written); `reveal_export(path: String) -> ()`. Helper `insights::path_is_inside(path: &Path, dir: &Path) -> bool`.

- [ ] **Step 1: Write the failing test (path guard)**

Append inside `mod tests` in `insights.rs`:

```rust
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
        assert!(!path_is_inside(&exports.join("missing.csv"), &exports), "must exist");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test insights::tests::export_path_guard 2>&1 | tail -5` — Expected: compile error, `path_is_inside` not found.

- [ ] **Step 3: Path guard**

Append to `insights.rs` (above tests):

```rust
/// True only if `path` exists and canonicalizes to a location inside `dir`.
pub fn path_is_inside(path: &std::path::Path, dir: &std::path::Path) -> bool {
    match (std::fs::canonicalize(path), std::fs::canonicalize(dir)) {
        (Ok(p), Ok(d)) => p.starts_with(&d) && p != d,
        _ => false,
    }
}
```

Run: `cargo test insights::tests::export_path_guard 2>&1 | tail -3` — Expected: 1 passed.

- [ ] **Step 4: Commands**

In `src-tauri/src/commands.rs`:

Add to the imports: `use crate::insights::{self, AtRiskRow, Insights};`

Change `profile_selected` to rebuild after profiling:

```rust
#[tauri::command]
pub async fn profile_selected(state: State<'_, AppState>) -> CmdResult<usize> {
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        let n = profile::profile_all(s)?;
        s.audit(
            &w,
            "profile.run",
            None,
            Some(serde_json::json!({"objects": n})),
        )?;
        if s.table_exists("Account")? {
            let info = insights::rebuild(s)?;
            s.audit(
                &w,
                "insights.rebuild",
                None,
                Some(serde_json::json!({"households": info.households, "unavailable": info.unavailable})),
            )?;
        }
        Ok(n)
    })
}
```

Append a new section at the end of the file:

```rust
// ── insights ────────────────────────────────────────────────────────────────

fn exports_dir(state: &AppState) -> PathBuf {
    state
        .db_path
        .parent()
        .map(|p| p.join("exports"))
        .unwrap_or_else(|| PathBuf::from("exports"))
}

/// Rebuild the mart if it is missing or older than the newest sync.
fn ensure_fresh(s: &mut Store, w: &Who, force: bool) -> anyhow::Result<()> {
    let built = s.get_meta("insights_built_at")?;
    let newest = s.newest_sync_at()?;
    let stale = match (&built, &newest) {
        (None, _) => true,
        (Some(b), Some(n)) => n > b, // ISO-8601 strings compare chronologically
        (Some(_), None) => false,
    };
    if force || stale || !s.table_exists(insights::MART)? {
        let info = insights::rebuild(s)?;
        s.audit(
            w,
            "insights.rebuild",
            None,
            Some(serde_json::json!({"households": info.households, "unavailable": info.unavailable})),
        )?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_insights(force_rebuild: bool, state: State<'_, AppState>) -> CmdResult<Insights> {
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        ensure_fresh(s, &w, force_rebuild)?;
        insights::views(s, insights::current_fy())
    })
}

#[tauri::command]
pub async fn get_at_risk(state: State<'_, AppState>) -> CmdResult<Vec<AtRiskRow>> {
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        ensure_fresh(s, &w, false)?;
        let rows = insights::at_risk(s, insights::current_fy())?;
        s.audit(
            &w,
            "insights.at_risk",
            None,
            Some(serde_json::json!({"count": rows.len()})),
        )?;
        Ok(rows)
    })
}

#[tauri::command]
pub async fn export_insights_csv(view: String, state: State<'_, AppState>) -> CmdResult<String> {
    if !insights::VIEWS.contains(&view.as_str()) {
        return Err(format!("unknown insights view: {view}"));
    }
    let w = who(state.inner());
    let dir = exports_dir(state.inner());
    with_store(state.inner(), |s| {
        ensure_fresh(s, &w, false)?;
        let cur = insights::current_fy();
        let ins = insights::views(s, cur)?;
        let ar = if view == "at_risk" { insights::at_risk(s, cur)? } else { Vec::new() };
        let (text, rows) = insights::to_csv(&view, &ins, &ar)?;
        std::fs::create_dir_all(&dir)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M");
        let path = dir.join(format!("insights-{view}-{stamp}.csv"));
        std::fs::write(&path, text)?;
        s.audit(
            &w,
            "insights.export",
            None,
            Some(serde_json::json!({"view": view, "rows": rows})),
        )?;
        Ok(path.to_string_lossy().into_owned())
    })
}

#[tauri::command]
pub async fn reveal_export(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let dir = exports_dir(state.inner());
    let p = PathBuf::from(&path);
    if !insights::path_is_inside(&p, &dir) {
        return Err("can only reveal files inside the app's exports folder".into());
    }
    tauri_plugin_opener::reveal_item_in_dir(&p).map_err(err)
}
```

Register in `src-tauri/src/lib.rs` `generate_handler!` after `commands::purge_local_data,`:

```rust
            commands::get_insights,
            commands::get_at_risk,
            commands::export_insights_csv,
            commands::reveal_export,
```

- [ ] **Step 5: Build and run everything**

Run: `cargo build 2>&1 | grep -E "^(warning|error)" | grep -v LNK4099` — Expected: no output (no warnings from our code). Then `cargo test 2>&1 | grep "test result"` — Expected: all suites pass (48 + 19 insights tests; count may differ by one or two — all `ok`).

If `tauri_plugin_opener::reveal_item_in_dir` does not exist under that name in the installed plugin version, run `grep -rn "pub fn reveal" C:/ct/../../.cargo 2>/dev/null` — or simpler: `cargo doc -p tauri-plugin-opener --no-deps` is slow; instead check `~/.cargo/registry/src/*/tauri-plugin-opener-2*/src/lib.rs` for `reveal_item_in_dir`. It is public in tauri-plugin-opener 2.x.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/insights.rs
git commit -m "feat(insights): commands for views, audited at-risk list, CSV export with guarded reveal"
```

---

### Task 6: Frontend API layer, Recharts dependency, pure formatting helpers

**Files:**
- Modify: `src/api.ts`, `src/api.test.ts`, `package.json` (+ `package-lock.json` via npm)
- Create: `src/pages/insights/format.ts`, `src/pages/insights/format.test.ts`

**Interfaces:**
- Produces (api.ts): types `Insights, Kpis, TrendRow, CohortYear1, CohortCell, ChannelRow, SchoolRow, ReasonCell, AtRiskRow, InsightView`; `getInsights(forceRebuild = false)`, `getAtRisk()`, `exportInsightsCsv(view)`, `revealExport(path)`; `INSIGHT_VIEWS` const.
- Produces (format.ts): `fyLabel(fy) => "FY2025"`; `heatStep(pct) => 0..6` (index into the 7-step sapphire ramp, 30%→0, 90%→6); `heatInk(pct) => "#ffffff" | "var(--text-primary)"`; `soWhat(ins) => { trend, year1, cohort, channels, school, reasons }` sentences; `RULE_LABELS`.

- [ ] **Step 1: Install Recharts**

```bash
npm install recharts@^3
```
Expected: `package.json` dependencies gain `"recharts": "^3.x"`.

- [ ] **Step 2: Failing tests**

Append to `src/api.test.ts` inside the `describe`:

```ts
  it("insights wrappers use the exact command names", async () => {
    invoke.mockResolvedValue(undefined);
    await api.getInsights();
    await api.getInsights(true);
    await api.getAtRisk();
    await api.exportInsightsCsv("trend");
    await api.revealExport("C:\\x\\exports\\a.csv");
    expect(invoke.mock.calls).toEqual([
      ["get_insights", { forceRebuild: false }],
      ["get_insights", { forceRebuild: true }],
      ["get_at_risk"],
      ["export_insights_csv", { view: "trend" }],
      ["reveal_export", { path: "C:\\x\\exports\\a.csv" }],
    ]);
    expect([...api.INSIGHT_VIEWS]).toEqual(["trend", "year1", "cohort_matrix", "channels", "school", "reasons", "at_risk"]);
  });
```

Create `src/pages/insights/format.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { fyLabel, heatStep, heatInk, soWhat, RULE_LABELS } from "./format";
import type { Insights } from "../../api";

const base: Insights = {
  built_at: "2026-08-25T20:00:00Z", current_fy: 2026, unavailable: [],
  kpis: { members_now: 2490, net_vs_prior_fy: -62, joins_this_fy: 244, resigns_this_fy: 306,
    year1_cohort: 2025, year1_pct: 66.7, year1_baseline_pct: 87.4, at_risk_count: 12 },
  trend: [{ fy: 2025, joins: 321, resigns: 328, active_end_of_fy: 2552 }, { fy: 2026, joins: 244, resigns: 306, active_end_of_fy: 2490 }],
  year1: [{ cohort: 2024, n: 374, pct_retained: 69 }, { cohort: 2025, n: 321, pct_retained: 66.7 }],
  cohort_matrix: [{ cohort: 2015, n: 185, k: 5, pct_retained: 69.7 }, { cohort: 2019, n: 188, k: 5, pct_retained: 48.9 }],
  channels: [{ key: "clergy", label: "Clergy", n: 76, still_members: 49, pct: 64.5, avg_tenure: 4.7, left_within_2y: 11 },
             { key: "nursery_school", label: "Nursery school", n: 122, still_members: 32, pct: 26.2, avg_tenure: 4.6, left_within_2y: 34 }],
  school: [{ group: "Both nursery and religious school", n: 98, still_members: 67, pct: 68.4 }, { group: "No school history", n: 1115, still_members: 443, pct: 39.7 }],
  reasons: [{ fy: 2026, reason: "Non-payment", n: 90 }, { fy: 2026, reason: "Moved", n: 51 }],
};

describe("insights formatting", () => {
  it("labels fiscal years and steps the heat ramp", () => {
    expect(fyLabel(2025)).toBe("FY2025");
    expect(heatStep(0)).toBe(0);
    expect(heatStep(30)).toBe(0);
    expect(heatStep(60)).toBe(3);
    expect(heatStep(90)).toBe(6);
    expect(heatStep(100)).toBe(6);
    expect(heatInk(40)).toBe("var(--text-primary)");
    expect(heatInk(75)).toBe("#ffffff");
  });

  it("writes the so-what sentences from the numbers", () => {
    const s = soWhat(base);
    expect(s.year1).toContain("FY2025 cohort kept 66.7%");
    expect(s.year1).toContain("87.4%");
    expect(s.trend).toContain("2,490");
    expect(s.channels).toContain("Clergy");
    expect(s.channels).toContain("Nursery school");
    expect(s.school).toContain("68.4%");
    expect(s.reasons).toContain("Non-payment");
    expect(s.cohort).toContain("FY2015");
  });

  it("has a label for every at-risk rule", () => {
    expect(Object.keys(RULE_LABELS).sort()).toEqual(["first_year", "intro_tier_aging", "new_ns_only", "rs_ended"]);
  });
});
```

- [ ] **Step 3: Run to verify failure**

Run: `npx vitest run 2>&1 | tail -12` — Expected: both new tests fail (missing exports / module).

- [ ] **Step 4: Implement api.ts additions**

Append to `src/api.ts` after the `AuditRow` interface:

```ts
export interface Kpis {
  members_now: number; net_vs_prior_fy: number; joins_this_fy: number; resigns_this_fy: number;
  year1_cohort: number; year1_pct: number; year1_baseline_pct: number; at_risk_count: number;
}
export interface TrendRow { fy: number; joins: number; resigns: number; active_end_of_fy: number }
export interface CohortYear1 { cohort: number; n: number; pct_retained: number }
export interface CohortCell { cohort: number; n: number; k: number; pct_retained: number }
export interface ChannelRow { key: string; label: string; n: number; still_members: number; pct: number; avg_tenure: number; left_within_2y: number }
export interface SchoolRow { group: string; n: number; still_members: number; pct: number }
export interface ReasonCell { fy: number; reason: string; n: number }
export interface AtRiskRow { account_id: string; name: string; tier: string | null; join_fy: number | null; rules: string[] }
export interface Insights {
  built_at: string | null; current_fy: number; unavailable: string[]; kpis: Kpis;
  trend: TrendRow[]; year1: CohortYear1[]; cohort_matrix: CohortCell[];
  channels: ChannelRow[]; school: SchoolRow[]; reasons: ReasonCell[];
}
export const INSIGHT_VIEWS = ["trend", "year1", "cohort_matrix", "channels", "school", "reasons", "at_risk"] as const;
export type InsightView = (typeof INSIGHT_VIEWS)[number];
```

Append after `purgeLocalData`:

```ts
export const getInsights = (forceRebuild = false) => invoke<Insights>("get_insights", { forceRebuild });
export const getAtRisk = () => invoke<AtRiskRow[]>("get_at_risk");
export const exportInsightsCsv = (view: InsightView) => invoke<string>("export_insights_csv", { view });
export const revealExport = (path: string) => invoke<void>("reveal_export", { path });
```

- [ ] **Step 5: Implement format.ts**

Create `src/pages/insights/format.ts`:

```ts
import type { Insights } from "../../api";

export const fyLabel = (fy: number) => `FY${fy}`;
export const fmt = (n: number) => n.toLocaleString();

/** 7-step sequential ramp index for a retention percentage: 30% -> 0, 90% -> 6. */
export function heatStep(pct: number): number {
  const t = Math.max(0, Math.min(1, (pct - 30) / 60));
  return Math.round(t * 6);
}
/** Ink for a heat cell: white on the four darkest steps, primary text otherwise. */
export const heatInk = (pct: number) => (heatStep(pct) >= 4 ? "#ffffff" : "var(--text-primary)");

export const RULE_LABELS: Record<string, string> = {
  first_year: "First year",
  new_ns_only: "Nursery school only",
  intro_tier_aging: "Introductory tier aging out",
  rs_ended: "Religious school ended",
};

/** One plain sentence per card, built from the numbers so it never goes stale. */
export function soWhat(ins: Insights) {
  const last = ins.trend[ins.trend.length - 1];
  const prev = ins.trend[ins.trend.length - 2];
  const k = ins.kpis;
  const trend = last && prev
    ? `${fmt(last.active_end_of_fy)} member households at the end of ${fyLabel(last.fy)}, ${last.active_end_of_fy - prev.active_end_of_fy >= 0 ? "up" : "down"} ${fmt(Math.abs(last.active_end_of_fy - prev.active_end_of_fy))} on ${fyLabel(prev.fy)}; ${fmt(last.joins)} joined and ${fmt(last.resigns)} resigned.`
    : "Not enough history yet.";
  const year1 = `The ${fyLabel(k.year1_cohort)} cohort kept ${k.year1_pct}% of its households through the first year, against a ${k.year1_baseline_pct}% average for earlier cohorts.`;
  const five = ins.cohort_matrix.filter((c) => c.k === 5);
  const best = five.reduce<typeof five[number] | null>((a, c) => (!a || c.pct_retained > a.pct_retained ? c : a), null);
  const worst = five.reduce<typeof five[number] | null>((a, c) => (!a || c.pct_retained < a.pct_retained ? c : a), null);
  const cohort = best && worst
    ? `Five-year retention ranges from ${worst.pct_retained}% (${fyLabel(worst.cohort)} cohort) to ${best.pct_retained}% (${fyLabel(best.cohort)} cohort).`
    : "Five-year retention needs at least one cohort with five years of history.";
  const chTop = ins.channels[0];
  const chBottom = ins.channels[ins.channels.length - 1];
  const channels = chTop && chBottom
    ? `${chTop.label} joiners are the most durable (${chTop.pct}% still members); ${chBottom.label} joiners the least (${chBottom.pct}%).`
    : "Join-channel comparison needs at least 20 households per reason.";
  const schoolBest = [...ins.school].sort((a, b) => b.pct - a.pct)[0];
  const school = schoolBest
    ? `${schoolBest.group} households retain best at ${schoolBest.pct}%.`
    : "No school history available.";
  const latestFy = Math.max(...ins.reasons.map((r) => r.fy), 0);
  const topReason = ins.reasons.filter((r) => r.fy === latestFy).sort((a, b) => b.n - a.n)[0];
  const reasons = topReason
    ? `In ${fyLabel(latestFy)} the leading coded reason was ${topReason.reason} (${fmt(topReason.n)} households).`
    : "No coded resignation reasons yet.";
  return { trend, year1, cohort, channels, school, reasons };
}
```

- [ ] **Step 6: Run tests, typecheck**

Run: `npx vitest run 2>&1 | tail -8` — Expected: all pass (5 tests). Run: `npx tsc --noEmit` — Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add package.json package-lock.json src/api.ts src/api.test.ts src/pages/insights/format.ts src/pages/insights/format.test.ts
git commit -m "feat(ui): insights API layer, formatting helpers, add recharts"
```

---

### Task 7: Chart components (Recharts + CSS heatmap)

**Files:**
- Create: `src/pages/insights/charts.tsx`

**Interfaces:**
- Consumes: `api` types, `format.ts`.
- Produces: `PALETTE` constants; `TypedTable<T>`; components `TrendChart({ rows })`, `FlowsChart({ rows })`, `Year1Chart({ rows, emphasize: number[] })`, `CohortHeatmap({ cells })`, `HBarChart({ rows: { label, pct, n, still }[], emphasize?: string[] })`, `ReasonsChart({ cells })`, `TableView({ columns, rows, getRowKey })` (a `<details>` wrapper around `TypedTable`).

- [ ] **Step 1: Write the file**

Create `src/pages/insights/charts.tsx`:

```tsx
import type React from "react";
import { Fragment } from "react";
import {
  Bar, BarChart, CartesianGrid, Legend, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis,
} from "recharts";
import type { CohortCell, CohortYear1, ReasonCell, TrendRow } from "../../api";
import { Table } from "../../design-system";
import { fmt, fyLabel, heatInk, heatStep } from "./format";

/* Palette — validated with dataviz/scripts/validate_palette.js (light, white surface).
   Hex values are the design-system tokens named beside them. Do not reorder. */
export const PALETTE = {
  series: ["#3b6eb8", "#d97706", "#0284c7", "#dc2626", "#059669", "#ca8a04"], // primary-500, warning-500, info-500, error-500, success-500, accent-600
  other: "#a8a29e",   // neutral-400 — "Other" / de-emphasis, not a series hue
  emphasis: "#3b6eb8", // primary-500
  deemphasis: "#d6d3d1", // neutral-300
  ramp: ["#dae6ff", "#bdd4ff", "#90baff", "#5c94fc", "#3b6eb8", "#2d5a9e", "#1e4785"], // primary-100…700
  grid: "#e7e5e4",     // neutral-200
  ink: "#78716c",      // neutral-500 (axis text)
};

const axisTick = { fontSize: 12, fill: PALETTE.ink, fontFamily: "var(--font-body)" };
const tooltipStyle = { fontFamily: "var(--font-body)", fontSize: 12, borderRadius: 8, border: "1px solid var(--border-default)" };

// The design-system Table.jsx has no types under allowJs; retype once here.
export interface TableProps<T> {
  getRowKey: (r: T) => string;
  rows: T[];
  empty?: string;
  columns: { key: string; header: string; align?: "left" | "right" | "center"; width?: number | string; render: (r: T) => React.ReactNode }[];
}
export const TypedTable = Table as unknown as <T>(props: TableProps<T>) => React.JSX.Element;

export function TableView<T>(props: TableProps<T>) {
  return (
    <details style={{ marginTop: "var(--space-3)" }}>
      <summary style={{ cursor: "pointer", fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>Table view</summary>
      <div style={{ marginTop: "var(--space-2)" }}><TypedTable {...props} /></div>
    </details>
  );
}

export function TrendChart({ rows }: { rows: TrendRow[] }) {
  const data = rows.map((r) => ({ fy: fyLabel(r.fy), active: r.active_end_of_fy }));
  return (
    <ResponsiveContainer width="100%" height={240}>
      <LineChart data={data} margin={{ top: 8, right: 24, bottom: 0, left: 0 }}>
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} interval={3} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} domain={[2000, 3000]} tickFormatter={(v: number) => fmt(v)} width={48} />
        <Tooltip contentStyle={tooltipStyle} formatter={(v) => [fmt(Number(v)), "Member households"]} />
        <Line type="monotone" dataKey="active" stroke={PALETTE.emphasis} strokeWidth={2} dot={false} activeDot={{ r: 5, strokeWidth: 2, stroke: "#fff" }} isAnimationActive={false} />
      </LineChart>
    </ResponsiveContainer>
  );
}

export function FlowsChart({ rows }: { rows: TrendRow[] }) {
  const data = rows.map((r) => ({ fy: fyLabel(r.fy), Joins: r.joins, Resignations: r.resigns }));
  return (
    <ResponsiveContainer width="100%" height={240}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }} barGap={2} barCategoryGap="30%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} interval={3} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} width={40} />
        <Tooltip contentStyle={tooltipStyle} />
        <Legend wrapperStyle={{ fontSize: 12, fontFamily: "var(--font-body)" }} />
        <Bar dataKey="Joins" fill={PALETTE.series[0]} radius={[4, 4, 0, 0]} maxBarSize={24} isAnimationActive={false} />
        <Bar dataKey="Resignations" fill={PALETTE.series[1]} radius={[4, 4, 0, 0]} maxBarSize={24} isAnimationActive={false} />
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Single series with emphasized cohorts. Two stacked keys (one null per row) keep
    Recharts 3 happy without the deprecated <Cell>. */
export function Year1Chart({ rows, emphasize }: { rows: CohortYear1[]; emphasize: number[] }) {
  const data = rows.map((r) => ({
    fy: fyLabel(r.cohort),
    main: emphasize.includes(r.cohort) ? r.pct_retained : null,
    rest: emphasize.includes(r.cohort) ? null : r.pct_retained,
    n: r.n,
  }));
  return (
    <ResponsiveContainer width="100%" height={240}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }} barCategoryGap="35%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} domain={[0, 100]} tickFormatter={(v: number) => `${v}%`} width={44} />
        <Tooltip contentStyle={tooltipStyle} formatter={(v) => [`${v}%`, "Retained after 1 year"]} />
        <Bar dataKey="rest" stackId="a" fill={PALETTE.deemphasis} radius={[4, 4, 0, 0]} maxBarSize={24} isAnimationActive={false} />
        <Bar dataKey="main" stackId="a" fill={PALETTE.emphasis} radius={[4, 4, 0, 0]} maxBarSize={24} isAnimationActive={false} />
      </BarChart>
    </ResponsiveContainer>
  );
}

export function CohortHeatmap({ cells }: { cells: CohortCell[] }) {
  const cohorts = [...new Set(cells.map((c) => c.cohort))].sort();
  const ks = [1, 2, 3, 4, 5, 6, 7, 8];
  const n = (c: number) => cells.find((x) => x.cohort === c)?.n ?? 0;
  const at = (c: number, k: number) => cells.find((x) => x.cohort === c && x.k === k);
  const cell = { height: 30, borderRadius: "var(--radius-sm)", display: "flex", alignItems: "center", justifyContent: "center", fontSize: "var(--text-xs)", fontVariantNumeric: "tabular-nums" } as const;
  return (
    <div>
      <div style={{ display: "grid", gridTemplateColumns: `110px repeat(${ks.length}, 1fr)`, gap: 2 }}>
        <div />
        {ks.map((k) => <div key={k} style={{ textAlign: "center", fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>{k} yr{k > 1 ? "s" : ""}</div>)}
        {cohorts.map((c) => (
          <Fragment key={c}>
            <div style={{ alignSelf: "center", textAlign: "right", paddingRight: 8, fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
              {fyLabel(c)} <span style={{ color: "var(--text-tertiary)" }}>({fmt(n(c))})</span>
            </div>
            {ks.map((k) => {
              const v = at(c, k);
              return v
                ? <div key={`${c}-${k}`} title={`${fyLabel(c)} cohort · ${v.pct_retained}% still members after ${k} year${k > 1 ? "s" : ""}`}
                    style={{ ...cell, background: PALETTE.ramp[heatStep(v.pct_retained)], color: heatInk(v.pct_retained) }}>
                    {Math.round(v.pct_retained)}%
                  </div>
                : <div key={`${c}-${k}`} style={cell} />;
            })}
          </Fragment>
        ))}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 8, fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>
        <span>30%</span>
        <div style={{ width: 160, height: 8, borderRadius: 4, background: `linear-gradient(90deg, ${PALETTE.ramp[0]}, ${PALETTE.ramp[6]})` }} />
        <span>90%</span>
      </div>
    </div>
  );
}

export interface HBarRow { label: string; pct: number; n: number; still: number }

export function HBarChart({ rows, emphasize }: { rows: HBarRow[]; emphasize?: string[] }) {
  const deemph = emphasize && emphasize.length > 0;
  const data = rows.map((r) => ({
    label: r.label,
    main: !deemph || emphasize!.includes(r.label) ? r.pct : null,
    rest: deemph && !emphasize!.includes(r.label) ? r.pct : null,
    n: r.n, still: r.still,
  }));
  return (
    <ResponsiveContainer width="100%" height={28 * rows.length + 24}>
      <BarChart data={data} layout="vertical" margin={{ top: 4, right: 48, bottom: 0, left: 8 }} barCategoryGap="25%">
        <CartesianGrid horizontal={false} stroke={PALETTE.grid} />
        <XAxis type="number" domain={[0, 100]} tick={axisTick} tickLine={false} axisLine={false} tickFormatter={(v: number) => `${v}%`} />
        <YAxis type="category" dataKey="label" width={190} tick={axisTick} tickLine={false} axisLine={false} />
        <Tooltip contentStyle={tooltipStyle} formatter={(v, _name, item) => [`${v}% (${fmt(item.payload.still)} of ${fmt(item.payload.n)})`, "Still members"]} />
        <Bar dataKey="rest" stackId="a" fill={PALETTE.deemphasis} radius={[0, 4, 4, 0]} maxBarSize={18} isAnimationActive={false} />
        <Bar dataKey="main" stackId="a" fill={PALETTE.emphasis} radius={[0, 4, 4, 0]} maxBarSize={18} isAnimationActive={false} label={{ position: "right", fontSize: 12, fill: PALETTE.ink, formatter: (v: number) => `${v}%` }} />
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Fixed series order (matches PALETTE.series); everything else folds into "Other". */
export const REASON_ORDER = ["Non-payment", "Moved", "No longer engaged", "Deceased", "Young-adult tier aged out", "Joined another synagogue"];

export function ReasonsChart({ cells }: { cells: ReasonCell[] }) {
  const fys = [...new Set(cells.map((c) => c.fy))].sort();
  const data = fys.map((fy) => {
    const row: Record<string, number | string> = { fy: fyLabel(fy) };
    let other = 0;
    for (const c of cells.filter((x) => x.fy === fy)) {
      if (REASON_ORDER.includes(c.reason)) row[c.reason] = c.n; else other += c.n;
    }
    row["Other"] = other;
    return row;
  });
  return (
    <ResponsiveContainer width="100%" height={280}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }} barCategoryGap="45%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} width={40} />
        <Tooltip contentStyle={tooltipStyle} />
        <Legend wrapperStyle={{ fontSize: 12, fontFamily: "var(--font-body)" }} />
        {REASON_ORDER.map((r, i) => (
          <Bar key={r} dataKey={r} stackId="a" fill={PALETTE.series[i]} maxBarSize={24} isAnimationActive={false} stroke="#fff" strokeWidth={1} />
        ))}
        <Bar dataKey="Other" stackId="a" fill={PALETTE.other} maxBarSize={24} radius={[4, 4, 0, 0]} isAnimationActive={false} stroke="#fff" strokeWidth={1} />
      </BarChart>
    </ResponsiveContainer>
  );
}
```

- [ ] **Step 2: Typecheck and build**

Run: `npx tsc --noEmit` — Expected: clean. If Recharts' `Tooltip formatter` item typing rejects `item.payload`, type the callback param as `{ payload: HBarRow }` via `(v, _n, item: { payload: HBarRow })`; if `React.JSX.Element` is unavailable, use `JSX.Element`. Run: `npm run build 2>&1 | tail -3` — Expected: `✓ built`.

- [ ] **Step 3: Commit**

```bash
git add src/pages/insights/charts.tsx
git commit -m "feat(ui): insights chart components on the validated design-system palette"
```

---

### Task 8: The Insights page and navigation

**Files:**
- Create: `src/pages/InsightsPage.tsx`
- Modify: `src/App.tsx` (nav item, page switch, `PageKey`)

**Interfaces:**
- Consumes: `api.getInsights/getAtRisk/exportInsightsCsv/revealExport`, `charts.tsx`, `format.ts`, design system.
- Produces: `InsightsPage({ status, refresh }: PageProps)`.

- [ ] **Step 1: Write the page**

Create `src/pages/InsightsPage.tsx`:

```tsx
import type React from "react";
import { useCallback, useEffect, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Badge, Button, Card, CardHeader, CardTitle, EmptyState, Select } from "../design-system";
import { PageTitle, Stat } from "../design-system/ui-kits/grant-management/chrome.jsx";
import { CohortHeatmap, FlowsChart, HBarChart, ReasonsChart, TableView, TrendChart, Year1Chart } from "./insights/charts";
import { RULE_LABELS, fmt, fyLabel, soWhat } from "./insights/format";

function SoWhat({ text }: { text: string }) {
  return (
    <p style={{ margin: "var(--space-3) 0 0", padding: "var(--space-2) var(--space-3)", background: "var(--bg-secondary)", borderRadius: "var(--radius-md)", fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
      <span style={{ fontWeight: "var(--font-semibold)", color: "var(--text-primary)" }}>So what: </span>{text}
    </p>
  );
}

function Lede({ children }: { children: string }) {
  return <p style={{ margin: "0 0 var(--space-3)", fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>{children}</p>;
}

function Unavailable({ column }: { column: string }) {
  return <EmptyState icon="database" title="Not available" message={`This view needs ${column} to be synced and not withheld.`} action={undefined} />;
}

export default function InsightsPage({ status }: PageProps) {
  const [ins, setIns] = useState<api.Insights | null>(null);
  const [atRisk, setAtRisk] = useState<api.AtRiskRow[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [exportView, setExportView] = useState<api.InsightView>("trend");
  const [exported, setExported] = useState<string | null>(null);

  const load = useCallback(async (force = false) => {
    setBusy(force ? "rebuild" : "load"); setErr(null);
    try { setIns(await api.getInsights(force)); }
    catch (e) { setErr(String(e)); }
    finally { setBusy(null); }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const showAtRisk = async () => {
    setBusy("risk"); setErr(null);
    try { setAtRisk(await api.getAtRisk()); } catch (e) { setErr(String(e)); } finally { setBusy(null); }
  };
  const doExport = async () => {
    setBusy("export"); setErr(null); setExported(null);
    try { setExported(await api.exportInsightsCsv(exportView)); } catch (e) { setErr(String(e)); } finally { setBusy(null); }
  };

  if (status.synced_rows === 0) {
    return (
      <div>
        <PageTitle eyebrow="Customer Intelligence" title="Insights" actions={undefined} />
        <EmptyState icon="chart-line" title="Nothing synced yet" message="Select Account on the Data page and run Sync now from the Overview page. Insights are built from the local mirror after each sync." action={undefined} />
      </div>
    );
  }

  const missing = (col: string) => ins?.unavailable.includes(col) ?? false;
  const s = ins ? soWhat(ins) : null;
  const latestTwo = ins ? ins.year1.slice(-2).map((r) => r.cohort) : [];
  const built = ins?.built_at ? new Date(ins.built_at).toLocaleString() : "not built";

  return (
    <div style={{ maxWidth: 1180 }}>
      <PageTitle eyebrow="Customer Intelligence" title="Insights" actions={
        <>
          <Select value={exportView} onChange={(e: React.ChangeEvent<HTMLSelectElement>) => setExportView(e.target.value as api.InsightView)}
            options={api.INSIGHT_VIEWS.map((v) => ({ value: v, label: v.replace("_", " ") }))} children={undefined} />
          <Button variant="secondary" disabled={busy !== null || !ins} onClick={() => void doExport()}>Export CSV</Button>
          <Button variant="secondary" disabled={busy !== null} onClick={() => void load(true)}>{busy === "rebuild" ? "Rebuilding…" : "Rebuild"}</Button>
        </>
      } />
      <div style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary)", marginTop: "calc(-1 * var(--space-4))", marginBottom: "var(--space-4)" }}>
        Built {built} · fiscal years run June 1 – May 31 and are labeled by the year they end
      </div>
      {err && <Alert tone="error" style={{ marginBottom: "var(--space-4)" }}>{err}</Alert>}
      {exported && (
        <Alert tone="success" style={{ marginBottom: "var(--space-4)" }}>
          Exported to <span style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}>{exported}</span>{" "}
          <Button size="sm" variant="secondary" onClick={() => void api.revealExport(exported)}>Reveal</Button>
        </Alert>
      )}

      {!ins ? <EmptyState icon="loader" title="Loading insights" message="Reading the local mirror." action={undefined} /> : (
        <>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 1fr)", gap: "var(--space-3)", marginBottom: "var(--space-5)" }}>
            <Stat label="Member households" value={fmt(ins.kpis.members_now)} sub={`${ins.kpis.net_vs_prior_fy >= 0 ? "+" : ""}${fmt(ins.kpis.net_vs_prior_fy)} vs ${fyLabel(ins.current_fy - 1)}`} icon="users" tone="primary" />
            <Stat label={`Joins ${fyLabel(ins.current_fy)}`} value={fmt(ins.kpis.joins_this_fy)} sub="fiscal year to date" icon="user-plus" tone="success" />
            <Stat label={`Resignations ${fyLabel(ins.current_fy)}`} value={fmt(ins.kpis.resigns_this_fy)} sub="fiscal year to date" icon="user-minus" tone="neutral" />
            <Stat label="First-year retention" value={`${ins.kpis.year1_pct}%`} sub={`${fyLabel(ins.kpis.year1_cohort)} cohort · baseline ${ins.kpis.year1_baseline_pct}%`} icon="repeat" tone="primary" />
            <Stat label="Households at risk" value={fmt(ins.kpis.at_risk_count)} sub="current members matching a churn pattern" icon="triangle-alert" tone="accent" />
          </div>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Membership over time</CardTitle></CardHeader>
            <Lede>Active member households at the end of each fiscal year, and the joins and resignations behind them.</Lede>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "var(--space-4)" }}>
              <TrendChart rows={ins.trend} />
              <FlowsChart rows={ins.trend} />
            </div>
            <SoWhat text={s!.trend} />
            <TableView rows={ins.trend} getRowKey={(r) => String(r.fy)} columns={[
              { key: "fy", header: "Fiscal year", render: (r) => fyLabel(r.fy) },
              { key: "j", header: "Joins", align: "right", render: (r) => fmt(r.joins) },
              { key: "r", header: "Resignations", align: "right", render: (r) => fmt(r.resigns) },
              { key: "a", header: "Active at year end", align: "right", render: (r) => fmt(r.active_end_of_fy) },
            ]} />
          </Card>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>First-year retention by cohort</CardTitle></CardHeader>
            <Lede>Of the households that joined in each fiscal year, the share still members one year later. The two newest cohorts are highlighted.</Lede>
            <Year1Chart rows={ins.year1} emphasize={latestTwo} />
            <SoWhat text={s!.year1} />
            <TableView rows={ins.year1} getRowKey={(r) => String(r.cohort)} columns={[
              { key: "c", header: "Cohort", render: (r) => fyLabel(r.cohort) },
              { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
              { key: "p", header: "Still members after 1 year", align: "right", render: (r) => `${r.pct_retained}%` },
            ]} />
          </Card>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Cohort retention</CardTitle></CardHeader>
            <Lede>Each row is a join-year cohort; each cell is the share still members after that many years. Blank cells are years that haven't happened yet.</Lede>
            <CohortHeatmap cells={ins.cohort_matrix} />
            <SoWhat text={s!.cohort} />
            <TableView rows={ins.cohort_matrix} getRowKey={(r) => `${r.cohort}-${r.k}`} columns={[
              { key: "c", header: "Cohort", render: (r) => fyLabel(r.cohort) },
              { key: "k", header: "Years after", align: "right", render: (r) => r.k },
              { key: "p", header: "Still members", align: "right", render: (r) => `${r.pct_retained}%` },
            ]} />
          </Card>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Stickiness by join reason</CardTitle></CardHeader>
            {missing("Join_Reason__c") ? <Unavailable column="Join_Reason__c" /> : (
              <>
                <Lede>Households that joined 4–12 fiscal years ago and recorded a reason. Share still members today; a household counts under every reason it named. School-driven reasons are highlighted.</Lede>
                <HBarChart rows={ins.channels.map((c) => ({ label: c.label, pct: c.pct, n: c.n, still: c.still_members }))} emphasize={["Religious school", "Nursery school"]} />
                <SoWhat text={s!.channels} />
                <TableView rows={ins.channels} getRowKey={(r) => r.key} columns={[
                  { key: "l", header: "Join reason", render: (r) => r.label },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still members", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                  { key: "t", header: "Avg tenure (yrs)", align: "right", render: (r) => r.avg_tenure },
                ]} />
              </>
            )}
          </Card>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Stickiness by school history</CardTitle></CardHeader>
            <Lede>Same joiner window, grouped by whether the household ever had a child in nursery or religious school.</Lede>
            <HBarChart rows={ins.school.map((g) => ({ label: g.group, pct: g.pct, n: g.n, still: g.still_members }))} />
            <SoWhat text={s!.school} />
            <TableView rows={ins.school} getRowKey={(r) => r.group} columns={[
              { key: "g", header: "School history", render: (r) => r.group },
              { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
              { key: "p", header: "Share still members", align: "right", render: (r) => `${r.pct}%` },
            ]} />
          </Card>

          <Card style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Why people leave</CardTitle></CardHeader>
            {missing("Resign_Reason__c") ? <Unavailable column="Resign_Reason__c" /> : (
              <>
                <Lede>Coded resignation reasons by fiscal year. Reasons outside the six most common fold into "Other".</Lede>
                <ReasonsChart cells={ins.reasons} />
                <SoWhat text={s!.reasons} />
                <TableView rows={ins.reasons} getRowKey={(r) => `${r.fy}-${r.reason}`} columns={[
                  { key: "f", header: "Fiscal year", render: (r) => fyLabel(r.fy) },
                  { key: "r", header: "Reason", render: (r) => r.reason },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                ]} />
              </>
            )}
          </Card>

          <Card>
            <CardHeader><CardTitle>Households at risk</CardTitle></CardHeader>
            <Lede>Current members matching a churn pattern: first year of membership, nursery-school-only joiners, introductory tiers aging out, or families whose religious-school years just ended. Viewing this list is recorded in the audit log.</Lede>
            {atRisk === null
              ? <Button variant="secondary" disabled={busy !== null} onClick={() => void showAtRisk()}>{busy === "risk" ? "Loading…" : `Show ${fmt(ins.kpis.at_risk_count)} households`}</Button>
              : <TableView rows={atRisk} getRowKey={(r) => r.account_id} empty="No households match the at-risk rules." columns={[
                  { key: "n", header: "Household", render: (r) => r.name },
                  { key: "t", header: "Tier", render: (r) => r.tier ?? "—" },
                  { key: "j", header: "Joined", render: (r) => (r.join_fy ? fyLabel(r.join_fy) : "—") },
                  { key: "r", header: "Patterns", render: (r) => <span style={{ display: "inline-flex", gap: 4, flexWrap: "wrap" }}>{r.rules.map((k) => <Badge key={k} tone="warning">{RULE_LABELS[k] ?? k}</Badge>)}</span> },
                ]} />}
          </Card>
        </>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Wire navigation**

In `src/App.tsx`:
- `export type PageKey = "overview" | "data" | "segments" | "insights" | "audit";`
- Add `import InsightsPage from "./pages/InsightsPage";`
- Insert into `NAV` after segments: `{ key: "insights", icon: "chart-line", label: "Insights" },`
- Add in the frame: `{page === "insights" && <InsightsPage {...props} />}` before the audit line.

- [ ] **Step 3: Typecheck, test, build**

Run: `npx tsc --noEmit` (clean; apply the same `{undefined}` / `children={undefined}` call-site fixes as other pages if the design-system `.jsx` props are inferred as required). Run: `npx vitest run` (all pass). Run: `npm run build 2>&1 | tail -3` (`✓ built`). Icon check: `chart-line`, `users`, `user-plus`, `user-minus`, `repeat`, `triangle-alert`, `loader`, `database` all exist in lucide-react ≥ 0.4; `Icon.jsx` returns null for unknown names, so verify with `node -e "const i=require('lucide-react'); console.log(['ChartLine','Users','UserPlus','UserMinus','Repeat','TriangleAlert','Loader','Database'].map(n=>n+':'+!!i[n]).join(' '))"` — all `true`.

- [ ] **Step 4: Manual verification (human, GUI)**

`npm run tauri dev` → Insights tab → KPIs and all six cards render from the real mirror; Rebuild works; Export CSV writes under `%APPDATA%\org.emanuelnyc.customerintelligence\exports\` and Reveal opens Explorer; "Show N households" lists names with pattern badges and adds an `insights.at_risk` audit row; Audit tab shows `insights.rebuild`/`insights.export` rows. Withhold `Join_Reason__c` on the Data page, re-sync, and confirm the join-reason card shows the unavailable state.

- [ ] **Step 5: Commit**

```bash
git add src/pages/InsightsPage.tsx src/App.tsx
git commit -m "feat(ui): insights page with KPIs, retention charts, at-risk list, and export"
```

---

### Task 9: README and wrap-up

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Document**

Add a section after "Governance model":

```markdown
## Insights
The Insights page is computed from the local mirror only (never Salesforce). After each
sync + profile, a household mart (`_m_household`) is rebuilt from the synced, non-withheld
Account columns; the page reads that table. Fiscal years run June 1 – May 31 and are labeled
by the year they end. Viewing the at-risk list and exporting CSVs are recorded in the audit log;
exports land in `%APPDATA%\org.emanuelnyc.customerintelligence\exports\`.
```

- [ ] **Step 2: Full verification**

`npm run verify` — Expected: typecheck clean, vitest green, cargo test all `ok`.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: insights page and mart in README"
```
