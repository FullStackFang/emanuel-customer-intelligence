## ADDED Requirements

### Requirement: Membership-age composition of current members
The system SHALL report the composition of current Membership Households by membership age — the number of fiscal years between the start of the household's current Membership Spell and the in-progress fiscal year — in five fixed bands: New (0–1 years), Establishing (2–4), Settled (5–9), Long-standing (10–24), and Legacy (25 or more). Every band SHALL be reported in that order even when empty, with its household count and its share of all current Membership Households. Households whose join date is unusable SHALL be excluded from every band and their count reported separately, not silently dropped.

#### Scenario: Households fall into bands by membership age
- **WHEN** the in-progress fiscal year is FY2026 and eight current Membership Households joined in FY2026, FY2025, FY2024, FY2021, FY2017, FY2016, FY2002, and FY1999 (membership ages 0, 1, 2, 5, 9, 10, 24, and 27)
- **THEN** New reports 2, Establishing 1, Settled 2, Long-standing 2, and Legacy 1, and the five shares sum to 100 percent of dated households

#### Scenario: Band edges are inclusive of their stated years
- **WHEN** a household's membership age is exactly 1, 2, 4, 5, 9, 10, 24, or 25 years
- **THEN** it is assigned to New, Establishing, Establishing, Settled, Settled, Long-standing, Long-standing, and Legacy respectively

#### Scenario: Undated joins are reported, not hidden
- **WHEN** three current Membership Households have no usable join date
- **THEN** no band includes them, the band counts sum to the current household count minus three, and the view states that three households have no usable join date and are not shown

### Requirement: Joined-versus-survivors cohort view
The Overview SHALL show, for each join cohort from FY2010 through the in-progress fiscal year, the number of Membership Households that joined in that fiscal year beside the number of those still current Membership Households, and SHALL show current households that joined before FY2010 as a single earlier group with survivors only, stating that join counts before FY2010 are not shown because departures before then are not reliably recorded. The view SHALL state, for cohorts FY2010 through the last complete fiscal year, the total that joined and the count and share still members.

#### Scenario: A cohort shows joined beside still here
- **WHEN** 320 Membership Households joined in FY2019 and 190 of them are current members
- **THEN** the FY2019 cohort shows 320 joined beside 190 still here

#### Scenario: Survivors never exceed joiners
- **WHEN** any cohort from FY2010 onward is displayed
- **THEN** its still-here count is less than or equal to its joined count

#### Scenario: Pre-FY2010 cohorts are one survivors-only group
- **WHEN** current Membership Households joined in FY1998 and FY2007
- **THEN** they are counted together in a single "Before FY2010" group that shows a still-here count and no joined count, with the stated reason

### Requirement: Financial value by membership age is aggregate-only
The Financials view SHALL report, for each membership-age band over current Membership Households, the household count, total cash received in the latest complete fiscal year, the band's share of all such households, and the band's share of all such cash. The per-household average SHALL be reported only for a band with at least ten households and SHALL otherwise be withheld before the data reaches the webview. The system SHALL NOT return cash figures for any grouping smaller than a membership-age band, and in particular SHALL NOT return per-join-year cash figures.

#### Scenario: Shares expose who carries the base
- **WHEN** the Legacy band holds 22 percent of current Membership Households and received 35 percent of the latest complete year's cash
- **THEN** the view reports both shares for Legacy and its per-household average, and the takeaway names Legacy as the band whose share of money most exceeds its share of households

#### Scenario: Small band withholds the per-household figure
- **WHEN** a membership-age band holds fewer than ten current Membership Households
- **THEN** its household count and shares are reported, its per-household average is absent from the payload, and the view shows the average as withheld for fewer than ten households

#### Scenario: Per-join-year cash never reaches the webview
- **WHEN** staff load the Financials view
- **THEN** the payload contains cash figures only at the band level and the year level, and no cash figure keyed to an individual join fiscal year

#### Scenario: Financials unavailable
- **WHEN** the billing capability is unavailable or no member cash exists in the latest complete fiscal year
- **THEN** no membership-age value chart is rendered and the existing Financials unavailable state is shown

### Requirement: Composition views survive read-model caching
The system SHALL serve the membership-age and financial-by-age figures from a rebuilt read model whenever the cached read model predates their introduction, rather than serving the new fields as absent.

#### Scenario: Cached read model predates the fields
- **WHEN** a read model cached before this change is present on first load after upgrade
- **THEN** the system rebuilds the read model and the membership-age views are populated
