## ADDED Requirements

### Requirement: Governed snapshot is the sole model input

The chat SHALL send to any model backend only a de-identified aggregate snapshot
assembled by a single Rust builder. No other data derived from the encrypted mirror
SHALL be placed in any prompt, on any transport, for any backend.

#### Scenario: Only the snapshot reaches the backend
- **WHEN** a chat turn is dispatched to any backend
- **THEN** the bytes sent to that backend contain the governed snapshot, the conversation history, and the user's message, and no other mirror-derived content

#### Scenario: No raw source is read outside the builder
- **WHEN** a chat turn is prepared
- **THEN** the prompt content is produced exclusively by the governed snapshot builder, and no Tauri command that returns Membership Household rows is invoked to build it

### Requirement: PII-bearing sources are excluded by construction

The snapshot builder SHALL read only from an explicit allow-list of aggregate sources.
It SHALL NOT reference any source that carries Membership Household identity, including
household rows, per-household financial rows, at-risk rows, the Watch List, or Segment
member lists. Names, email addresses, postal addresses, and household identifiers SHALL
never appear in a snapshot.

#### Scenario: Identifying fields are absent from a snapshot
- **WHEN** a snapshot is built over data that includes households with names, emails, addresses, and identifiers
- **THEN** the snapshot contains none of those names, emails, addresses, or household identifiers

#### Scenario: Watch List and at-risk households never enter a snapshot
- **WHEN** a Watch List and at-risk households exist in the store
- **THEN** no Watch List entry and no at-risk Membership Household appears in the snapshot

### Requirement: k-anonymity floor

The snapshot SHALL omit any aggregate group that represents fewer than five (5)
Membership Households. The snapshot SHALL indicate that small groups may be omitted so
the model does not treat published totals as exhaustive.

#### Scenario: Sub-floor group is dropped
- **WHEN** an aggregate group represents fewer than five Membership Households
- **THEN** that group is absent from the snapshot

#### Scenario: At-floor group is retained
- **WHEN** an aggregate group represents five or more Membership Households
- **THEN** that group may appear in the snapshot

### Requirement: Automated leak test proves the guarantee

An automated test SHALL build a snapshot over representative data containing PII and
assert the snapshot carries no email pattern, no household-identifier value, no personal
name, and no group below the k-anonymity floor. This test SHALL run in the standard test
suite and SHALL fail the build when a snapshot leaks identifying data.

#### Scenario: Leak test fails on an introduced leak
- **WHEN** the builder is changed so a snapshot includes an identifying field
- **THEN** the leak test fails

#### Scenario: Leak test passes on a clean snapshot
- **WHEN** the builder produces a snapshot from the sanctioned aggregate sources only
- **THEN** the leak test passes

### Requirement: CLI agent lockdown

A CLI-agent backend (Claude Code, Codex) SHALL be spawned with no route to the encrypted
mirror other than the snapshot on standard input. It SHALL run with file-editing and
shell tools disabled, without any external agent configuration, and without a working
directory that contains the store or the repository. The store file and its encryption
key SHALL never be passed to a subprocess.

#### Scenario: Claude Code is locked down
- **WHEN** the Claude Code backend is launched
- **THEN** it runs with a strict empty agent configuration, with Bash/Read/Write disallowed, and with no added data directory

#### Scenario: Codex is locked down
- **WHEN** the Codex backend is launched
- **THEN** it runs read-only in a fresh empty working directory that contains neither the store nor the repository

#### Scenario: The encrypted store is never handed to a subprocess
- **WHEN** any CLI-agent backend is launched
- **THEN** neither the store file path used for data nor its encryption key is present in the subprocess arguments, environment, or working directory

### Requirement: Uniform governance across backends

The same governed snapshot SHALL be the input for every backend, including the local
Ollama backend. Selecting a different backend SHALL NOT change what data is exposed.

#### Scenario: Switching backend does not change exposure
- **WHEN** the same question is asked with Ollama, then with Claude, then with ChatGPT
- **THEN** each backend receives the same governed snapshot and none receives raw Membership Household data
