## 1. Decisions to lock before coding

- [x] 1.1 **Offline packaged basemap** confirmed (per spec) — remove the network CARTO fetch. See design.md decision 5.
- [x] 1.2 Join-era segment is **per join fiscal year** (no buckets), accepting sparse-ZIP suppression; Tier / Member Category collapse to **top 6 + "Other"**. See design.md decision 6.
- [x] 1.3 **Cut over immediately** — `get_insights` drops `zip_attrition`; the map consumes `zip_geography` only.
- [x] 1.4 **Per-FY ZIP time series** (supersedes single-snapshot decision 4): resolve each household's ZIP *as of the display fiscal year* from dated `BillingStatement__c`, bounded by statement coverage (FY2023+). For display years before a household's first statement, use its earliest known statement ZIP (Account fallback if none), labeled as a proxy. See design.md decision 4 (revised).

## 2. Backend — per-FY ZIP + `zip_geography` command (tests first)

- [x] 2.0 Per-household ZIP **time series** from dated `BillingStatement__c` (`billing_statement_zip_series`, one ZIP per FY = latest statement in that FY), plus the `zip_as_of(fy)` resolver (latest ≤ FY-end; else earliest; else Account fallback). Privacy filters carried over. Test: `per_fiscal_year_zip_series_reads_dated_statements_and_places_by_year`, `zip_as_of_resolves_the_statement_in_force_that_fiscal_year`.
- [x] 2.1 Density test — `density_counts_active_households_per_zip_and_suppresses_small_zips`.
- [x] 2.2 Provenance test — `provenance_counts_join_year_households_at_their_as_of_zip` (as-of-FY placement; earliest-known proxy for pre-coverage).
- [x] 2.3 Net Change test — `net_change_reports_joins_minus_exits_and_keeps_churn_in_the_tooltip` (zero-net-with-churn).
- [x] 2.4 Attrition test — `attrition_mode_matches_the_rate_formula_and_uses_the_ten_household_rate_floor` (rate math regression guard; `<10` rate floor).
- [x] 2.5 Segment tests — `segment_filter_narrows_the_population_before_aggregation`, `tier_segment_buckets_beyond_the_top_six_into_other`.
- [x] 2.6 Suppression — enforced in `zip_geography` (`min_n` 5/10); every cell carries `n`. Covered across the mode tests.
- [x] 2.7 Out-of-area + capability — `out_of_area_counts_normalizable_non_new_york_zips_without_dropping_them_silently`, `zip_geography_view_gates_on_capability_and_leaks_no_raw_postal`. Backend owns the NY ZCTA set (`ny_zctas.txt`) so out-of-area is counted pre-suppression.
- [x] 2.8 Implemented `zip_geography` + `zip_geography_view` over `Hh`, reusing `member_in`, `zip_as_of`, and the geo capability gate. All 2.0–2.7 pass.
- [x] 2.9 Registered `commands::zip_geography` in the fixed handler; audited as aggregate access; response carries only ZIP + measure + N (no name/postal/coord/bill-to-other). Capability renamed `zip_attrition` → `geography`.

## 3. Frontend — API + mode-driven map (tests first)

- [x] 3.1 Added `GeoMode`, `Segment`, `ZipGeoCell`, `SegmentOptions`, `ZipGeography` types + the `zipGeography(fy, mode, segment)` invoke wrapper to `src/api.ts`; removed `ZipAttritionCell` and the `zip_attrition` payload field.
- [x] 3.2 Rewrote the map tests in `InsightsPage.test.tsx`: default Density render, mode toggle re-encodes + refetches, segment dropdown filters, FY selector drives refetch, out-of-area + suppression notes, unavailable state fetches nothing. Plus the pure geo tests on the Rust side.
- [x] 3.3 Per-ZIP largest-ring centroids precomputed once at module load (`centroidOf`/`ringCentroid`) into a `cent` point source for the symbol layers.
- [x] 3.4 `ZipGeographyMap`: graduated-symbol circle layer (Density/Provenance, radius ∝ √count), diverging color for Net Change, rate choropleth for Attrition — counts on symbols, rate on fill, never one scale.
- [x] 3.5 Mode toggle, segment dropdown (with top-6 Tier/Category, per-FY join era, channels, school lifecycle), per-mode legends, and N in every tooltip/inspector/accessible label.
- [x] 3.6 Out-of-area counter ("N members not shown (outside the mapped New York area)") + suppression note; only mapped, suppressed cells reach the accessible list and table.
- [x] 3.7 Provenance copy states join-year mirrored ZIP where a statement covers it, a labeled proxy otherwise (component lede + design/spec).

## 4. Offline basemap (per decision 1.1)

- [x] 4.1 `OFFLINE_STYLE` — a bare inline style (tinted background = water) plus the bundled ZCTA GeoJSON as land fabric; no runtime network style/tile/glyph fetch. (Full water/land context GeoJSON deferred as unnecessary; the tinted-ground + ZCTA-fabric approach is the lightweight offline basemap the design chose.)
- [x] 4.2 `ZipGeographyMap.offline.test.ts` guards the style: no sources, no glyphs/sprite, no `http(s)`/tile-host references.

## 5. Migration & cleanup

- [x] 5.1 `InsightsPage.tsx` now renders `<ZipGeographyMap>` (on-demand `zip_geography`); `zip_attrition` removed from `get_insights` entirely (cut over, decision 1.3).
- [x] 5.2 Attrition documented as one mode of `geographic-membership-insights` in this change's proposal/design/spec (the `zip-attrition-map` change is superseded, not archived).
- [x] 5.3 Removed the `zip_attrition` fn + `ZipAttritionCell` struct/type, deleted `ZipAttritionMap.tsx`, and updated all fixtures (`fakeInsights`, `format.test.ts`, insights tests).

## 6. Verification

- [x] 6.1 `npm run typecheck` clean; `npm test` green (30 tests, 5 files).
- [x] 6.2 `cd src-tauri && cargo test` green (138 tests — aggregations, suppression, capability, out-of-area, per-FY series).
- [ ] 6.3 Real-mirror check: run each mode + a school-family segment against the mirror; confirm suppression, N-in-tooltip, and the out-of-area count are correct. **(Needs the real encrypted mirror — user-run.)**
- [ ] 6.4 Launch the app and verify each mode reads correctly, offline, on the real ZIP map (no smothering), with keyboard + accessible-table parity. **(Needs the real Tauri app; maplibre can't render in the test sandbox.)**
