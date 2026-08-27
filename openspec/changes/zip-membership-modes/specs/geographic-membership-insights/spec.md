## ADDED Requirements

### Requirement: Mode-driven ZIP membership map
The system SHALL present ZIP-level membership geography through selectable modes — Density, Provenance, Net Change, and Attrition — where each mode determines the measure, encoding, and legend. Count-based modes and rate-based modes SHALL NOT share a color scale or legend.

#### Scenario: A default mode is shown
- **WHEN** staff open the geographic membership view for a selected fiscal year
- **THEN** a mode control is presented and Density is the default mode, shown as household counts

#### Scenario: Switching between a count mode and a rate mode reencodes
- **WHEN** staff switch from a count mode to a rate mode
- **THEN** the legend and encoding change so that counts and rates are never displayed on the same scale

### Requirement: Membership density by ZIP
Density mode SHALL report, for each eligible ZIP, the number of Membership Households active at the end of the selected fiscal year, encoded as a graduated symbol sized by that count rather than a filled choropleth.

#### Scenario: Active households drive symbol size
- **WHEN** a ZIP has 336 Membership Households active at the end of the selected fiscal year
- **THEN** Density mode reports 336 for that ZIP and sizes its map symbol by that count, not as a filled polygon

### Requirement: New-member provenance by ZIP
Provenance mode SHALL report, for each eligible ZIP, the number of Membership Households whose join fiscal year equals the selected fiscal year, placing each household at its ZIP resolved as of that fiscal year. Where a billing statement covers the join fiscal year the placement is the household's join-year mirrored ZIP; where no statement covers it the placement is a labeled proxy (the earliest known mirrored ZIP), and the view SHALL distinguish the two so a proxy is never presented as an asserted join-time address.

#### Scenario: New joins are counted per ZIP at their join-year ZIP
- **WHEN** 12 Membership Households in ZIP 10024 have a join fiscal year equal to the selected fiscal year and a billing statement covering that year
- **THEN** Provenance mode reports 12 new members for ZIP 10024 as their join-year mirrored ZIP

#### Scenario: Pre-coverage provenance is labeled a proxy, not a join-time fact
- **WHEN** a Membership Household joined before billing-statement coverage began and is shown in Provenance mode
- **THEN** the view places it at its earliest known mirrored ZIP and labels that placement a proxy, not the ZIP it joined from

### Requirement: Fiscal-year ZIP resolution
The system SHALL resolve each Membership Household's ZIP as of a given fiscal year from its dated billing statements: the ZIP of the latest statement dated on or before the end of that fiscal year; if no statement covers that year, the ZIP of the household's earliest known statement; if the household has no usable statement, the Account postal fallback. Every mode SHALL place households by this fiscal-year-resolved ZIP, and the resolution SHALL never read a bill-to-other identifier as household geography.

#### Scenario: Household is placed by the statement in force that year
- **WHEN** a household filed a statement in ZIP 10023 in one fiscal year and later moved and filed in ZIP 10024
- **THEN** for the earlier fiscal year the household resolves to 10023 and for the later fiscal year it resolves to 10024

#### Scenario: Years before first coverage fall back to earliest known ZIP
- **WHEN** a fiscal year precedes a household's first billing statement
- **THEN** the household resolves to its earliest known statement ZIP, or the Account postal fallback if it has no statement

### Requirement: Net membership change by ZIP
Net Change mode SHALL report, for each eligible ZIP and selected fiscal year, joins minus exits — Membership Households whose join fiscal year equals the year, minus completed Membership Spells that end in the year — encoded on a diverging scale with a neutral zero midpoint.

#### Scenario: Net gain is reported on the gain side
- **WHEN** a ZIP has 8 joins and 3 exits in the selected fiscal year
- **THEN** Net Change reports +5 for that ZIP on the gain side of the diverging scale

#### Scenario: Zero net does not hide churn
- **WHEN** a ZIP has equal joins and exits in the selected fiscal year
- **THEN** Net Change reports 0 at the neutral midpoint, and the tooltip still exposes the underlying join and exit counts

### Requirement: Attrition mode preserves resign-rate behavior
Attrition mode SHALL report, for each eligible ZIP and selected fiscal year, completed Membership Spell exits divided by the starting Membership Household count, encoded as a rate choropleth, preserving the existing ZIP attrition behavior as one mode of this view.

#### Scenario: Exit rate uses the fiscal-year starting population
- **WHEN** a ZIP has 40 Membership Households active at the beginning of the selected fiscal year and 4 completed Membership Spells end during it
- **THEN** Attrition mode reports 40 starting households, 4 exits, and a 10.0 percent rate for that ZIP as a choropleth

### Requirement: In-mode segment filtering
Each mode SHALL support filtering the mapped population by a single segment before aggregation. Segments SHALL include join era (bucketed cohort fiscal year), dues Tier, Member Category, Entry Job channel, and school-family lifecycle. Applying a segment SHALL recompute the per-ZIP aggregates over only the Membership Households in that segment.

#### Scenario: Segment narrows the mapped population
- **WHEN** staff filter Density mode to the school-family lifecycle segment for active religious-school families
- **THEN** each ZIP's reported count reflects only Membership Households with active religious-school students

#### Scenario: Segment filtering can trigger suppression
- **WHEN** a segment filter reduces a ZIP below the active mode's suppression threshold
- **THEN** that ZIP is suppressed and the view states that in-segment ZIPs below the threshold are hidden

### Requirement: Small-ZIP suppression and count-with-denominator honesty
The system SHALL exclude, before returning aggregates to the webview, any ZIP with fewer than five Membership Households in a count mode, and any ZIP with fewer than ten Membership Households in a rate mode. Every mapped ZIP tooltip SHALL expose the underlying household count so a rate cannot be read without its denominator. Suppressed ZIPs SHALL be omitted from both the map and the accessible table.

#### Scenario: Sparse ZIP is suppressed in a count mode
- **WHEN** a ZIP has four Membership Households in a count mode
- **THEN** that ZIP's value and geometry are not returned or displayed

#### Scenario: Rate is never shown without its N
- **WHEN** a rate-mode ZIP is displayed
- **THEN** its tooltip shows both the rate and the Membership Household count it is computed from

### Requirement: Out-of-area membership is counted, not silently dropped
When eligible Membership Households have a normalizable ZIP that is absent from the packaged New York boundary asset, the system SHALL report the count of such households as not shown because they are outside the mapped area, rather than dropping them without acknowledgement or converting them to a different geography.

#### Scenario: Out-of-area members are surfaced as a count
- **WHEN** 37 eligible Membership Households have normalizable ZIPs outside the packaged New York boundary asset
- **THEN** the view reports that 37 members are not shown because they are outside the mapped area

### Requirement: Aggregate-only, capability-gated, offline geographic access
The system SHALL make the geographic membership view unavailable when neither the usable local billing-statement postal source nor the local Account postal fallback is available. Across every mode and segment it SHALL return only ZIP-level aggregates, without household names, raw postal codes, street addresses, bill-to-other identifiers, coordinates, or pins, and SHALL render against the packaged local boundary asset without a third-party map, tile, geocoding, or runtime network service. It SHALL provide an accessible table containing the same aggregates as the active mode and segment.

#### Scenario: Postal source is unavailable
- **WHEN** both local postal sources are missing, withheld, or contain no normalizable ZIP for the relevant population
- **THEN** the view shows a geographic-source unavailable state and does not render a zero-valued map

#### Scenario: Aggregate access does not reveal household identity
- **WHEN** staff load any mode and segment
- **THEN** the response contains only ZIP-level aggregates for that mode and segment, without household names, raw postal codes, street addresses, coordinates, or pins

#### Scenario: Map renders without a network
- **WHEN** the desktop app has no network connection and eligible aggregates exist
- **THEN** the map remains renderable from packaged application assets
