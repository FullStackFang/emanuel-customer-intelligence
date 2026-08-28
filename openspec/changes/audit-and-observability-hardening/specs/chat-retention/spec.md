## ADDED Requirements

### Requirement: Stored chat transcripts expire by age

The application SHALL apply an age-based retention policy to stored chat messages: a
message older than a configured maximum age SHALL be pruned from storage. The maximum
age SHALL have a documented default expressed as a named constant.

#### Scenario: A message past the maximum age is pruned
- **WHEN** retention runs and a stored chat message is older than the maximum age
- **THEN** that message is deleted from storage

#### Scenario: A message within the maximum age is retained
- **WHEN** retention runs and a stored chat message is within the maximum age
- **THEN** that message remains in storage

### Requirement: Retention is applied without a background scheduler

Retention SHALL be applied opportunistically on the existing chat entry points — at
minimum when a conversation is opened and when a chat turn begins — rather than by a
background timer. Pruning SHALL be best-effort and SHALL NOT block or fail the user's
chat action if it cannot run.

#### Scenario: Opening chat prunes expired history
- **WHEN** a user opens the chat and expired messages exist
- **THEN** the expired messages are pruned as part of that interaction

#### Scenario: A pruning failure does not break chat
- **WHEN** retention cannot complete for any reason during a chat action
- **THEN** the user's chat action still proceeds and the failure does not surface as a chat error

### Requirement: Conversations left empty by pruning are removed

A conversation SHALL be removed when retention deletes its last remaining message, so
no empty conversation shells accumulate.

#### Scenario: Emptied conversation is removed
- **WHEN** retention prunes the final message of a conversation
- **THEN** that conversation no longer appears in the conversation list

#### Scenario: Partially pruned conversation is kept
- **WHEN** retention prunes some but not all messages of a conversation
- **THEN** that conversation remains with its surviving messages

### Requirement: Retention is independent of the data-mirror purge

Age-based chat retention SHALL be a policy distinct from the data-mirror purge. Purging
the data mirror SHALL continue to leave chat history untouched, and chat retention SHALL
NOT delete any mirrored source data.

#### Scenario: Mirror purge still preserves chat history
- **WHEN** the data mirror is purged
- **THEN** chat conversations and their unexpired messages remain intact
