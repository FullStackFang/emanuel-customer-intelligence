## Why

The Insights ZIP map answers only one question — "what share of each ZIP's households resigned this year" — which reads as a thin, isolated metric ("10021 · 11.6% attrition · 39 exits from 336 households") that does not tell staff where members actually cluster, where new members come from, or which neighborhoods are growing versus declining. The mart already stores each household's ZIP alongside its join era, tier, category, join channel, and school-family lifecycle, so far richer geographic insight is one backend re-aggregation away, with no new mirrored data.

## What Changes

- The ZIP map becomes **mode-driven**. A mode toggle selects what the map shows; each mode owns its own encoding and legend (counts and rates never share a color scale):
  - **Density** — active member households per ZIP (a count), as graduated symbols sized by household count. The direct answer to "where is our membership clustered."
  - **Provenance / Inflow** — new joins per ZIP for the selected fiscal year (a count), graduated symbols, placed at each household's **join-year mirrored ZIP** where a billing statement covers the join year (FY2023+), and a labeled earliest-known-ZIP proxy for older cohorts.
  - **Net growth / decline** — joins minus exits per ZIP per fiscal year, diverging encoding (gain vs. loss).
  - **Attrition** — the existing resign-rate choropleth, retained as one mode.
- A **segment filter** applies within any mode: join fiscal year (per-year `cohort_fy`), dues tier and member category (top 6 + "Other"), join channel, and school-family lifecycle (e.g. active school families, families past the religious-school cliff).
- A shared **fiscal-year selector** drives the time-varying modes (Provenance, Net, Attrition).
- **Suppression and honesty rules**: hide ZIPs with fewer than 5 households for count modes and fewer than 10 for any rate; always show N in the tooltip so a bright rate cannot mislead; suppressed ZIPs are also excluded from the accessible table. A counter reports **"N members not shown (outside mapped area)"** for suburban and out-of-state members with no NY ZCTA geometry.
- Cohort-retention-by-ZIP (retained-to-date % of a join cohort per ZIP) is **explicitly deferred** to a follow-up change; its small-N fragility needs a stricter suppression design before it is trustworthy on a map.

## Capabilities

### New Capabilities
- `geographic-membership-insights`: A mode-driven ZIP-level map of member households — density, new-member provenance, net growth/decline, and attrition — with in-mode segment filtering, encoding discipline (symbols for counts, choropleth for rates, diverging for net), small-ZIP suppression, an out-of-area counter, and the same aggregate-only, offline, capability-gated privacy contract as the existing ZIP attrition view. Attrition is subsumed as one mode of this capability.

### Modified Capabilities
<!-- None. The single-metric attrition view lives only in the in-flight `zip-attrition-map` change, not yet an archived spec; this capability supersedes it (see Impact). -->

## Impact

- **Supersedes/extends** the in-flight `zip-attrition-map` change: attrition becomes one mode of `geographic-membership-insights` rather than the whole view. That change's privacy, suppression, and offline-basemap requirements are carried forward, not weakened. (Note: the current `ZipAttritionMap.tsx` loads a network CARTO basemap while the attrition spec mandates an offline packaged basemap — this change must not make that gap worse.)
- **Backend (`src-tauri/src/insights.rs`)**: new per-ZIP-per-FY-per-segment aggregations over the existing `Hh` mart (`zip`, `join_fy`, `cohort_fy`, `resign_fy`, `is_current`, `tier`, `category`, join-channel flags, school-family fields). No new mirrored Salesforce columns. Rides the existing `zip_attrition` capability gate (`geo_available`).
- **API (`src/api.ts`)**: a new geographic payload shape (per-ZIP cells carrying counts/net per mode and segment) replacing or extending `zip_attrition`; command surface unchanged (still a fixed Rust command, no SQL/token crosses to the webview).
- **Frontend (`src/pages/insights/ZipAttritionMap.tsx`, `InsightsPage.tsx`)**: mode toggle, segment dropdown, per-mode legends and encodings (graduated symbols added alongside the existing choropleth), out-of-area counter, and updated accessible table/labels.
- **Privacy/audit**: unchanged boundary — aggregate ZIP cells only, member-family households only, no raw postal/address, no household-level pins.
- **Data honesty**: households are placed by their ZIP *resolved as of the display fiscal year* from dated billing statements (Account fallback). Join-time provenance is a fact only for households that joined within statement coverage (FY2023+); before that it is a labeled earliest-known-ZIP proxy. UI copy must distinguish the two.
- **Tests**: extend `InsightsPage.test.tsx` and Rust insights tests for the new aggregations, suppression thresholds, and mode/segment behavior. Full rationale in `docs/research/geographic-membership-insights.md`.
