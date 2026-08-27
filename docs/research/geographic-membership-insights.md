# Geographic & Cohort Membership Insights — Research Note

**File path chosen:** `docs/research/geographic-membership-insights.md`
(No `research/` convention existed; `docs/` already holds `reports/` and `superpowers/plans|specs/`. Created `docs/research/` alongside them. This is a spec input for a future OpenSpec change, not a change proposal itself.)

**Purpose:** Menu of ZIP-map "modes" richer than today's single attrition-rate choropleth, each feasibility-checked against the fields the mart actually carries. Feeds an OpenSpec change proposal.

---

## Step 1 — What data actually exists (cited to code)

### The per-household mart (`Hh`) — `src-tauri/src/insights.rs:697-724`
One row per **Membership Household** (Account rows filtered `Type = 'Member Family'`, `insights.rs:1320`). Fields, all confirmed present:

| Field | Type | Source / meaning |
|---|---|---|
| `account_id`, `name` | id, text | Account Id / Name |
| `is_current`, `is_resigned` | bool | `IsATempleMember__c`, `IsResigned__c` (`:561-562`) |
| `join_fy` | FY int | fiscal year of `Join_Date__c` (`:768`) |
| `cohort_fy` | FY int | `OriginalJoinDate__c` else `join_fy` — the join-era anchor (`:771`) |
| `resign_fy` | FY int | `LastResignDate__c`, nulled for current members (`:774`) |
| `resigned_unknown_date`, `bad_join_date`, `rejoined` | bool | data-quality / rejoin flags (`:775-777`) |
| `tier` | text | `Sub_Type__c` (`:566`) |
| `category` | text | `Member_Category__c` (`:567`) |
| `join_reason` | text | `Join_Reason__c` (`:568`) |
| **`zip`** | 5-digit text | normalized ZIP, latest billing statement w/ Account fallback (see below) |
| `ch[12]` | bool[] | Entry-Job channel flags parsed from `join_reason` — 12 channels at `:47-60` |
| `rs_family`, `ns_family` | bool | ever religious-school / nursery-school family (`:794-795`) |
| `active_rs_students` | int | current religious-school students in household (`:796`) |
| `last_rs_year` | FY int | last year attended religious school (`:797`) |
| `resign_reason_group`, `exit_reason` | text | churn-model group + primary exit reason; `"(not coded)"` for current members (`:798-807`) |

Tenure is derivable (`fy - join_fy + 1`, used in the per-FY table `:890`); it is not stored on `Hh` but on the household-year table `_m_household_fy` (`:817-827`).

### How `zip` is derived — **critical for provenance claims**
- `normalize_zip` (`:753-760`) takes the first 5 digits, accepts bare 5-digit or `ZIP+4`, rejects non-numeric/short.
- Primary source: **latest dated `BillingStatement__c.AddressPostalCode__c`** linked via `Account__c`, one ZIP per household — `billing_statement_zips` (`:1026-1060`), applied in `apply_billing_statement_zips` (`:1062-1070`, called `:1348`).
- Fallback: Account `BillingPostalCode` normalized at load (`:1336`, select at `:1319`).
- **`zip` is a single current snapshot, not history.** The mart stores exactly one ZIP per household (the newest statement, `:1051-1057`). There is no ZIP-as-of-FY. The existing spec already forces this caveat into UI copy ("based on the locally mirrored snapshot rather than an asserted address at exit", `spec.md:22-24`).
- Member-family households only; no raw postal/street ever crosses to the webview (privacy mandate, `spec.md:41-44`).

### Capability gating — `source_capabilities` (`:655-693`)
`zip_attrition` capability is `available` when a usable normalizable ZIP exists from **either** the billing-statement source **or** the Account fallback (`geo_available`, `:666`). Unavailable-reason strings distinguish "no normalizable ZIP" from "source not synced" (`:686-691`). Any new geographic mode rides the **same** capability gate — no new source needed for the modes below.

### How the current attrition cell is built — `zip_attrition()` (`:1701-1714`)
For each of the last 5 completed FYs (`cur-5 .. cur`), per ZIP:
- `start_households` = households `member_in(hh, fy-1)` (active at start of FY; `member_in` at `:1679-1691`)
- `exits` = households with `resign_fy == fy`
- `attrition_rate` = `pct(exits, start_households)` (`:1693-1699`)
- **Suppression: cells with `start_households < 5` are dropped** (`:1712`) — matches `spec.md:27,37-39`.
- Non-NY ZIPs are retained by the backend and filtered/labeled unmapped by the UI (`:1711`, test `:2837-2842`; UI filter `NY_ZCTAS` at `InsightsPage.tsx:574`).

### The Insights payload — `src/api.ts:53-62`
Existing rows/cells and whether each is geographic-capable:

| Insight | Geographic? | Why |
|---|---|---|
| `zip_attrition: ZipAttritionCell[]` (`api.ts:46,61`) | **Yes (only one today)** | carries `zip` |
| `trend`, `year1`, `cohort_matrix`, `channels`, `school`, `reasons`, `multi_job`, `outcome_by_tenure`, `school_progression`, `school_gap`, `dues`, `anchor_type`, `anchor_count` | No | aggregated with no ZIP dimension — but all derive from the same `Hh` mart that **has** `zip`, so any can be re-cut by ZIP in the backend |

So: **every proposed mode below is a new backend aggregation over `Hh` fields that already exist.** No new mirror column is required for modes (a)–(e). The limits are analytical (small-N, snapshot ZIP), not schema.

### Map rendering facts — `ZipAttritionMap.tsx`
- MapLibre-GL, **NY ZCTA GeoJSON** bundled (`ny-zcta-boundaries.json`, `:3`), ~1,826 features → `NY_ZCTAS` set (`:22`).
- Fixed absolute rate ramp gold→amber→red, `SCALE_MAX = 35%` (`:42-49`), painted at 0.82 opacity over the basemap.
- **Note a spec/impl gap** (out of scope here, flag only): `spec.md:45-46,72-74` mandate an *offline packaged* basemap; the component actually loads CARTO Positron from the network (`BASEMAP`, `:55`). Any new mode inherits this and should not make it worse.
- Metro leash ≈ 50-mi Manhattan radius (`METRO`, `:58`).
- Only NY ZCTAs have geometry; suburban/out-of-state members (NJ, CT, Westchester beyond coverage, FL snowbirds) are **unmappable** — ZIP is the only geographic unit the data supports, and even it is NY-bounded.

---

## Step 2 — Which map insights actually drive congregational decisions

Synthesis from membership/donor-CRM geo practice (this is professional synthesis unless a source is named):

- **Density/clustering** drives *where to put programming and satellite gatherings* — congregations and member associations use "where do our people physically cluster" to site neighborhood chavurot, minyanim, and pop-up events. (Synthesis; congregational-planning practice, e.g. URJ/USCJ demographic-study playbooks.)
- **Provenance / inflow over time** answers *where is growth coming from* — the leading edge of a congregation's catchment shifting (e.g. new families arriving from a gentrifying ZIP) is the single most common trigger for targeted outreach mailings and realtor/pre-school partnerships. (Synthesis; nonprofit CRM geo-analytics.)
- **Cohort segmentation overlay** — join-era, dues tier, member category, and **school-family lifecycle** are the segments a synagogue actually acts on; school families in particular are a known retention cliff (they leave when the youngest finishes religious school). The app already models this (`school_gap`, `rs_family`, `last_rs_year`). Mapping *where* the school-family segment lives tells you which neighborhoods to defend before the cliff.
- **Segment/cohort retention by geography** answers *is a place or a cohort the problem* — a ZIP with high attrition of the FY2019 cohort but healthy FY2023 cohort is a different intervention than a uniformly declining ZIP. (Cohort-retention practice.)
- **Net growth/decline** is the one-glance "which neighborhoods are we winning vs losing" board-level map. (Synthesis.)

Not useful / avoid: generic "heatmap of everything", household-penetration vs. Census without a population denominator (we have none — see Step 5), and anything implying an address-level pin (privacy-barred, `spec.md:41-44`).

---

## Step 3 — Prioritized menu of map modes (each feasibility-checked)

Legend for **Data fields required**: ✅ present in `Hh`; ⚠️ present but caveated; ❌ not available.

### (a) Membership Density by ZIP  ·  *decision: where to site neighborhood programming & communications*
- **Question:** Where are our active member households physically clustered right now (or at end of any FY)?
- **Metric + denominator:** count of households with `member_in(hh, fy_end)` per ZIP. A **count**, no denominator (it is the denominator for everything else).
- **Fields:** `zip` ✅, `join_fy`/`resign_fy` via `member_in` ✅.
- **Encoding:** **graduated symbol / proportional dot** centered on each ZIP — *not* choropleth. ZCTA polygons vary wildly in area; a filled choropleth of counts makes big rural ZIPs shout and dense Manhattan ZIPs vanish ("blocks smother the map"). Dot area ∝ household count keeps small dense ZIPs legible.
- **Pitfalls:** count ≠ rate — needs its own encoding/legend, never the red attrition ramp. Suppress ZIP `< 5` households (reuse existing rule, `:1712`). ZIP ≠ neighborhood. Out-of-NY clusters invisible (Step 5).

### (b) New-Member Provenance / Inflow  ·  *decision: where to aim outreach, realtor/preschool partnerships, welcome mailings*
- **Question:** Which ZIPs are new members coming from, and is that shifting year over year?
- **Metric + denominator:** two useful cuts —
  1. **Count:** new joins per ZIP in FY = households with `join_fy == fy` (or `cohort_fy == fy`) per ZIP.
  2. **Share:** that ZIP's joins ÷ all joins that FY (concentration of inflow).
- **Fields:** `zip` ⚠️ (see caveat), `join_fy`/`cohort_fy` ✅.
- **Encoding:** graduated symbol for counts; **small-multiple** (one mini-map per recent FY) or an animated FY selector to show the inflow front moving. Reuse the existing FY selector pattern (`InsightsPage.tsx:571-573`).
- **Pitfalls:** **`zip` is the household's *current* snapshot ZIP, not the ZIP they lived in when they joined** (`:1051-1057`). For recent joins this is a fair proxy; for old cohorts it is where they live *now*. Label it "current ZIP of members who joined in FY" — do not call it "where they joined from". Small-N per ZIP per single FY is worse than density; keep the `<5` suppression, consider bucketing multiple FYs.

### (c) Cohort / Segment Overlay  ·  *decision: target the neighborhoods that matter for a specific segment (esp. school families)*
- **Question:** Where does a chosen segment live — a join era, a dues tier, a member category, or the school-family lifecycle group?
- **Metric + denominator:** same count as (a)/(b) but **filtered** to a segment before aggregating by ZIP. Segments available:
  - **Join era:** `cohort_fy` bucketed (e.g. ≤2010 / 2011–2018 / 2019+) ✅
  - **Dues tier:** `tier` (`Sub_Type__c`) ✅
  - **Member category:** `category` (`Member_Category__c`) ✅
  - **Join channel:** `ch[12]` flags (religious school, nursery, young professionals, HHD, etc., `:47-60`) ✅
  - **School-family lifecycle:** `rs_family`/`ns_family`/`active_rs_students`/`last_rs_year` ✅ — e.g. "active school families" or "school families past the cliff" (mirrors `school_gap`).
- **Encoding:** a **filter** applied to the density/provenance base map (mode stays the same, segment is a dropdown), *not* a separate mode. Optionally a two-color small-multiple to compare two segments side by side.
- **Pitfalls:** filtering shrinks N hard → suppression bites more ZIPs; state clearly "ZIPs with <5 in-segment households hidden". Category/tier are free-text-ish Salesforce picklists; expect a long tail — bucket before mapping. This is the mode that most directly answers the user's "clustered by cohort segmentation".

### (d) Cohort / Segment Retention (or Attrition) by ZIP  ·  *decision: is decline a place problem or a cohort problem?*
- **Question:** Of a given join cohort (or segment), what share in each ZIP is still a member today (or exited)?
- **Metric + denominator:** **retained-to-date %** = of households with `cohort_fy == X` (and optional segment filter) and `zip == Z`, the share with `is_current` true. Denominator = that cohort×ZIP's original size. (Attrition = its complement; the existing per-FY `zip_attrition` is the un-cohorted version.)
- **Fields:** `zip` ✅, `cohort_fy`/`join_fy` ✅, `is_current`/`resign_fy` ✅, segment fields ✅.
- **Encoding:** **choropleth** (it is a rate, 0–100%) with a *different* ramp from attrition (e.g. a sequential green for "retained") to avoid confusion with the red attrition map. Small-multiple by cohort era is the strongest board view.
- **Pitfalls:** **worst small-N of any mode** — cohort × segment × ZIP denominators are tiny, so a single family flips a ZIP from 100% to 50%. Needs a **stricter** suppression floor than 5 (recommend ≥10 for a cohort×ZIP rate) and/or era-bucketing rather than single-FY cohorts. Rate-vs-count confusion is acute here: a bright ZIP may be 3-of-5. Always show N in the tooltip.

### (e) Net Growth / Decline by ZIP  ·  *decision: one-glance "which neighborhoods are we winning vs losing"*
- **Question:** Per ZIP per FY, did we net-gain or net-lose households?
- **Metric + denominator:** net = (joins `join_fy==fy`) − (exits `resign_fy==fy`) per ZIP. Optionally normalized: net ÷ `start_households` for a rate.
- **Fields:** `zip` ✅, `join_fy` ✅, `resign_fy` ✅ (all already used by `zip_attrition`).
- **Encoding:** **diverging choropleth** (blue-gain ↔ red-loss, zero neutral) for the normalized rate, **or** diverging graduated symbol (up/down, size ∝ |net|) for raw counts — the symbol form avoids the area-distortion problem and is honest about magnitude.
- **Pitfalls:** diverging rate + tiny N = extreme values; suppress `start_households < 5`. Net near zero can hide high churn (10 in, 10 out) — pair with a churn tooltip. Snapshot-ZIP caveat applies to the join leg (as in (b)).

---

## Step 4 — Recommended first build (top 3) + interaction model

**Build first, in order:**

1. **Density by ZIP (mode a) with the Cohort/Segment overlay (mode c) as filters.** This is the direct answer to the user's core ask ("where our membership is clustered by zipcode… by cohort segmentation") and the safest statistically — it is a **count**, immune to the rate-instability that plagues small ZIPs. The segment filter reuses `tier`/`category`/`cohort_fy`/`ch`/school-family fields already in the mart, so it is one backend aggregation + a dropdown.
2. **New-Member Provenance (mode b)** with the FY selector already in the UI. Highest decision value for outreach, low incremental cost (same aggregation keyed on `join_fy`). Ship with the explicit "current ZIP, not join-time ZIP" label.
3. **Net Growth/Decline (mode e).** Board-level one-glance map, reuses the exact fields `zip_attrition` already computes. Diverging encoding is the only new UI piece.

Defer **Cohort Retention by ZIP (mode d)** to a second pass despite its high analytical value: its small-N fragility needs a stricter suppression design and careful copy before it is trustworthy on a map.

**Interaction model:**
- **Mode toggle** (Density / Provenance / Net / Attrition) as the primary control — each mode owns its encoding and legend, because **counts and rates must never share a color scale**. Density/provenance = graduated symbol with a neutral sequential legend; attrition/retention = choropleth with rate ramp; net = diverging.
- **Segment = a filter dropdown within a mode**, not a mode. Keep the existing **FY selector** (`InsightsPage.tsx:571-573`) shared across time-varying modes.
- **Suppression rule:** keep the existing `start_households/household count < 5` floor (`:1712`) for counts; raise to **≥10** for any cohort×segment *rate* (mode d). A suppressed ZIP shows no symbol/fill and is excluded from the accessible table (parity with `spec.md:37-39`). Always show N in the tooltip so a bright rate can't mislead.
- **Encoding discipline:** graduated **symbols** (dots) for counts to dodge the ZCTA area-distortion "blocks smother the map" problem; **choropleth** only for rates.

---

## Step 5 — Not feasible with current data / what new columns would unlock

**Not feasible now (hard limits):**
- **Household penetration / market share** ("what % of Jewish households in ZIP are members") — no population/Census denominator anywhere in the mirror. Needs an external ZIP-household reference asset.
- **Point/address maps, pins, drive-time, distance-to-temple** — no lat/long, no geocoder, offline mandate, and aggregate-only privacy bar (`spec.md:41-44`). ZIP is the finest legal unit.
- **Historical ZIP (ZIP-at-join, ZIP-at-exit, migration paths)** — the mart stores **one current snapshot ZIP** per household (`:1051-1057`). "Where members joined from" and any member-migration map are approximations, not facts.
- **Sub-ZIP neighborhoods** — ZCTA is the only geometry; no NYC-neighborhood boundaries.
- **Suburban / out-of-state members** (NJ, CT, outer Westchester, FL) — real members in the data but **no geometry** (boundaries are NY ZCTAs only, `ZipAttritionMap.tsx:22`). They silently drop off every map. Worth a "N members not shown (outside mapped area)" counter.
- **Non-member households** — mart is `Type = 'Member Family'` only (`:1320`); prospects/lapsed non-members can't be mapped.

**New mirror columns / computation that would unlock more:**
- **Per-FY ZIP from dated billing statements.** `billing_statement_zips` already reads *dated* statements but keeps only the latest (`:1051-1057`). Computing the ZIP *as of each FY* would make provenance and migration real rather than snapshot-proxied — bounded by statement coverage (FY2023+ per project data facts).
- **A packaged Census ZIP-household (and, if licensable, Jewish-household) count asset** → true penetration/market-share choropleths.
- **Multi-state ZCTA boundaries** (NJ/CT/NY tri-state) → map the suburban members who are currently invisible.
- **Address-effective-date history** (if Salesforce carries it) → member relocation flows.

---

## One-line feasibility bottom line
Every count/segment/net mode is a straightforward re-aggregation of `Hh` fields that already exist — the binding constraints are **(1) `zip` is a single current snapshot, not history**, so "where members came from" is a labeled proxy, and **(2) small-N per ZIP** forces count-based encodings and strict suppression, with cohort×segment *rates* (mode d) the most fragile.
