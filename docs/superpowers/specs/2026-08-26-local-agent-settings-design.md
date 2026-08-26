# Local Agent Settings Foundation Design (Claude Code + Codex)

Date: 2026-08-26

Status: approved in conversation (direction confirmed: Claude Code + Codex, repurpose scaffold); pending written review

**Supersedes:** `2026-08-26-llm-provider-settings-design.md`. That design drove HTTP LLM
provider APIs with API keys. The intent was always to drive the user's **locally-installed CLI
agents** (Claude Code, Codex) using their own login/subscription — no API key. This design replaces
the mechanism and keeps the reusable scaffold (Settings page + nav, encrypted `_meta` persistence,
audit hooks, `api.ts` binding style).

## 1. Purpose

Add a Settings surface that detects the locally-installed `claude` (Claude Code) and `codex` CLIs,
lets staff configure how each is invoked, and drives them **headless** — piping a prompt in and
streaming their output back into the app live. Auth is the CLI's own login (verified: Claude Code
reads OAuth/keychain in normal mode; only `--bare` forces `ANTHROPIC_API_KEY`). **No API key is ever
stored or sent by this app.**

This is the foundation. Nothing yet wires the app's own data/tools into the agent; that is a later
sub-project. This delivers: detection, per-agent configuration, and a spawn/stream/kill runtime with
a minimal "test run" in Settings.

## 2. Scope

**In scope**
- Two agents: `claude-code`, `codex`, each detected via `<bin> --version` (short timeout).
- Non-secret per-agent config persisted in the encrypted `_meta` store.
- A spawn/stream/kill runtime: run the CLI headless, stream stdout lines to the webview via Tauri
  events, enforce a timeout, and allow cancel.
- A Settings page: detection status per agent, per-agent config, and a "Test run" that streams a
  short response.
- Remove the superseded HTTP-provider mechanism (provider enum, API-key keychain entries, the HTTP
  `build_test_request`/`run_test`, cloud-egress opt-in) — repurposing the scaffold around it.

**Out of scope** (later sub-projects)
- Feeding the app's membership data/tools to the agent (via MCP or otherwise).
- A full chat UI / conversation history.
- Multi-turn session resumption.

## 3. Success Criteria

- Settings shows, per agent, whether it is installed and its version (or a clear "not found").
- A user can set each agent's model, working directory, timeout, isolation toggle, and agent-specific
  option (Claude: permission mode; Codex: sandbox mode), save, and reload with settings intact.
- A "Test run" spawns the selected agent headless, streams its output into the panel line by line,
  and can be cancelled; a timeout kills the process.
- No API key is stored anywhere; nothing in this feature reads or writes a keychain key.
- Every settings mutation and every spawn is audited (action + agent name; never prompt contents in
  the audit).
- New Rust logic that can be tested without spawning a process is unit-tested (arg building,
  settings serde, Windows binary resolution); spawn/stream is verified by an integration test that
  runs a trivial cross-platform command, plus manual runs against the real CLIs.

## 4. Data Model

### 4.1 Config — `_meta['agent_settings']` (JSON)

```jsonc
{
  "active_agent": "claude-code",            // "claude-code" | "codex" | null
  "agents": {
    "claude-code": {
      "model": null,                        // Option<String>; null => the CLI's own default
      "working_dir": null,                  // Option<String>; null => the app data dir
      "permission_mode": "plan",            // Claude-only: plan | default | acceptEdits | bypassPermissions
      "timeout_minutes": 10,
      "isolate": false,                     // true => --strict-mcp-config --mcp-config {} (ignore ~/.claude MCP)
      "extra_args": []                      // escape hatch, appended verbatim
    },
    "codex": {
      "model": null,
      "working_dir": null,
      "sandbox_mode": "read-only",          // Codex-only: read-only | workspace-write | danger-full-access
      "timeout_minutes": 10,
      "isolate": false,                     // true => --skip-git-repo-check already on; reserved for config isolation
      "extra_args": []
    }
  }
}
```

No secrets are stored — the config carries only invocation options. Defaults are seeded per agent.

### 4.2 No keychain

This feature stores nothing in Windows Credential Manager. The superseded design's `llm_key_*`
entries are removed (a one-time cleanup deletes any that were written during the API build).

### 4.3 Detection result — `AgentStatus`

`{ agent, installed: bool, version: Option<String>, path: Option<String>, error: Option<String> }`,
returned by detection; not persisted.

## 5. Backend

Repurpose `src-tauri/src/llm.rs` → **`src-tauri/src/agent.rs`** (rename; update `lib.rs`). The module
owns the agent enum, config, persistence, arg building, detection, and the spawn/stream/kill runtime.

### 5.1 Types

- `Agent { ClaudeCode, Codex }` — `#[serde(rename_all = "kebab-case")]` ⇒ `"claude-code"`, `"codex"`;
  `all()`, `as_str()`, `bin()` (`"claude"` / `"codex"`).
- `AgentConfig { model: Option<String>, working_dir: Option<String>, permission_mode: Option<String>,
  sandbox_mode: Option<String>, timeout_minutes: u64, isolate: bool, extra_args: Vec<String> }`
  with `default_for(agent)`.
- `AgentSettings { active_agent: Option<Agent>, agents: { claude_code, codex } }` with
  `load(&Store)`/`save(&Store)` over `_meta['agent_settings']`, per-field serde defaults, `config(agent)`.
- `AgentSettingsView` — same config plus each agent's live `AgentStatus` (so the UI shows install
  state without a second call). Detection runs when the view is built.

### 5.2 Binary resolution (Windows-aware)

`resolve_bin(agent) -> Option<PathBuf>`: on Windows an npm-installed CLI is a `claude.cmd` shim, and
Rust's `Command` cannot execute `.cmd` directly — it must go through `cmd.exe`. Strategy:
- Look up the base name on `PATH` trying, in order, `claude.cmd`, `claude.exe`, `claude` (and the
  `codex` equivalents). Return the first that exists.
- Spawning always goes through `cmd /C <resolved> <args…>` on Windows; direct `Command` on Unix.

Pure and unit-testable given a fake PATH.

### 5.3 Argument building (pure, unit-tested)

`build_argv(agent, &AgentConfig) -> Vec<String>` (prompt is NOT an arg — it is piped via stdin to
avoid quoting issues):

- **claude-code:** `-p --output-format stream-json --verbose`
  + `--model <model>` if set
  + `--permission-mode <permission_mode>` if set
  + `--add-dir <working_dir>` if set
  + `--strict-mcp-config --mcp-config {"mcpServers":{}}` if `isolate`
  + `extra_args…`
- **codex:** `exec --json --skip-git-repo-check`
  + `-m <model>` if set
  + `-s <sandbox_mode>` if set
  + `-C <working_dir>` if set
  + `extra_args…`

### 5.4 Spawn / stream / kill runtime

Uses `tokio::process` (enable the `process` and `io-util` features on the existing `tokio`
dependency — a feature add, not a new crate).

- `AppState` gains `agents: Mutex<HashMap<String, tokio::process::Child>>` (or an abort handle) keyed
  by a generated `stream_id`.
- `spawn(app, agent, config, prompt) -> stream_id`:
  1. Resolve the binary; error clearly if not installed.
  2. Build the command (`cmd /C` on Windows), set `cwd = working_dir` (or app data dir),
     `stdin/stdout/stderr = piped`.
  3. Write `prompt` to stdin, then close stdin.
  4. Spawn a task that reads stdout line by line and emits a Tauri event
     `agent:output` `{ stream_id, line }`; on stderr, emit `agent:error` `{ stream_id, line }`;
     on exit emit `agent:exit` `{ stream_id, code }`.
  5. A timeout (`timeout_minutes`) kills the child and emits `agent:exit` with a timeout marker.
- `kill(stream_id)`: look up the child and kill it.

Prompts are never logged; audit records the spawn as `agent.run` with the agent name only.

### 5.5 Commands (`commands.rs`), registered in `lib.rs`

Remove the 5 superseded `*_llm_*` commands. Add:

| Command | Behaviour |
|---|---|
| `get_agent_settings() -> AgentSettingsView` | Load config, run detection for both agents, return config + status. |
| `set_agent_settings(settings)` | Persist to `_meta`; audit `settings.agent.update`. |
| `detect_agents() -> [AgentStatus; 2]` | Detection only (for a manual "re-check"). |
| `agent_run(agent, prompt) -> String` | Spawn headless; returns the `stream_id`; audit `agent.run` (agent only). Output arrives via events. |
| `agent_cancel(stream_id)` | Kill the running process. |

`agent_run` reads config + resolves the binary without holding the store lock across the spawn await.

## 6. Frontend

### 6.1 `api.ts`

Replace the llm bindings with: types `Agent` (`"claude-code" | "codex"`), `AgentConfig`,
`AgentStatus`, `AgentSettingsView`, `AgentSettings`; const `AGENTS`; functions `getAgentSettings`,
`setAgentSettings`, `detectAgents`, `agentRun`, `agentCancel`; and event listeners
`onAgentOutput`, `onAgentError`, `onAgentExit` (following the existing `onScanProgress` pattern).

### 6.2 `SettingsPage.tsx`

Rewrite the AI Agent section:
- Per agent, a detection card: installed ✓ + version, or "not found" with the resolved lookup, and a
  **Re-check** button (`detectAgents`).
- Agent selector (`active_agent`) and, for the selected agent, its config fields (model, working dir,
  timeout, isolate; Claude: permission mode; Codex: sandbox mode) + **Save**.
- A **Test run**: a prompt box + Run button that calls `agentRun`, streams `agent:output` lines into a
  scrolling panel, shows exit status, and offers **Cancel** (`agentCancel`). No API-key field, no
  egress warning — neither applies.

### 6.3 App shell

`App.tsx` keeps the existing `settings` nav entry and render (unchanged from the scaffold).

## 7. Error Handling

- Missing binary ⇒ `AgentStatus.installed=false` with the attempted lookup; `agent_run` returns a
  clear "‹agent› is not installed" error.
- Spawn/IO failures never panic; they surface as an `agent:error`/`agent:exit` event or a command
  `Err(String)`.
- Timeout kills the child and reports it as a distinct exit reason.

## 8. Testing

**Rust**
- `build_argv` per agent for representative configs (flags present/absent, isolate on/off, extra_args).
- `AgentSettings` serde round-trip through `_meta`, default seeding for absent agents.
- `resolve_bin` PATH resolution given a fake directory containing `claude.cmd` (Windows) / `claude`
  (Unix).
- One integration test of the spawn/stream runtime using a trivial portable command (e.g. echo) via
  a test seam so it does not depend on `claude`/`codex` being installed in CI, asserting an
  `agent:output` line and an `agent:exit` are produced.

**Frontend (Vitest)**
- api bindings map to the exact command names/args; event listeners subscribe to the right channels.

**Manual**
- Real `claude -p` / `codex exec` runs from the Settings "Test run", streaming and cancel, on Windows.

## 9. Migration / cleanup from the superseded build

- Rename `llm.rs` → `agent.rs`; delete the HTTP/provider/key code.
- Remove the 5 `*_llm_*` commands and their registrations; delete the llm `api.ts` bindings and their
  test.
- Delete any `llm_key_*` keychain entries written during the API build (best-effort, ignored if
  absent).
- Keep: the `settings` nav entry, `_meta` persistence pattern, audit calls, and the Settings page
  shell (rewritten body).
