# membership-lifecycle-insights Specification

## Purpose
TBD - created by archiving change membership-lifecycle-insights. Update Purpose after archive.
## Requirements
### Requirement: Consistent household-year analysis
The system SHALL derive all membership lifecycle aggregates from one household-by-fiscal-year analytical dataset using Membership Household as the unit and June 1 through May 31 as the fiscal year.

#### Scenario: Views use the same analytical population
- **WHEN** staff compare membership, join-reason, renewal, engagement, and exit views for a fiscal year
- **THEN** each view uses the same household-year population and fiscal-year boundaries

#### Scenario: Rejoined household has separate spell context
- **WHEN** a household resigned and later rejoined
- **THEN** the current spell is active without treating the prior spell's resignation date or reason as a current exit

### Requirement: Transactional analytical rebuild
The system SHALL rebuild the household and household-year analytical datasets atomically and SHALL retain the previous valid datasets when rebuilding fails.

#### Scenario: Successful rebuild
- **WHEN** all available source rows normalize and insert successfully
- **THEN** both analytical datasets and their freshness metadata become visible together

#### Scenario: Failed rebuild
- **WHEN** any analytical rebuild step fails
- **THEN** the previous datasets remain readable and Insights displays their age and the rebuild error

### Requirement: Source capability handling
The system SHALL independently report Membership, Renewal, School, and Committee source capabilities and SHALL NOT interpret an unavailable optional source as household behavior.

#### Scenario: Billing statements are not synced
- **WHEN** `BillingStatementLine__c` is synced but `BillingStatement__c` is not synced
- **THEN** Renewal content is unavailable and the UI identifies both required billing objects without labeling households as unbilled or at risk

#### Scenario: Committee source is unavailable
- **WHEN** `Committee_Membership__c` is not synced
- **THEN** Committee-dependent content is unavailable while Membership, School, and eligible Renewal content remain usable

### Requirement: Multi-label Exit Outcomes
The system SHALL preserve every recognized resignation-reason label and SHALL assign one primary Exit Outcome using Structural Exit, Conversion Loss, Addressable Churn, then Administrative or Unknown precedence.

#### Scenario: Exit has structural and payment reasons
- **WHEN** a resignation reason contains both moved and non-payment evidence
- **THEN** the primary outcome is Structural Exit and both moved and non-payment labels remain available for analysis

#### Scenario: Exit reason is absent
- **WHEN** a completed membership spell has no classifiable resignation reason
- **THEN** the primary outcome is Administrative or Unknown Exit rather than Addressable Churn

### Requirement: Join-reason analysis
The system SHALL treat stated joining reasons as multi-label join reasons and SHALL present their retention and exit associations without claiming they reveal complete or causal member intent.

#### Scenario: Household states multiple joining reasons
- **WHEN** a household has more than one recognized joining reason
- **THEN** it contributes to each applicable join-reason analysis and to the multi-reason comparison

#### Scenario: Staff view join-reason outcomes
- **WHEN** staff open the Join reasons view
- **THEN** they can compare retention by join reason with proxy language

### Requirement: Engagement-driver analysis
The system SHALL derive distinct dues, Nursery School, Religious School, and Committee engagement drivers and SHALL report retention by engagement-driver type and driver count.

#### Scenario: Confirmed school enrollment
- **WHEN** a household has confirmed Religious School enrollment in a fiscal year
- **THEN** that fiscal year includes a Religious School engagement driver

#### Scenario: Withdrawn school enrollment
- **WHEN** a household has only withdrawn enrollment in a fiscal year
- **THEN** the withdrawal remains reportable but does not create a confirmed school engagement driver

#### Scenario: Far-future committee end date
- **WHEN** a committee membership has a known placeholder far-future end date
- **THEN** the end date is treated as open-ended and current activity follows `IsActive__c`

### Requirement: Membership-dues evidence
The system SHALL classify qualifying membership-dues lines separately from security fees, gifts, tuition, events, sales, and other non-dues products, and SHALL treat missing billing coverage as unknown rather than non-renewal. Eventual dues settlement SHALL be derived only from the qualifying membership-dues lines, not from statement totals that also cover non-dues products.

#### Scenario: Security fee line
- **WHEN** a billing line is a security fee rather than membership dues
- **THEN** it is excluded from membership-dues billed and settlement measures

#### Scenario: Active household has no qualifying dues line
- **WHEN** an active household has no qualifying dues line for a fiscal year
- **THEN** it is reported as billing coverage missing and not as churn or non-renewal evidence

#### Scenario: Historical settlement is displayed
- **WHEN** historical balance or received amount comes from a current/final mirror snapshot
- **THEN** the UI labels the measure as eventual settlement rather than an as-of historical state

#### Scenario: Settlement excludes non-dues charges on the same statement
- **WHEN** a statement carries both membership-dues lines and non-dues charges such as security fees or tuition
- **THEN** the eventual settlement reflects only the dues lines' balance and received amounts, not the statement's combined total

### Requirement: Analytical views
The system SHALL provide Overview, Join reasons, Engagement & Renewal, Financials, and Attrition & Risk views and SHALL compose every aggregate section and its chart visuals in a measurable report surface before PDF capture, regardless of the selected screen tab. The report surface SHALL exclude the named household Watch List.

#### Scenario: Staff navigate Insights
- **WHEN** staff select an Insights tab
- **THEN** the selected analytical view is displayed without changing the underlying fiscal-year definitions

#### Scenario: Staff create a PDF from a hidden-tab view
- **WHEN** staff export the Insights report while Join reasons, Engagement & Renewal, or Attrition & Risk is not the selected screen tab
- **THEN** the system lays out every aggregate report card and chart at non-zero printable dimensions before creating the PDF

#### Scenario: Staff create a PDF
- **WHEN** staff export the Insights report from any selected tab
- **THEN** the PDF contains every aggregate section and its chart visuals and no named household Watch List

#### Scenario: Report layout does not become ready
- **WHEN** an aggregate chart or report section has not reached a measurable printable layout before the export readiness timeout
- **THEN** the system reports that the PDF could not be rendered and does not report a successful export path

### Requirement: Freshness and traceability
The system SHALL display the analytical build time, source sync time, available capabilities, and any unavailable reasons.

#### Scenario: Mart is older than source sync
- **WHEN** a source has been synced after the latest successful analytical build
- **THEN** Insights identifies that the analysis is stale and offers rebuilding

### Requirement: Aggregate and named export separation
The system SHALL keep aggregate Insights access separate from named household access and SHALL preserve restricted export paths and audit behavior.

#### Scenario: Aggregate view loads
- **WHEN** staff load aggregate Insights
- **THEN** no household names are returned and no named-access audit event is required

#### Scenario: Named list is requested
- **WHEN** staff explicitly load or export the named Watch List
- **THEN** the action is audited and exports remain within the approved application export directory

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

