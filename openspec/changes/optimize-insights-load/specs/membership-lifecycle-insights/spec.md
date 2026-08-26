## ADDED Requirements

### Requirement: Insights views render independently of risk analysis

The system SHALL make the membership lifecycle views available without waiting for the validated risk analysis to complete, and SHALL NOT let risk computation block or delay the display of the lifecycle views.

#### Scenario: Lifecycle views appear before risk is ready

- **WHEN** staff open Insights and the risk analysis has not yet completed
- **THEN** the membership, Entry Job, renewal, engagement, and exit views display, and the risk view shows its own in-progress state until it completes

#### Scenario: Risk failure does not blank the lifecycle views

- **WHEN** the risk analysis fails or produces no validated ranking
- **THEN** the membership lifecycle views remain fully displayed and only the risk view reports its unavailability

#### Scenario: Reading lifecycle views does not wait on risk computation

- **WHEN** the lifecycle views for the current fiscal year are read
- **THEN** their result does not require the risk analysis to run first and is not serialized behind an in-progress risk computation
