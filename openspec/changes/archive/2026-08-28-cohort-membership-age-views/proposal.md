## Why

Two Insights cards — Overview "Cohort makeup of current members" and Financials "Cohort value" — draw one bar per join fiscal year for every year any current Membership Household joined, so both charts run from FY1950 to FY2026. That grain answers a question no executive asks ("what happened in 1987?") and hides the ones they do:

- **How old is our base?** Fifty bars cannot be read as a distribution; the executive wants five.
- **Does a household become more valuable the longer it stays?** The per-household average is as tall and confident for a FY1961 cohort of two households as for a FY2019 cohort of 150, so the chart is dominated by noise at the old end. Worse, a per-household figure for a one- or two-household cohort *is* a household's dues, which breaks the Financials tab's aggregate-only contract.
- **Of the households we bring in, how much sticks?** The product owner asked to overlay each cohort's original join count against its survivors "so we can see how much of it is left." The retention grid already holds that answer but as a heatmap that executives do not read at a glance.

The data for all three is already in the mart (`join_fy`, `is_current`, per-year joins, latest-complete-year receipts); this is a re-aggregation and a presentation change, with no new mirrored Salesforce data.

## What Changes

- **Membership age replaces join year as the grain of both composition charts.** Membership age is the number of fiscal years since the start of a household's current Membership Spell (`current_fy − join_fy`), bucketed into five lifecycle bands: **New** (0–1), **Establishing** (2–4), **Settled** (5–9), **Long-standing** (10–24), **Legacy** (25+).
  - Overview → **"Makeup of today's members by membership age"**: households and share of base per band, with a plain-language takeaway.
  - Financials → **"Value by membership age"**: per band, share of Membership Households versus share of latest-complete-year cash received, with the per-household average in the tooltip and table. The gap between the two shares is the headline.
- **A new Overview card, "Joined vs. still here"**, shows for each join cohort FY2010 → current the households that joined that year beside those still members today, plus one collapsed "Before FY2010" bar carrying survivors only. FY2010 is the same floor the retention grid uses; before it, departed households are not reliably recorded, so a joined-versus-survivor comparison would overstate retention.
- The year-by-year "Cohort makeup" and "Cohort value" charts and tables are **removed**, not kept as detail.
- **Aggregate-only enforcement moves server-side for Financials**: the per-join-year `by_cohort` rows leave the payload; the band rows replace them, and a band under ten households carries no per-household figure.

## Capabilities

### New Capabilities
<!-- None. -->

### Modified Capabilities
- `membership-lifecycle-insights`: the Overview and Financials views gain membership-age composition, a joined-versus-survivors cohort view, and a testable aggregate-only rule for financial composition; the per-join-year composition charts are retired.

## Impact

- **Backend (`src-tauri/src/insights.rs`)**: new `membership_age` rows on the Insights payload and `by_membership_age` rows on the Financials view (replacing `by_cohort`), both derived from the existing `Hh` mart and the existing per-household latest-complete-year receipts. `cohort_makeup` and `trend` are unchanged and together feed the joined-versus-survivors card. `READ_MODEL_REVISION` is bumped so a cached read model is not served with the new fields absent.
- **API (`src/api.ts`)**: `MembershipAgeRow` and `FinancialAgeRow` types; `FinancialCohortRow` and `FinancialsView.by_cohort` removed. Command surface unchanged.
- **Frontend (`src/pages/InsightsPage.tsx`, `src/pages/insights/charts.tsx`, `src/pages/insights/format.ts`)**: two band charts and one grouped joined-versus-survivors chart replace `CohortMakeupChart` and `CohortValueChart`; the `soWhat.makeup` takeaway is rewritten around bands; the Overview gains one card.
- **Privacy / audit**: strengthened, not changed in kind. Financial figures reaching the webview are now band totals over at least ten households or nothing; no household-level dollar figure can appear. Household counts per join year (already in the payload) remain, as they carry no money and are needed for the survivors view. No named access, no audit-event change.
- **Source data**: no new columns. Join fiscal year still comes from Account join date; receipts from `BillingStatementLine__c` for the latest complete year (FY2023+ coverage caveat unchanged).
- **Exports / PDF**: the CSV export views (`trend`, `year1`, `cohort_matrix`, …) never included cohort makeup or financials, so exports are unaffected. The report surface gains one card and swaps two; all must still reach printable layout before PDF capture.
- **Tests**: Rust unit tests for band assignment, edges, undated households, and the ten-household floor; `InsightsPage.test.tsx` and `format.test.ts` fixtures and assertions updated for the new shapes and copy.
