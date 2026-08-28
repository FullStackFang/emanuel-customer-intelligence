## ADDED Requirements

### Requirement: Each audited action carries a correlation id

An audited command SHALL generate a correlation identifier at the start of the action
and SHALL include that identifier in the audit row it writes. The identifier SHALL be
unique per action invocation.

#### Scenario: Audit row carries a correlation id
- **WHEN** an audited command runs
- **THEN** the audit row it writes includes a correlation identifier

#### Scenario: Distinct invocations get distinct ids
- **WHEN** the same command is invoked twice
- **THEN** the two audit rows carry different correlation identifiers

### Requirement: Log events for an action share its correlation id

The log events emitted while handling an audited action SHALL carry the same correlation
identifier that appears on that action's audit row, so a log line can be tied to its
audit row.

#### Scenario: A log event and its audit row can be matched
- **WHEN** an audited action emits log events and writes an audit row
- **THEN** the correlation identifier on those log events matches the one recorded on the audit row

### Requirement: Correlation adds no schema change or PII

Threading the correlation identifier SHALL reuse the existing audit detail and log
fields and SHALL NOT require a new database column. The correlation identifier SHALL be
an opaque value that is not derived from and does not contain Membership Household
identifying data.

#### Scenario: No new audit column is introduced
- **WHEN** correlation identifiers are recorded
- **THEN** they are carried within the existing audit detail without adding a database column

#### Scenario: Correlation id reveals no identity
- **WHEN** a correlation identifier is generated
- **THEN** it contains no household name, email, address, or household identifier
