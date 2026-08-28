## Context

The Insights payload carries `cohort_makeup: CohortMakeupRow[]` (`insights.rs` `cohort_makeup`, one row per join fiscal year with the count of current Membership Households and share of base, no floor year) and `financials.by_cohort: FinancialCohortRow[]` (`insights.rs` `financials`, one row per join fiscal year with households, cash received in the latest complete fiscal year, and the per-household average). `InsightsPage.tsx` renders each as a single-series bar chart (`CohortMakeupChart`, `CohortValueChart`) with the two newest cohorts emphasized and a full table beneath. Both run from the earliest join year on record (FY1950) to the in-progress year. The retention views (`year1`, `cohort_matrix`) start at `FIRST_COHORT_FY = 2010`; `trend` (joins, resignations, active households per fiscal year) starts at `FIRST_TREND_FY = 2005`.

Constraints are unchanged: read-only Salesforce, encrypted local mirror only, the webview reaches data only through fixed Rust commands, aggregate-only Insights (the Financials tab in particular shows decile bands and totals, never a household), and every report card must reach a measurable layout before PDF capture. `CONTEXT.md` reserves "Membership Spell" for a continuous active period and asks that "tenure" be avoided; this design uses **membership age** for years since the current spell began.

## Goals / Non-Goals

**Goals:**
- Present composition of today's Membership Households at a grain an executive reads in one look: five membership-age bands.
- Make the value question explicit: does money per household rise with membership age, and which band carries the base?
- Give the product owner's "how much of each cohort is left" view directly — joined versus still here, per cohort, inside the window where departures are reliably recorded.
- Move financial small-count protection to the Rust side of the trust boundary.

**Non-Goals:**
- Lifetime value or multi-year value by cohort — billing coverage begins FY2023, so a one-year snapshot is all the data supports; the lede keeps saying so.
- Changing the retention grid, first-year retention, or any Jobs / Renewal / Geography / Risk view.
- Rejoin-aware membership age (using `cohort_fy`, the original join) — the composition views key on `join_fy` today so they reconcile with `trend` and the retention grid; that stays.
- Segment filters on the new cards.

## Decisions

### 1. Membership age, not join year, is the grain — five fixed bands
`age = current_fy − join_fy`, where `current_fy` is the in-progress fiscal year (so age 0 is this year's joiners, age 1 last year's). Bands: **New** 0–1 · **Establishing** 2–4 · **Settled** 5–9 · **Long-standing** 10–24 · **Legacy** 25+. Band order is fixed and every band is always emitted, even when empty, so charts and tables have a stable shape.
- **Why:** five bands are readable and map to how staff already talk about households (new families, settled families, the old guard). Age rather than year keeps the chart meaningful as the calendar moves — a FY2010 bar means something different every year; "5–9 years" does not.
- **Alternative considered:** a fixed 10- or 15-year window of join years with an "earlier" bucket. Rejected: still asks the reader to do the subtraction, and the "earlier" bucket would hold most of the base.
- **Alternative considered:** quantile (equal-count) bands. Rejected: edges would drift with every rebuild and could not be explained in a sentence.

### 2. Bands are computed in Rust and replace the per-join-year financial rows
Add `membership_age: Vec<MembershipAgeRow>` to the Insights payload and `by_membership_age: Vec<FinancialAgeRow>` to `FinancialsView`; delete `by_cohort` and `FinancialCohortRow`. A `FinancialAgeRow` carries `households`, `received`, `share_of_households`, `share_of_received`, and `received_per_household: Option<f64>` which is `None` when `households < 10`.
- **Why:** the current payload sends the webview a per-household dollar average for join years holding one or two households — a household's dues in all but name. Rolling up in the frontend would leave that in the payload. The geography change set the precedent that suppression happens before data crosses to the webview; this follows it. The ten-household floor matches the rate floor used there.
- **Alternative considered:** frontend-only rollup of `by_cohort` and `cohort_makeup` (no Rust, no cache bump). Rejected for the privacy reason above; it was the first proposal to the product owner and is explicitly reversed here.
- **Alternative considered:** keep `by_cohort` alongside the bands for a detail table. Rejected: the product owner asked for fewer, sharper views, and keeping it keeps the leak.

### 3. `cohort_makeup` stays, and the joined-versus-survivors card is composed in the frontend
The Overview card "Joined vs. still here" pairs, for each fiscal year from `FIRST_COHORT_FY` (2010) to `current_fy`, `trend[fy].joins` with `cohort_makeup[fy].current`. Rows of `cohort_makeup` before 2010 are summed into one "Before FY2010" entry that shows survivors only, with no joined bar and a caption explaining why. Household counts per join year carry no money, so they may stay in the payload.
- **Why:** both series already exist and key on `join_fy`; survivors are a subset of joins by construction, so the grouped bar can never show more survivors than joiners. Putting the FY2010 floor on the card, rather than pretending to know how many joined in 1985, is what makes the comparison honest — the retention grid draws the same line for the same reason.
- **Alternative considered:** a new Rust `cohort_survival` view. Rejected: it would duplicate two arrays the payload already carries.
- **Alternative considered:** extending the window back to `FIRST_TREND_FY` (2005). Rejected: the retention grid's floor is the one staff already trust; one floor, one explanation.

### 4. The band chart on Financials plots two shares, not dollars
"Value by membership age" draws, per band, share of Membership Households and share of latest-complete-year cash received as a pair of bars on one 0–100% axis; the per-household average appears in the tooltip and table (as "—" for a suppressed band).
- **Why:** the executive insight is the gap ("Legacy is 22% of households and 35% of the money"), which two shares show directly and dollars do not. Shares also stay comparable as the base grows.
- **Alternative considered:** per-household dollars as the bar. Rejected: it is the noisier measure and the one that needed suppression.

### 5. Takeaways are rewritten around bands
`soWhat.makeup` becomes: the share of the base that is New + Establishing (joined within the last five fiscal years) and the share that is Legacy, naming the largest band. The joined-versus-survivors card gets its own sentence: of the households that joined FY2010 through the last complete year, the count and share still members. The Financials card's takeaway names the band with the largest gap between its share of money and its share of households.

### 6. Read-model revision bump
`READ_MODEL_REVISION` increments. Without it a cached read model is served with `membership_age` absent and `by_membership_age` absent, which is exactly the class of bug that previously hid the Financials data.

## Failure behavior, freshness, migration, rollback

- **Undated joins:** current households with no usable `join_fy` are excluded from every band and from the survivors card, and the existing "N current households have no usable join date and are not shown" note is shown under each affected card (band households therefore sum to `members_now` minus that remainder).
- **Suppressed band:** a `FinancialAgeRow` with fewer than ten households still reports its household count and its two shares (they are shares of a base of thousands), but no per-household figure; the UI renders "—" and the tooltip says "fewer than 10 households".
- **Financials unavailable:** unchanged — when the `renewal` capability is off or no member money exists in the latest complete year, the Financials tab shows its existing unavailable card and no age chart.
- **Empty window:** if no `cohort_makeup` row falls at or after FY2010 the survivors card shows only the "Before FY2010" survivors bar and its caption; it does not error.
- **Freshness:** all figures derive from the mart at rebuild time and are cached with the read model; the existing built-at / stale indicators apply unchanged.
- **Migration:** additive Rust fields plus removal of `by_cohort`; the TS types, fixtures, and cards change in the same commit; the revision bump invalidates cached read models on first load. No stored-data migration.
- **Rollback:** revert the commit; the prior read model is rebuilt on next load because the revision no longer matches.

## Open Questions (resolved)

- **Replace or keep the per-year charts?** → **Replace**, per the product owner's request for fewer, sharper views and the privacy reason in decision 2.
- **Band edges** → the five bands in decision 1; the product owner may adjust before implementation, and they are one constant table in Rust.
- **Frontend-only?** → **No**, decision 2.
