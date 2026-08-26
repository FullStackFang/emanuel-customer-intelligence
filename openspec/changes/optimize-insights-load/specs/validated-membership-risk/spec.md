## ADDED Requirements

### Requirement: Reused validated risk result

The system SHALL reuse a previously computed validated risk result across reads while the analytical dataset is unchanged, and SHALL recompute it when the analytical dataset is rebuilt. Reuse SHALL NOT alter the model, its validation gates, or any score, name, or evidence it would otherwise produce.

#### Scenario: Second read after a build reuses the result

- **WHEN** the risk analysis has been computed for the current analytical dataset and Insights is opened again with no intervening rebuild
- **THEN** the same validation results, current household scores, and named Watch List are returned without retraining the model

#### Scenario: Rebuild invalidates the reused result

- **WHEN** the analytical dataset is rebuilt after a source sync
- **THEN** the next risk read recomputes the model against the rebuilt dataset rather than returning the pre-rebuild result

#### Scenario: Unreadable or absent reuse falls back to computation

- **WHEN** no reusable result exists for the current dataset, or a stored result cannot be read back
- **THEN** the system computes the risk analysis from the analytical dataset and produces the same result it would have without any reuse

#### Scenario: Reuse preserves the privacy and audit boundary

- **WHEN** a reused result includes named Watch List households
- **THEN** those names remain stored only inside the encrypted local database and named Watch List access remains audited exactly as an uncached load
