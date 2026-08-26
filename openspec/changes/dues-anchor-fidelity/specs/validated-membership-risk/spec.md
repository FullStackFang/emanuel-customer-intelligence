## MODIFIED Requirements

### Requirement: Optional-source coverage validation
The system SHALL require each optional model feature family to have at least 70 percent coverage in training and scoring data with no more than a 15 percentage-point coverage shift. Coverage SHALL be measured on whether each household-year actually carries source data for the family in that fiscal year, not on whether the family's source is merely available, so a fiscal year with no source data for a family counts as uncovered for that family.

#### Scenario: Billing coverage shifts materially
- **WHEN** billing feature coverage differs by more than 15 percentage points between training and scoring data
- **THEN** billing features are removed and the complete rolling backtest reruns

#### Scenario: Reduced feature set fails
- **WHEN** the model without an ineligible optional feature family fails any quality gate
- **THEN** no household scores or named Watch List are produced

#### Scenario: Family source has no data in early training years
- **WHEN** an optional family's source data begins only in later fiscal years, so its coverage across the training window falls below 70 percent
- **THEN** that family is treated as uncovered for the years without data and is removed before the rolling backtest, rather than counted as fully covered because the source is available
