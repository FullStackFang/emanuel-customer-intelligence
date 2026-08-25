# Membership Insights — design ("make the mirror speak")

Status: approved in conversation 2026-08-25; implements Approach A (analytic layer inside the app, two phases).
Builds on: `2026-08-25-customer-intelligence-v1-design.md` (the mirror, governance, command boundary).
Findings that motivated this: `docs/reports/membership-quick-scan-2026-08-25.html`.

## 1. Purpose

Staff want membership insights they do not have to assemble by hand: is membership growing, which
join channels produce durable members, whether retention has dropped by cohort, why people leave,
and which current households look like next year's resignations. The quick scan (25 Aug 2026)
proved the mirror can answer these; this design makes the answers **live, repeatable, and rendered
inside the app in the app's own design system**.

Unit of analysis: the **household** (Account). People (Contacts) enrich it in v2.
Fiscal year: **June 1 – May 31, labeled by the calendar year in which it ends** (June 2024 – May 2025 = FY2025).

## 2. Non-negotiables

- Everything is computed from the **local mirror only** — never a live Salesforce call. A view is
  only as fresh as the last sync, and says so.
- The analytic layer reads **only synced, non-withheld columns**. It can never reintroduce a
  withheld field. If a column a view needs is absent (not synced or withheld), the view reports
  itself *unavailable* with the reason; it never guesses.
- All SQL is **static, versioned text in Rust** with parameters bound. No user string ever reaches
  an insights query. (The at-risk list and exports are filtered by fixed criteria, not free text.)
- Aggregate views carry no personal data. The **at-risk list does** (household name); viewing and
  exporting it are **audited**, like `segment.query` today.
- Same command discipline as v1: every command returns webview-safe data, never a token; the store
  lock is never held across an await; `_audit` stays insert-only.
- Charts are bundled (npm), never fetched: the capability stays `core:default`.

## 3. Architecture

```
sync_selected ──► profile_selected ──► insights::rebuild ──► _m_household (mart table in mirror.db)
                                                                    │
get_insights ◄──────────────────────── insights::views(store) ◄─────┘   (reads mart only; ms, not seconds)
get_at_risk  ◄── audited
export_insights_csv ◄── audited, writes to app_data_dir/exports, revealed via Rust opener
```

**Why a mart:** the Account mirror table has ~405 columns and ~450 MB decrypts per full scan; the
quick-scan queries took tens of seconds each because every one rescanned it. Building one narrow
household table (~13k rows × ~25 columns) after each sync makes every view a sub-second query and
keeps all derivation logic in one place.

New Rust module `src-tauri/src/insights.rs`; three new commands in `commands.rs`; new page
`src/pages/InsightsPage.tsx`; typed wrappers in `src/api.ts`; a fifth nav item.

## 4. The mart: `_m_household`

One row per household that is or ever was a member family: `Account."Type" = 'Member Family'`.
Rebuilt from scratch (`DROP` + `CREATE` + `INSERT … SELECT`) inside one transaction. Underscore
prefix marks it internal (excluded from object/mirror listings and from `purge_mirror`'s per-object
drop list; it is dropped and rebuilt by `insights::rebuild`, and dropped by `purge_mirror` too).

| column | derivation (v1) |
|---|---|
| `account_id` | `Account.Id` |
| `name` | `Account.Name` (for the at-risk list only; never surfaced by aggregate views) |
| `is_current` | `IsATempleMember__c = 'true'` |
| `join_date`, `original_join_date`, `resign_date` | the three Account dates, **nulled when outside 1900–2035** (placeholders `2199-…`, `2991-…` seen in data) |
| `bad_join_date` | true when `Join_Date__c` was present but rejected |
| `join_fy`, `cohort_fy` | `fy(join_date)`, `fy(coalesce(original_join_date, join_date))` |
| `resign_fy` | `fy(resign_date)` **only when not current**; a current household's old `LastResignDate__c` is a past spell, not a resignation (302 such rejoiners exist) |
| `resigned_unknown_date` | resigned (`IsResigned__c`) but no usable resign date — treated as lost after year 1 in retention math, and counted separately |
| `rejoined` | `original_join_date < join_date` |
| `tier` | `Sub_Type__c` |
| `category` | `Member_Category__c` |
| `join_reason` | raw `Join_Reason__c` multipicklist |
| `ch_*` (12 booleans) | `join_reason` contains the phrase: religious_school, nursery_school, affiliation, life_cycle, family (“To be with Family”), young_professionals, community, hhd_tickets (“High Holy Day”), streicker, clergy, worship, move |
| `rs_family`, `ns_family` | `FormerReligiousSchoolStudents__c > 0 OR ActiveReligiousSchoolStudents__c > 0`; `WasEverNSAffiliated__c = 'true'` |
| `active_rs_students` | `ActiveReligiousSchoolStudents__c` |
| `last_rs_year` | `LastYearAttendedRS__c` (string like `2025-2026`; end year parsed) |
| `resign_reason_group` | `Resign_Reason__c` bucketed: Moved · Non-payment · No longer engaged · Deceased · Young-adult tier aged out · Joined another synagogue · Elderly/ill · Financial hardship · Displeased · Other · (not coded) — first match wins in that order |
| `built_at` | rebuild timestamp (also stored in `_meta.insights_built_at`) |

`fy(date)`: `year(date) + (month(date) >= 6 ? 1 : 0)`. `FY_START_MONTH = 6` is a single constant.
`current_fy()` = `fy(today)`.

**Required source columns** (all on Account): `Id, Name, Type, IsATempleMember__c, IsResigned__c,
Join_Date__c, OriginalJoinDate__c, LastResignDate__c, Sub_Type__c, Member_Category__c,
Join_Reason__c, Resign_Reason__c, FormerReligiousSchoolStudents__c,
ActiveReligiousSchoolStudents__c, WasEverNSAffiliated__c, LastYearAttendedRS__c`. Rebuild checks
each exists in the Account mirror table; a missing one nulls its derived columns and is listed in
`unavailable`, so the page can show "Join-channel views need `Join_Reason__c` to be synced" rather
than an error. `Name` missing only disables the at-risk list.

**Retention definition (v1, spell-based):** household is a member in FY `N` if
`join_fy <= N AND (resign_fy IS NULL OR resign_fy > N)`; retained into `N+1` if member in both.
`resigned_unknown_date` rows count as members only in `join_fy`. v2 replaces this with billing truth
(§10) without changing any view's shape.

## 5. Views (`get_insights`)

One command returns one bundle so the page renders in a single round trip:

```rust
pub struct Insights {
  pub built_at: Option<String>, pub current_fy: i32, pub unavailable: Vec<String>,
  pub kpis: Kpis, pub trend: Vec<TrendRow>, pub year1: Vec<CohortYear1>,
  pub cohort_matrix: Vec<CohortCell>, pub channels: Vec<ChannelRow>,
  pub school: Vec<SchoolRow>, pub reasons: Vec<ReasonCell>,
}
```
(`at_risk_count` lives inside `kpis`; the list itself is a separate, audited command — §6.)
```rust
```

| view | rows | definition |
|---|---|---|
| `kpis` | 1 | `members_now` (is_current), `net_vs_prior_fy`, `joins_this_fy`, `resigns_this_fy`, `year1_retention_latest` (latest cohort with ≥1 full year), `year1_retention_baseline` (mean of cohorts 2010–2023), `at_risk_count` |
| `trend` | FY2005..current | `fy, joins, resigns, active_end_of_fy` |
| `year1` | cohorts 2010..current−1 | `cohort, n, pct_retained_1y` |
| `cohort_matrix` | cohorts 2010..current−1 × k=1..8 where `cohort+k <= current_fy` | `cohort, n, k, pct_retained` |
| `channels` | 12 channels, `n >= 20` | joiners in `[current_fy−12, current_fy−4]` (≥3 full years) with a join reason; `channel, n, still_members, pct, avg_tenure, left_within_2y` |
| `school` | 4 groups | same joiner window, all households; `Both NS+RS / RS family / NS family / No school history`; `n, still, pct` |
| `reasons` | FY(current−5)..current × groups | `fy, reason_group, n` (resigned households) |

Cohort windows are expressed relative to `current_fy` so the page never goes stale.

## 6. At-risk list (`get_at_risk`) — v1 rules

Current member households matching any rule; each row carries the rule(s) that fired.

| rule | logic | why (from the scan) |
|---|---|---|
| `new_ns_only` | `join_fy >= current_fy−2` and `ch_nursery_school` and not `ch_religious_school` and not `rs_family` | NS-only joiners retain 26% |
| `intro_tier_aging` | `tier` in {Young Adult Member, Young Professionals, Downtown} and `current_fy − join_fy >= 2` | ~30/yr age out |
| `rs_ended` | `rs_family` and `active_rs_students = 0` and `last_rs_year` in `[current_fy−2, current_fy−1]` | churn follows the last RS year |
| `first_year` | `join_fy = current_fy − 1` | first-year loss doubled in FY2024–25 |

Returns `Vec<AtRiskRow { account_id, name, tier, join_fy, rules: Vec<String> }>` ordered by rule
count desc. Audited as `insights.at_risk` with `{count}`. Rules are fixed in code (v1); tuning them
is a code change, on purpose.

## 7. Export (`export_insights_csv`)

`export_insights_csv(view: "trend"|"year1"|"cohort_matrix"|"channels"|"school"|"reasons"|"at_risk")`
writes `app_data_dir()/exports/insights-<view>-<YYYYMMDD-HHMM>.csv` (UTF-8, header row) and returns
the path; the UI shows it with a **Reveal** action that calls `reveal_export(path)`, a Rust command
using `tauri_plugin_opener::reveal_item_in_dir` (Rust-side; no webview opener permission).
`reveal_export` **refuses any path that does not resolve inside `app_data_dir()/exports`** — the
webview-supplied string is never trusted as an arbitrary path. Export is audited `insights.export`
with `{view, rows}`. No other file access is added.

## 8. Rebuild trigger

- The Overview "Sync now" flow already calls `sync_selected` then `profile_selected`. The
  **`profile_selected` command** ends by calling `insights::rebuild` when an `Account` mirror table
  exists (rebuild failure is reported in the command's error, never swallowed).
- `get_insights` rebuilds first when `_m_household` is missing or `_meta.insights_built_at` is older
  than the newest `_objects.last_synced_at`; the page's **Rebuild** action forces it.
- Every rebuild is audited `insights.rebuild` `{households, unavailable}`. The Insights page shows
  "Built <time> from the sync of <time>".

## 9. Frontend

**Nav:** fifth item `{ key: "insights", icon: "chart-line", label: "Insights" }` between Segments and Audit.

**`InsightsPage.tsx`** (design-system components throughout; Title Case title, sentence case
elsewhere, `--font-mono` for any API name, no gold text under 18px):

1. `PageTitle` "Insights" with the freshness line and actions: **Rebuild** (secondary) · **Export…** (secondary, choose view).
2. **KPI row** — five `Stat` tiles: Member households (delta vs prior FY) · Joins this FY · Resignations this FY · First-year retention, latest cohort (vs baseline) · Households at risk.
3. **Cards, one per view**, each: `CardHeader/CardTitle`, one-line explanation, the chart, a
   "Table view" disclosure (design-system `Table`), and a "So what" line generated from the numbers
   (templated, e.g. "FY2025 cohort kept 67% vs 87% baseline").
   - Membership trend — line (active) + grouped columns (joins vs resigns), two charts, one axis each.
   - First-year retention by cohort — columns; latest two cohorts emphasized, rest de-emphasized.
   - Cohort retention — **CSS-grid heatmap**, sequential sapphire ramp, value in each cell.
   - Stickiness by join reason — horizontal bars; school channels emphasized.
   - Stickiness by school history — horizontal bars, single hue.
   - Why people leave — stacked columns, ≤7 fixed-order series + legend.
4. **At-risk list** card — `Table` with household name (body font — a proper noun, not an API
   name), tier, joined FY, and the rules that fired as `Badge`s; loads on demand (button "Show
   households"), so the audit row reflects intent rather than every page view.
5. Unavailable views render the design-system `EmptyState` naming the missing column.

**Charts:** **Recharts** (MIT, bundled) for line/bar/stacked; heatmap in CSS. Series colors come from
design tokens: sequential ramp = sapphire `--color-primary-100…700`; emphasis pair = primary-500 vs
`--color-neutral-300`; the 7-slot categorical order for reasons is fixed in one constant and **must
pass `dataviz/scripts/validate_palette.js`** (light mode, chart surface `--bg-primary`) — the plan
picks steps from the design-system ramps that pass. Marks follow the dataviz specs (≤24px bars,
4px rounded data-ends, 2px lines, hairline grid, legend for ≥2 series, hover tooltips, table twin).
Text never wears a series color.

**`api.ts`:** `getInsights()`, `getAtRisk()`, `exportInsightsCsv(view)`, `revealExport(path)`, plus the
types above — names 1:1 with the commands.

## 10. v2 — billing and enrollment truth (designed for, not built now)

When `BillingStatementLine__c`, `Class_Enrolment__c`, and `Committee_Membership__c` are synced:
- a second mart `_m_household_fy` (household × FY) with `active` from a **dues line for that FY**
  (`active_source = billing|spell`), `paid`, `dues_amount`, `rs_enrolled`, `ns_enrolled`,
  `on_committee`;
- retention math switches to `active`, per-year, with the spell method as fallback for pre-billing
  years; every view keeps its shape;
- new views: NS→RS hand-off, years-since-last-RS churn curve, billed-but-unpaid at-risk rule,
  young-adult tier conversion.
Open question to settle before v2: are dues statements issued to **every** member household every
fiscal year (so a missing dues line reliably means non-member)?

## 11. Testing

- **Rust unit (`insights.rs`)**: `fy()` boundary cases (May 31 → same year; June 1 → next);
  placeholder-date rejection; channel flag parsing on multipicklist strings; reason bucketing order.
- **Rust integration (`insights.rs`, temp encrypted store)**: create a synthetic `Account` mirror via
  `replace_mirror` with ~12 crafted households (current, resigned with date, resigned unknown date,
  rejoiner, placeholder date, NS-only joiner, intro tier) → `rebuild` → assert `trend`, `year1`,
  `cohort_matrix`, `channels`, `school`, `reasons`, and `at_risk` rules exactly; assert the
  `unavailable` path when a column is dropped; assert `purge_mirror` drops the mart.
- **Command boundary**: at-risk and export write audit rows; aggregate views do not.
- **Frontend (vitest)**: `api.ts` wrapper names; heatmap color-scale function; "so what" templating.
- **Manual**: sync → Insights renders all cards; Rebuild/Export/Reveal work; unavailable state when
  `Join_Reason__c` is withheld.

## 12. Out of scope (v1)

Billing/enrollment marts (§10); editable at-risk rules; PDF report; per-person (Contact) views;
charts anywhere but the Insights page; scheduling/auto-sync.
