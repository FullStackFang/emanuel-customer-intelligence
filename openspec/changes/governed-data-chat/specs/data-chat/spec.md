## ADDED Requirements

### Requirement: Floating chat launcher

The app SHALL present a floating chat launcher available on every page. Toggling the
launcher SHALL open and close a chat panel without navigating away from the current page.

#### Scenario: Launcher opens the panel from any page
- **WHEN** a user is on any page and activates the chat launcher
- **THEN** the chat panel opens over the current page and the page does not change

#### Scenario: Launcher closes the panel
- **WHEN** the chat panel is open and the user toggles the launcher
- **THEN** the panel closes and the current page remains unchanged

### Requirement: Keyless backend selection

The chat SHALL offer three model backends — Ollama (local), Claude (Claude Code CLI), and
ChatGPT (Codex CLI) — and SHALL NOT require or use any API key for any of them. The user
SHALL be able to select which backend answers.

#### Scenario: Three backends are selectable
- **WHEN** the user opens the backend selector
- **THEN** Ollama, Claude, and ChatGPT are offered

#### Scenario: No API key is requested
- **WHEN** the user selects any backend and sends a message
- **THEN** the chat proceeds using the local server or the CLI's own login, and never prompts for or uses an API key

#### Scenario: Unavailable backend is reported
- **WHEN** a selected backend's CLI is missing or not logged in, or the local Ollama server is unreachable
- **THEN** the panel shows that the backend is unavailable and no snapshot is assembled or sent

### Requirement: Ask questions about the membership data

The chat SHALL answer natural-language questions about the membership data using only the
governed snapshot as grounding, using canonical terms from CONTEXT.md.

#### Scenario: Cohort question is answered from aggregates
- **WHEN** the user asks which cohort is most profitable
- **THEN** the assistant answers from the governed aggregate snapshot without disclosing any individual Membership Household

### Requirement: Streaming replies with cancel

Assistant replies SHALL stream incrementally as they are produced. The user SHALL be able
to cancel an in-progress reply.

#### Scenario: Reply streams token by token
- **WHEN** the assistant is generating a reply
- **THEN** partial content appears in the panel as it is produced

#### Scenario: User cancels a reply
- **WHEN** a reply is in progress and the user cancels it
- **THEN** generation stops, the backend process is terminated, and the panel returns to an idle input state

### Requirement: Multi-turn conversations

The chat SHALL maintain conversational context across turns within a conversation, so
follow-up questions are answered in light of earlier turns.

#### Scenario: Follow-up uses prior context
- **WHEN** the user asks a follow-up that depends on the previous question and answer
- **THEN** the assistant's reply reflects the earlier turns in the same conversation

### Requirement: Saved conversations

Conversations SHALL be persisted in the encrypted store. The user SHALL be able to list,
open, rename, and delete conversations, and to start a new conversation. A conversation
SHALL record which backend produced it.

#### Scenario: Conversation persists across restart
- **WHEN** the user holds a conversation, closes the app, and reopens it
- **THEN** the conversation is listed and can be reopened with its messages intact

#### Scenario: Delete removes a conversation
- **WHEN** the user deletes a conversation
- **THEN** it no longer appears in the list and its messages are removed from the store

#### Scenario: Clearing chat history leaves mirror data untouched
- **WHEN** the user clears chat history
- **THEN** all conversations and messages are removed and the synced mirror data and Insights are unaffected

### Requirement: Chat persistence survives data operations

Chat history SHALL be independent of the synced mirror. Purging mirror data SHALL NOT
delete conversations, and rebuilding Insights aggregates SHALL NOT invalidate them.

#### Scenario: Mirror purge preserves chat history
- **WHEN** the user purges the synced mirror data
- **THEN** existing conversations and messages remain available
