## ADDED Requirements

### Requirement: Record-level auditing of Membership Household disclosure

Any command that returns data identifying specific Membership Households SHALL write an
audit row recording the set of Membership Household identifiers disclosed to the viewer,
in addition to the existing result count. This applies to the at-risk read, the Watch
List read, and the Watch List CSV export. The identifier recorded SHALL be the household
account identifier; the household **name** SHALL NOT be written to the audit.

#### Scenario: At-risk read records the disclosed households
- **WHEN** a staff user loads the at-risk read and the result contains several Membership Households
- **THEN** an audit row is written whose detail includes the account identifier of every disclosed household and the total count

#### Scenario: Watch List read records the disclosed households
- **WHEN** a staff user loads the Watch List and it contains Membership Households
- **THEN** an audit row is written whose detail includes the account identifier of every listed household

#### Scenario: Household name is never stored in the audit
- **WHEN** any record-level access audit row is written
- **THEN** the audit detail contains household account identifiers but no household name

#### Scenario: Empty result still records an accountable read
- **WHEN** a staff user loads the at-risk read or Watch List and the result is empty
- **THEN** an audit row is written recording a zero count and an empty identifier set

### Requirement: The disclosed-household record is complete

The recorded set of household identifiers SHALL contain every Membership Household
returned to the viewer for that read, not a sample or a truncated subset, so the audit
is an authoritative record of what was disclosed.

#### Scenario: All returned households appear in the audit
- **WHEN** a read returns N Membership Households
- **THEN** the audit detail lists exactly those N household identifiers

### Requirement: Reads of governance-sensitive stores are audited

Reading the audit log, listing chat conversations, and opening a chat transcript SHALL
each write an audit row. These rows SHALL record only low-cardinality metadata — such
as counts, offsets, or a conversation identifier — and SHALL NOT record chat message
content or audit-row content.

#### Scenario: Reading the audit log is itself audited
- **WHEN** a staff user opens the audit view
- **THEN** an audit row is written recording that the audit log was read, with its paging parameters and no audit-row content

#### Scenario: Opening a chat transcript is audited
- **WHEN** a staff user opens a saved conversation and its messages are loaded
- **THEN** an audit row is written recording the conversation identifier and the message count, and no message content

#### Scenario: Reading the audit log does not recurse
- **WHEN** the audit log is read
- **THEN** exactly one audit row is written for that read and the write does not trigger further audit rows

### Requirement: The audit remains append-only

Adding record-level and read auditing SHALL NOT introduce any code path that updates or
deletes rows in the audit log. Audit rows SHALL continue to be insert-only.

#### Scenario: No update or delete path is added
- **WHEN** the audit capability is exercised through any command
- **THEN** audit rows are only ever inserted, never modified or removed
