## Context

The Insights ZIP map today renders a single measure — resign rate per ZIP per fiscal year — from `zip_attrition: ZipAttritionCell[]` inside the one `get_insights` payload, drawn as a filled choropleth in `ZipAttritionMap.tsx`. The per-household mart (`Hh`, `src-tauri/src/insights.rs:697-724`) already carries each Membership Household's `zip`, `join_fy`, `cohort_fy`, `resign_fy`, `is_current`, `tier`, `category`, Entry Job channel flags, and school-family fields, so density, provenance, net change, and segment cuts are all re-aggregations of data already mirrored. Full feasibility analysis: `docs/research/geographic-membership-insights.md`. Constraints are unchanged: read-only Salesforce, encrypted local mirror only, untrusted webview reached only through fixed Rust commands, aggregate-only geography (no names, raw postal, coordinates, or pins), and the attrition spec's offline-map mandate.

## Goals / Non-Goals

**Goals:**
- Turn the single-measure map into a mode-driven view (Density, Provenance, Net Change, Attrition), each with its own honest encoding and legend.
- Support in-mode segment filtering (join era, Tier, Member Category, Entry Job channel, school-family lifecycle) computed server-side.
- Keep all raw suppression server-side; never ship a sparse ZIP, a rate without its N, or an out-of-area household silently dropped.
- Preserve the existing attrition behavior exactly as one mode.

**Non-Goals:**
- Cohort-retention-by-ZIP (retained-to-date % of a join cohort per ZIP) — deferred; needs stricter suppression design.
- Address pins, drive-time, or population-penetration — not supported by the mirror (see research Step 5).
- Migration *flow* visualization (origin→destination arcs) and pre-FY2023 historical ZIP — out of scope here. Per-FY ZIP resolution (decision 4) makes year-over-year placement accurate within statement coverage (FY2023+), but rendering migration paths and recovering pre-coverage addresses are not attempted.
- Mapping suburban/out-of-state members — no boundary geometry; they get a counter only.

## Decisions

### 1. A dedicated on-demand geography command, not a precomputed cube in `get_insights`
Add a fixed Rust command `zip_geography(fiscal_year, mode, segment)` returning suppressed per-ZIP aggregates for that mode+segment, plus the out-of-area count and per-ZIP N. `get_insights` stops carrying `zip_attrition`.
- **Why:** Precomputing every mode × fiscal year × segment cut into the main payload is a combinatorial blow-up and bloats the load that `optimize-insights-load` is separately trying to shrink. The mart is a few thousand rows in local SQLite, so an on-demand aggregate is fast, and it keeps suppression on the Rust side of the trust boundary.
- **Alternative considered:** extend `get_insights` with a bounded precomputed cube (era/tier/category/channel/school breakdowns per ZIP). Rejected: payload bloat, rigid segment set, and it recomputes on every insights build even when the map is unopened.

### 2. Encoding by measure type — symbols for counts, choropleth for rates, diverging for net
Density and Provenance are counts → **graduated symbols** (dot area ∝ count) centered on each ZIP's centroid. Attrition is a rate → **choropleth** (existing gold→red ramp). Net Change → **diverging** scale, neutral zero.
- **Why:** ZCTA polygons vary enormously in area; a filled choropleth of counts makes big low-density ZIPs shout and dense Manhattan ZIPs vanish (the "blocks smother the map" problem the user already hit). Symbols keep small dense ZIPs legible and make counts honest. Counts and rates therefore never share a color scale (a spec requirement).
- **Implementation note:** ZIP centroids are computed once from the packaged boundary asset (centroid of each feature's largest ring), reused by the symbol layers.

### 3. Server-side suppression with two thresholds and mandatory N
`< 5` Membership Households suppresses a ZIP in count modes; `< 10` in rate modes. Suppressed ZIPs are dropped from the response entirely (map and table). Every returned cell includes its household N; the tooltip always shows it.
- **Why:** small per-ZIP denominators make a single family swing a rate; the stricter rate floor and always-visible N prevent "3-of-5 = 60%" from reading as a real signal. Matches and tightens the existing `< 5` rule (`insights.rs:1712`).

### 4. Per-FY ZIP time series, bounded FY2023+ (revised — supersedes the single-snapshot design)
Instead of collapsing each household to one latest ZIP, derive its ZIP **as of the display fiscal year** from its dated `BillingStatement__c` rows: `zip_as_of(fy)` = the ZIP of the latest statement dated ≤ FY-end; if the household has no statement covering that year (its first statement is later), fall back to its **earliest known statement ZIP**; if it has no statement at all, the Account `BillingPostalCode` snapshot. Every mode places households by `zip_as_of(display_fy)`; Provenance counts `join_fy == fy` at `zip_as_of(fy)`.
- **Why:** the raw billing table already carries a dated, postal-coded statement per paid year (`billing_statement_zips`, `insights.rs:1026-1060`, reads them all but keeps only the latest). Using them per-FY makes current-era geography accurate (households move between FY2023–FY2026) and makes **provenance real for households that joined FY2023+** — their join-year statement exists.
- **Hard bound (data honesty):** billing statements exist **FY2023 onward** only (project data facts). So join-time ZIP is a *fact* only when `join_fy ≥ 2023`; for older cohorts, and for any display year before a household's first statement, the resolver returns the earliest-known (or Account) ZIP, which the UI must label a **proxy**, not an asserted historical address. Migration is observable only across the FY2023–FY2026 window.
- **Alternative considered:** keep the single current snapshot and label all provenance "current ZIP." Rejected: it discards recoverable per-year truth for the recent window that the product owner explicitly wants; the honesty caveat is narrowed to pre-FY2023 rather than applied blanket.

### 5. Offline packaged basemap (reconciling a spec/implementation conflict)
Render all modes against **packaged local assets** — a bare tinted style plus a bundled water/land/boundary context GeoJSON — with no third-party tile, glyph, or style fetch at runtime.
- **Why:** the carried-forward spec requires offline rendering with no runtime network map service. The current `ZipAttritionMap.tsx` loads a **network** CARTO Positron style (`BASEMAP`), which already violates that requirement; this change must satisfy the spec, not entrench the violation.
- **This reverses an earlier in-session UI choice** (network CARTO basemap) in favor of the written offline requirement. **Confirmed by the product owner** (decision 1.1): offline wins.
- **Alternative considered:** bundle full offline vector tiles for the metro. Rejected for size/complexity; the bare-style + context-GeoJSON approach is lightweight and sufficient for a ZIP choropleth/symbol view.

### 6. Segment cardinality — per-year join era, top-6 picklists
Join era is offered as a **per join fiscal year** segment (no era buckets); sparse ZIP × year cells simply fall under the suppression floor and drop, which is accepted. Tier and Member Category collapse to their **top 6 values by household count + "Other"** before reaching the webview. Entry Job channels are the fixed 12 flags; school-family lifecycle is a small fixed set.
- **Why:** the product owner preferred per-year granularity over coarse era buckets; suppression already protects against reading a sparse cell. Top-6 keeps the segment dropdown legible against free-text-ish picklists.

## Risks / Trade-offs

- **Proxy ZIP misread as asserted history** → provenance is a *fact* only for FY2023+ joins (statement covers the join year); for older cohorts / pre-first-statement years the resolver returns the earliest-known ZIP, which the UI labels a proxy. Keep the existing "not an asserted address at exit" copy.
- **Small-N instability, especially segment × ZIP** → two-tier suppression, N always in tooltip, counts encoded as symbols; bucket join eras (e.g. ≤2010 / 2011–2018 / 2019+) rather than single-FY cohorts for segment cuts.
- **Segment cardinality explosion** (free-text-ish Tier/Category picklists) → bucket to a bounded set (top-N + "Other") before mapping; Entry Job channels are the fixed 12 flags.
- **Out-of-area members invisible** → explicit "N not shown (outside mapped area)" counter, never a silent drop.
- **Offline basemap loses street context** → accept per spec; the context GeoJSON (water, boroughs, major roads if bundled) gives enough orientation for ZIP-level reading.
- **New command changes the load model** → geography loads lazily when the view opens, which also keeps it off the `get_insights` critical path.

## Migration Plan

1. Add the per-household ZIP time series + `zip_as_of(fy)` resolver over dated `BillingStatement__c`; keep its privacy filters.
2. Add `zip_geography` command + Rust aggregations over `Hh` using `zip_as_of`; keep the existing attrition math intact as the Attrition mode path.
3. Add per-ZIP centroid derivation and graduated-symbol + diverging layers to `ZipAttritionMap.tsx`; add mode toggle, segment dropdown, per-mode legends, out-of-area counter; update the accessible table to follow the active mode/segment.
4. Swap the offline packaged basemap in for the network CARTO style.
5. Remove `zip_attrition` from `get_insights` (cut over immediately, decision 1.3); update `InsightsPage.tsx` and tests.
- **Rollback:** `zip_geography` is additive; the git history retains the prior single-mode attrition path if a revert is needed.

## Open Questions (resolved)

- **Offline basemap sign-off** → **Resolved:** offline packaged assets (decision 5 / 1.1).
- **Segment granularity** → **Resolved:** per-year join era, top-6 Tier/Category + Other (decision 6 / 1.2).
- **`get_insights` deprecation of `zip_attrition`** → **Resolved:** cut over immediately (decision 1.3).
- **ZIP model** → **Resolved:** per-FY ZIP time series bounded FY2023+, earliest-statement proxy before (decision 4, revised / 1.4).
