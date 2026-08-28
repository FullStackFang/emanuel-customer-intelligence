## 1. Decisions to lock before coding

- [x] 1.1 Band edges confirmed: New 0–1 · Establishing 2–4 · Settled 5–9 · Long-standing 10–24 · Legacy 25+ (design decision 1). One constant table in Rust; adjust here if the product owner wants different edges.
- [x] 1.2 Replace, don't keep, the per-join-year "Cohort makeup" and "Cohort value" charts and tables (design decision 2).
- [x] 1.3 Bands computed in Rust; `by_cohort` leaves the Financials payload (design decision 2).

## 2. Backend — bands and the ten-household floor (tests first)

- [x] 2.1 Test `membership_age_bands_assign_by_years_since_join_with_inclusive_edges`: ages 0,1 → New; 2,4 → Establishing; 5,9 → Settled; 10,24 → Long-standing; 25 → Legacy; all five bands emitted in order even when empty; shares sum to 100 of dated households.
- [x] 2.2 Test `membership_age_excludes_undated_joins_and_counts_only_current_households`: undated and non-current households appear in no band.
- [x] 2.3 Test `financials_by_membership_age_replaces_by_cohort_and_withholds_small_band_averages`: extend `financials_trend_cohorts_and_concentration` — band totals, `share_of_households` / `share_of_received`, `received_per_household` is `None` under ten households and `Some` at ten; `by_cohort` no longer exists on the view.
- [x] 2.4 Test `views_rebuild_when_cached_read_model_predates_membership_age`: mirror the existing cache-revision regression test (`insights.rs` ~4127–4140) — a cached blob without `membership_age` / `by_membership_age` is not served.
- [x] 2.5 Implement `MembershipAgeBand` constant table, `membership_age(hh, cur) -> Vec<MembershipAgeRow>`, and `FinancialAgeRow` + `by_membership_age` inside `financials` (reuse the existing `current` / `per_hh` alignment; delete `cohort_agg` / `FinancialCohortRow` / `by_cohort`).
- [x] 2.6 Add `membership_age` to `Insights` and `by_membership_age` to `FinancialsView`; bump `READ_MODEL_REVISION`. All 2.1–2.4 pass; `cargo test` green.

## 3. Frontend — API + charts + copy (tests first)

- [x] 3.1 `src/api.ts`: add `MembershipAgeRow`, `FinancialAgeRow`; add `membership_age` to `Insights` and `by_membership_age` to `FinancialsView`; remove `FinancialCohortRow` and `by_cohort`. `api.test.ts` fixture updated. (No change needed: `api.test.ts` has no Insights/Financials fixture referencing the changed types.)
- [x] 3.2 `format.test.ts`: replace the makeup assertions — takeaway names the New + Establishing share, the Legacy share, and the largest band; add a survivors takeaway test (joined FY2010 → last complete year, count and share still members); add a Financials takeaway test naming the band with the largest money-minus-households gap.
- [x] 3.3 `InsightsPage.test.tsx`: fixture gains `membership_age`, `by_membership_age`, and a `trend` / `cohort_makeup` pairing spanning pre-2010 and 2010+; assert the three card titles render, "Cohort value" and "Cohort makeup of current members" do not, the "Before FY2010" group shows survivors with no joined figure and its caption, a suppressed band shows "—" with the fewer-than-ten note, and the undated-join note appears when the fixture has a remainder.
- [x] 3.4 `format.ts`: `soWhat.makeup` rewritten around bands; add `survivors` and `financialAge` takeaways.
- [x] 3.5 `charts.tsx`: `MembershipAgeChart` (households per band, share in tooltip), `ValueByAgeChart` (paired share-of-households / share-of-received bars on one 0–100% axis, per-household in tooltip or "—"), `JoinedVsStillHereChart` (grouped joined / still-here bars FY2010 → current plus the "Before FY2010" survivors-only bar). Delete `CohortMakeupChart` and `CohortValueChart`. Follow the existing palette / `axisTick` / `tooltipStyle` conventions and keep `isAnimationActive={false}` for PDF capture.
- [x] 3.6 `InsightsPage.tsx`: Overview — replace the "Cohort makeup of current members" card with "Makeup of today's members by membership age" and add "Joined vs. still here" after it (lede states the FY2010 floor and why); Financials — replace "Cohort value" with "Value by membership age" (lede keeps the one-year-snapshot / FY2023 coverage caveat). Each card keeps its `TableView` (bands or cohorts) and reuses the undated-join note. Remove `makeupLatestTwo` / `finCohortLatestTwo`.
- [x] 3.7 `npm run typecheck` and `npm test` green.

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo test` green (151 passed); `npm run typecheck` clean; `npm test` green (36 passed).
- [ ] 4.2 Real-mirror check (user-run, on a COPY of the mirror via `src-tauri/examples/time_insights.rs` or the app): band households sum to `members_now` minus the undated remainder; every FY2010+ cohort's still-here ≤ joined; Financials band `received` sums to the latest-complete-year member total; no band under ten households shows an average.
- [ ] 4.3 Launch the real Tauri app: first load after upgrade rebuilds the read model (new cards populated, not empty); the three cards read correctly at a glance; PDF export includes all three with rendered charts.
