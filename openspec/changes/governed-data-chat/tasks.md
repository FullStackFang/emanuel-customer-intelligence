## 1. Governed snapshot builder (the guarantee, built first)

- [ ] 1.1 Write the leak test first: over representative data containing households with names, emails, addresses, identifiers, a Watch List, and at-risk households, assert a built snapshot contains none of them and no group below N=5 (test fails until the builder exists)
- [ ] 1.2 Add `chat_context.rs` with a `GovernedSnapshot` type and an allow-list of sanctioned aggregate sources (Insights trends, cohorts, financials-by-band/age, KPIs, retention, channels, schools, outcomes, dues, concentration) plus the CONTEXT.md data dictionary
- [ ] 1.3 Implement the builder: assemble only allow-listed aggregates, apply the k-anonymity floor (drop any group < 5 households), and add the "small groups may be omitted" note
- [ ] 1.4 Add unit tests for the k-anon floor (sub-floor dropped, at-floor retained) and confirm the builder references no PII-bearing source (`Hh`, `HhFy`, `AtRiskRow`, Watch List, segment members)
- [ ] 1.5 Verify: `cargo test` green, leak test passing

## 2. Chat backend abstraction and transports

- [ ] 2.1 Define a `ChatBackend` trait: `stream(snapshot, history, user_msg, on_token, cancel)`
- [ ] 2.2 Implement `CliAgentBackend` over the existing `agent.rs::run_streaming`, composing the prompt = snapshot + history + message and piping it to stdin
- [ ] 2.3 Add per-agent event parsing: extract assistant text from Claude Code `stream-json` events and from Codex `--json` events (unit-tested against captured sample lines)
- [ ] 2.4 Apply CLI lockdown per spec: Claude Code with `--strict-mcp-config --mcp-config {}`, Bash/Read/Write disallowed, no data `--add-dir`; Codex with `-s read-only` in a fresh empty temp cwd; assert the store path and key are never in argv/env/cwd
- [ ] 2.5 Implement `OllamaBackend`: keyless streaming client to local `POST /api/chat`, honoring the configured localhost base URL
- [ ] 2.6 Implement multi-turn per backend (Ollama replays history; Claude Code `--resume <session>`; Codex session/replay) and store the backend session id with the conversation
- [ ] 2.7 Unit tests for prompt composition and lockdown argument building

## 3. Conversation persistence

- [ ] 3.1 Add `_chat_conversations` and `_chat_messages` tables to the store `SCHEMA` (idempotent `CREATE TABLE IF NOT EXISTS`)
- [ ] 3.2 Add store methods: create/list/rename/delete conversation, append/list messages, clear-all-chat
- [ ] 3.3 Confirm `purge_mirror` does not touch chat tables; add a test that a mirror purge preserves conversations
- [ ] 3.4 Test round-trip persistence and delete; verify chat survives an Insights rebuild
- [ ] 3.5 Verify: `cargo test` green

## 4. Tauri commands and streaming events

- [ ] 4.1 Add commands: `chat_send` (builds snapshot, runs selected backend, emits `chat:token`/`chat:done`/`chat:error`), `chat_cancel`, and conversation CRUD + clear-history
- [ ] 4.2 Add a `chat_backend_status` command reporting availability (CLI present/logged-in via `agent.rs::detect`; Ollama reachable)
- [ ] 4.3 Register the new run in the existing `AgentRegistry` so `chat_cancel` aborts and terminates the subprocess
- [ ] 4.4 Register all new commands in `lib.rs` `invoke_handler`
- [ ] 4.5 Test cancel terminates an in-progress run and returns to idle

## 5. Frontend chat overlay

- [ ] 5.1 Add typed `api.ts` bindings for the new commands and event payloads
- [ ] 5.2 Build `ChatWidget`: bottom-right FAB toggling a ~380×560 panel, using existing design-system components (`Card`, `IconButton`, `Textarea`, `Menu`, `Select`)
- [ ] 5.3 Panel header: backend selector (Ollama/Claude/ChatGPT) with unavailable-state display; conversation menu (new/rename/delete) and saved-conversation drawer
- [ ] 5.4 Message list with incremental streaming render subscribed to `chat:token`/`chat:done`/`chat:error`; input with send and cancel
- [ ] 5.5 Mount `ChatWidget` in `App.tsx` outside the page router so it is available on every page
- [ ] 5.6 Add a "clear chat history" action wired to the store command
- [ ] 5.7 Frontend tests for open/close, backend selection, streaming render, and cancel

## 6. Verification

- [ ] 6.1 Run `npm run verify` (typecheck + vitest + `cargo test`) — all green, leak test included
- [ ] 6.2 Manual real-app check per each backend available: ask "who is our most profitable cohort?", confirm a grounded answer streams, and confirm no household name/email/address appears
- [ ] 6.3 Confirm switching backends does not change exposure (same snapshot), and that an unavailable backend shows the unavailable state without sending anything
- [ ] 6.4 Confirm conversations persist across restart and survive a mirror purge
