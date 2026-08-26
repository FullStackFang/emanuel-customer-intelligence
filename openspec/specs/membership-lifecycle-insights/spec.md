# membership-lifecycle-insights Specification

## Purpose
TBD - created by archiving change membership-lifecycle-insights. Update Purpose after archive.
## Requirements
### Requirement: Consistent household-year analysis
The system SHALL derive all membership lifecycle aggregates from one household-by-fiscal-year analytical dataset using Membership Household as the unit and June 1 through May 31 as the fiscal year.

#### Scenario: Views use the same analytical population
- **WHEN** staff compare membership, Entry Job, renewal, engagement, and exit views for a fiscal year
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

### Requirement: Entry Job analysis
The system SHALL treat stated joining reasons as multi-label Entry Jobs and SHALL present their retention and exit associations without claiming they reveal complete or causal member intent.

#### Scenario: Household states multiple joining reasons
- **WHEN** a household has more than one recognized joining reason
- **THEN** it contributes to each applicable Entry Job analysis and to the multi-job comparison

#### Scenario: Staff view Entry Job outcomes
- **WHEN** staff open the Jobs view
- **THEN** they can compare retention and Exit Outcomes by Entry Job and tenure with proxy language

### Requirement: Relationship Anchor analysis
The system SHALL derive distinct dues, Nursery School, Religious School, and Committee Relationship Anchors and SHALL report retention by anchor type and anchor count.

#### Scenario: Confirmed school enrollment
- **WHEN** a household has confirmed Religious School enrollment in a fiscal year
- **THEN** that fiscal year includes a Religious School Relationship Anchor

#### Scenario: Withdrawn school enrollment
- **WHEN** a household has only withdrawn enrollment in a fiscal year
- **THEN** the withdrawal remains reportable but does not create a confirmed school Relationship Anchor

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

### Requirement: Four analytical views
The system SHALL provide Overview, Jobs, Renewal & Engagement, and Risk views and SHALL compose every aggregate section and its chart visuals in a measurable report surface before PDF capture, regardless of the selected screen tab. The report surface SHALL exclude the named household Watch List.

#### Scenario: Staff navigate Insights
- **WHEN** staff select an Insights tab
- **THEN** the selected analytical view is displayed without changing the underlying fiscal-year definitions

#### Scenario: Staff create a PDF from a hidden-tab view
- **WHEN** staff export the Insights report while Jobs, Renewal & Engagement, or Risk is not the selected screen tab
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

