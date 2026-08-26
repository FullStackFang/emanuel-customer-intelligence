## ADDED Requirements

### Requirement: Addressable Churn prediction target
The system SHALL train and evaluate household risk using households active at the end of fiscal year N and Addressable Churn during fiscal year N+1 as the target.

#### Scenario: Household has a Structural Exit
- **WHEN** a household active in fiscal year N has a Structural Exit in N+1
- **THEN** that outcome is excluded from model training and evaluation rather than labeled as Addressable Churn

#### Scenario: Household has a Conversion Loss
- **WHEN** an introductory-tier household has a Conversion Loss in N+1
- **THEN** the outcome is analyzed separately and excluded from the Addressable Churn target

### Requirement: Cutoff-safe model features
The system SHALL use only feature values demonstrably available by the end of fiscal year N and SHALL reject post-cutoff or future information.

#### Scenario: Final balance lacks historical lineage
- **WHEN** a historical billing balance cannot be proven to represent its value by the fiscal-year cutoff
- **THEN** that balance is omitted from model features even if it remains available for descriptive analysis

#### Scenario: Future event enters a feature row
- **WHEN** a resignation, payment, enrollment, or committee event occurs after the feature cutoff
- **THEN** the feature-lineage validation rejects its inclusion

### Requirement: Rolling historical validation
The system SHALL evaluate regularized logistic regression with rolling fiscal-year backtests and SHALL display every eligible test year, including failed years.

#### Scenario: Validation history is insufficient
- **WHEN** fewer than three completed test years are available
- **THEN** no current household scores or named Watch List are produced

#### Scenario: Test-year sample is insufficient
- **WHEN** a test year has fewer than 200 eligible households or fewer than 20 Addressable Churn exits
- **THEN** the model fails the validation gate and the insufficient sample is reported

### Requirement: Predictive quality gates
The system SHALL produce current household scores only when aggregate ROC-AUC is at least 0.65, top-decile lift is at least 2.0 times baseline, and Brier score is better than a constant baseline-rate prediction.

#### Scenario: Discrimination passes but calibration fails
- **WHEN** ROC-AUC and lift pass but Brier score does not beat the baseline predictor
- **THEN** model validation fails and household scores remain hidden

#### Scenario: Every quality gate passes
- **WHEN** the model passes sample, coverage, ROC-AUC, lift, and Brier score gates
- **THEN** it may rank current households using the validated feature set

### Requirement: Optional-source coverage validation
The system SHALL require each optional model feature family to have at least 70 percent coverage in training and scoring data with no more than a 15 percentage-point coverage shift.

#### Scenario: Billing coverage shifts materially
- **WHEN** billing feature coverage differs by more than 15 percentage points between training and scoring data
- **THEN** billing features are removed and the complete rolling backtest reruns

#### Scenario: Reduced feature set fails
- **WHEN** the model without an ineligible optional feature family fails any quality gate
- **THEN** no household scores or named Watch List are produced

### Requirement: Evidence-gated Watch List
The system SHALL include a current Membership Household on the named Watch List only when it is in a passing model's top risk decile and has at least two independent classes of current or recent Risk Evidence.

#### Scenario: Religious School ended ten years ago
- **WHEN** a current household's only notable condition is that Religious School ended ten years earlier
- **THEN** the household is not included on the named Watch List

#### Scenario: One recent evidence class
- **WHEN** a top-decile household has multiple fields derived from one recent event but no second independent evidence class
- **THEN** the fields count as one class and the household is not included on the named Watch List

#### Scenario: Two independent recent evidence classes
- **WHEN** a top-decile current household has at least two independent current or recent evidence classes
- **THEN** the household may appear on the named Watch List with those classes explained

### Requirement: Risk ranking suppression
The system SHALL show historical factor aggregates and validation results when available but SHALL NOT substitute fixed rules or unvalidated scores when predictive validation fails.

#### Scenario: Model fails validation
- **WHEN** any required validation gate fails
- **THEN** Risk displays the failure and aggregate evidence with an explicit "No validated household ranking" state

### Requirement: Explainable review queue
The system SHALL present the Watch List as a staff review queue and SHALL provide the observed evidence, comparison baseline, model period, and confidence for each listed household.

#### Scenario: Staff opens a listed household row
- **WHEN** staff review a Watch List entry
- **THEN** the UI explains why it qualified without claiming the household will resign or that any feature is causal

### Requirement: Named risk privacy and audit
The system SHALL load named risk results only on explicit request, audit named access and export, and exclude household names from aggregate reports and PDFs.

#### Scenario: Risk tab loads
- **WHEN** staff open the Risk tab without requesting household names
- **THEN** only aggregate factors and validation results are returned

#### Scenario: Staff requests household names
- **WHEN** staff explicitly request the named Watch List
- **THEN** the system returns eligible rows and records an audit event containing the result count but no risk-feature values
