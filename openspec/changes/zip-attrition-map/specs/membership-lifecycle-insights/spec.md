## ADDED Requirements

### Requirement: Exit-outcome chart category fidelity
The system SHALL render every primary Exit Outcome category returned by the lifecycle analytical dataset as its own resignation-chart series and SHALL use the canonical primary Exit Outcome order when those outcomes are present. It SHALL NOT silently fold a returned primary Exit Outcome into an `Other` series because a frontend fixed list is stale.

#### Scenario: Primary Exit Outcomes replace raw reason groups
- **WHEN** lifecycle analytics returns `Addressable Churn`, `Conversion Loss`, `Structural Exit`, and `Administrative or Unknown Exit` for fiscal-year exits
- **THEN** the resignation chart displays those outcomes as distinct series rather than displaying all exits as `Other`

#### Scenario: Future category is returned
- **WHEN** lifecycle analytics returns a valid category outside the canonical primary Exit Outcome order
- **THEN** the chart renders that category as a distinct deterministic series and the aggregate table shows its exact label
