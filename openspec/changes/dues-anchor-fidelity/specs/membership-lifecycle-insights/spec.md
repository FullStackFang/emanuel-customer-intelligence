## MODIFIED Requirements

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
