# Local Agent Settings (Claude Code + Codex) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the HTTP-API-key provider mechanism with a runtime that drives the user's locally-installed `claude` (Claude Code) and `codex` CLIs headless — detect them, configure them, and stream their output into the app — using each CLI's own login (no API key).

**Architecture:** A new `src-tauri/src/agent.rs` owns the agent enum, per-agent config (persisted in the encrypted `_meta` store), Windows-aware binary resolution, headless argument building, and an async spawn/stream/kill runtime built on `tokio::process`. The prompt is piped via stdin; stdout/stderr lines are relayed to the webview as Tauri events. New commands replace the `*_llm_*` commands; the old `llm.rs`, its keychain use, and its `api.ts` bindings are removed. The Settings page and `_meta`/audit scaffolding are reused.

**Tech Stack:** Rust + Tauri 2, `tokio` (add `process` + `io-util` features), `serde`/`serde_json`, `rusqlite`/SQLCipher; React 19 + TypeScript, Vitest, the in-repo design system.

**Spec:** `docs/superpowers/specs/2026-08-26-local-agent-settings-design.md` (supersedes the API-key design).

## Global Constraints

- Drive the **local CLIs** only. No API key is ever stored, read, or sent by this feature. This feature touches NO keychain entry (except a best-effort one-time delete of leftover `llm_key_*` entries in Task 3).
- Agent wire strings are exactly `claude-code` and `codex`. Binaries are `claude` and `codex`.
- The prompt is passed to the CLI via **stdin**, never as a shell argument.
- On Windows, an npm CLI is a `.cmd` shim that `Command` cannot exec directly — spawn via `cmd /C`. On Unix, spawn the binary directly.
- Commands return `Result<T, String>` via the existing `err()` helper; no panic/unwrap reaches the webview; never hold the store `Mutex` across an `.await`.
- Prompt contents are never logged or audited. Audit records action + agent name only.
- The crate must compile and `cargo test` must pass at the end of every task.
- No new crates. Enabling extra features on the existing `tokio` dependency is allowed.
- Work on branch `feat/llm-provider-settings`. Commit only the files each task names (the repo has unrelated concurrent WIP — never `git add -A`).

---

### Task 1: Agent core — types, config, persistence, arg building, binary resolution (`agent.rs`)

Pure/testable logic. Added ALONGSIDE the existing `llm.rs` so the crate keeps compiling.

**Files:**
- Create: `src-tauri/src/agent.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod agent;` with the other modules)
- Test: inline `#[cfg(test)]` in `src-tauri/src/agent.rs`

**Interfaces produced:**
- `Agent { ClaudeCode, Codex }` — `#[serde(rename_all = "kebab-case")]`, `Clone,Copy,PartialEq,Eq,Debug`; `all() -> [Agent;2]`, `as_str()`, `bin() -> &'static str`.
- `AgentConfig { model: Option<String>, working_dir: Option<String>, permission_mode: Option<String>, sandbox_mode: Option<String>, timeout_minutes: u64, isolate: bool, extra_args: Vec<String> }` — `Serialize,Deserialize,Clone,Debug,PartialEq`; `default_for(Agent)`.
- `AgentSettings { active_agent: Option<Agent>, claude_code: AgentConfig, codex: AgentConfig }` — per-field serde defaults + manual `Default`; `config(Agent) -> &AgentConfig`; `META_KEY="agent_settings"`; `load(&Store)`, `save(&Store)`.
- `build_argv(Agent, &AgentConfig) -> Vec<String>`.
- `resolve_bin_in(dirs: &[PathBuf], agent: Agent) -> Option<PathBuf>` (PATH-injectable for tests) and `resolve_bin(Agent) -> Option<PathBuf>` (uses real `PATH`).

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/agent.rs` with a test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    #[test]
    fn agent_wire_strings_and_bins() {
        assert_eq!(serde_json::to_string(&Agent::ClaudeCode).unwrap(), "\"claude-code\"");
        assert_eq!(serde_json::to_string(&Agent::Codex).unwrap(), "\"codex\"");
        assert_eq!(Agent::ClaudeCode.bin(), "claude");
        assert_eq!(Agent::Codex.bin(), "codex");
    }

    #[test]
    fn defaults_are_per_agent() {
        let s = AgentSettings::default();
        assert_eq!(s.active_agent, None);
        assert_eq!(s.claude_code.permission_mode.as_deref(), Some("plan"));
        assert_eq!(s.codex.sandbox_mode.as_deref(), Some("read-only"));
        assert_eq!(s.claude_code.timeout_minutes, 10);
    }

    #[test]
    fn build_argv_claude_flags() {
        let mut c = AgentConfig::default_for(Agent::ClaudeCode);
        c.model = Some("claude-sonnet-5".into());
        c.working_dir = Some("/tmp/wd".into());
        c.isolate = true;
        let a = build_argv(Agent::ClaudeCode, &c);
        assert!(a.starts_with(&["-p".to_string(), "--output-format".into(), "stream-json".into(), "--verbose".into()]));
        assert!(a.windows(2).any(|w| w == ["--model", "claude-sonnet-5"]));
        assert!(a.windows(2).any(|w| w == ["--permission-mode", "plan"]));
        assert!(a.windows(2).any(|w| w == ["--add-dir", "/tmp/wd"]));
        assert!(a.iter().any(|x| x == "--strict-mcp-config"));
        assert!(a.iter().any(|x| x == "--mcp-config"));
    }

    #[test]
    fn build_argv_codex_flags_and_isolate_off() {
        let mut c = AgentConfig::default_for(Agent::Codex);
        c.model = Some("gpt-5".into());
        c.working_dir = Some("/tmp/wd".into());
        let a = build_argv(Agent::Codex, &c);
        assert!(a.starts_with(&["exec".to_string(), "--json".into(), "--skip-git-repo-check".into()]));
        assert!(a.windows(2).any(|w| w == ["-m", "gpt-5"]));
        assert!(a.windows(2).any(|w| w == ["-s", "read-only"]));
        assert!(a.windows(2).any(|w| w == ["-C", "/tmp/wd"]));
        // claude-only flags never appear for codex
        assert!(!a.iter().any(|x| x == "--permission-mode"));
    }

    #[test]
    fn settings_round_trip_through_meta() {
        let dir = tempfile::tempdir().unwrap();
        let s = store::open(&dir.path().join("t.db"), KEY).unwrap();
        assert_eq!(AgentSettings::load(&s).unwrap().active_agent, None);
        let mut settings = AgentSettings::default();
        settings.active_agent = Some(Agent::Codex);
        settings.codex.model = Some("gpt-5".into());
        settings.save(&s).unwrap();
        let back = AgentSettings::load(&s).unwrap();
        assert_eq!(back.active_agent, Some(Agent::Codex));
        assert_eq!(back.codex.model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn resolve_bin_finds_platform_shim() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // On Windows the shim is claude.cmd; on Unix it's an extensionless file.
        let name = if cfg!(windows) { "claude.cmd" } else { "claude" };
        std::fs::write(root.join(name), b"x").unwrap();
        let found = resolve_bin_in(&[root.to_path_buf()], Agent::ClaudeCode).unwrap();
        assert_eq!(found.file_name().unwrap().to_string_lossy(), name);
        // absent agent yields None
        assert!(resolve_bin_in(&[root.to_path_buf()], Agent::Codex).is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test agent::tests`
Expected: FAIL to compile (module/types not defined).

- [ ] **Step 3: Write the implementation**

At the top of `src-tauri/src/agent.rs`:

```rust
//! Local CLI agents (Claude Code, Codex): config, persistence, arg building,
//! binary resolution. Auth is the CLI's own login — no API key is handled here.

use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    ClaudeCode,
    Codex,
}

impl Agent {
    pub fn all() -> [Agent; 2] { [Agent::ClaudeCode, Agent::Codex] }
    pub fn as_str(&self) -> &'static str {
        match self { Agent::ClaudeCode => "claude-code", Agent::Codex => "codex" }
    }
    pub fn bin(&self) -> &'static str {
        match self { Agent::ClaudeCode => "claude", Agent::Codex => "codex" }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentConfig {
    #[serde(default)] pub model: Option<String>,
    #[serde(default)] pub working_dir: Option<String>,
    #[serde(default)] pub permission_mode: Option<String>,
    #[serde(default)] pub sandbox_mode: Option<String>,
    pub timeout_minutes: u64,
    #[serde(default)] pub isolate: bool,
    #[serde(default)] pub extra_args: Vec<String>,
}

impl AgentConfig {
    pub fn default_for(a: Agent) -> AgentConfig {
        match a {
            Agent::ClaudeCode => AgentConfig {
                model: None, working_dir: None,
                permission_mode: Some("plan".into()), sandbox_mode: None,
                timeout_minutes: 10, isolate: false, extra_args: vec![],
            },
            Agent::Codex => AgentConfig {
                model: None, working_dir: None,
                permission_mode: None, sandbox_mode: Some("read-only".into()),
                timeout_minutes: 10, isolate: false, extra_args: vec![],
            },
        }
    }
}

fn dflt_claude() -> AgentConfig { AgentConfig::default_for(Agent::ClaudeCode) }
fn dflt_codex() -> AgentConfig { AgentConfig::default_for(Agent::Codex) }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentSettings {
    #[serde(default)] pub active_agent: Option<Agent>,
    #[serde(default = "dflt_claude")] pub claude_code: AgentConfig,
    #[serde(default = "dflt_codex")] pub codex: AgentConfig,
}

impl Default for AgentSettings {
    fn default() -> Self {
        AgentSettings { active_agent: None, claude_code: dflt_claude(), codex: dflt_codex() }
    }
}

pub const META_KEY: &str = "agent_settings";

impl AgentSettings {
    pub fn config(&self, a: Agent) -> &AgentConfig {
        match a { Agent::ClaudeCode => &self.claude_code, Agent::Codex => &self.codex }
    }
    pub fn load(store: &Store) -> anyhow::Result<AgentSettings> {
        match store.get_meta(META_KEY)? {
            Some(j) => Ok(serde_json::from_str(&j)?),
            None => Ok(AgentSettings::default()),
        }
    }
    pub fn save(&self, store: &Store) -> anyhow::Result<()> {
        store.set_meta(META_KEY, &serde_json::to_string(self)?)
    }
}

/// Build the headless argv (prompt is piped via stdin, never an arg).
pub fn build_argv(agent: Agent, c: &AgentConfig) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    match agent {
        Agent::ClaudeCode => {
            v.extend(["-p", "--output-format", "stream-json", "--verbose"].map(String::from));
            if let Some(m) = &c.model { v.push("--model".into()); v.push(m.clone()); }
            if let Some(p) = &c.permission_mode { v.push("--permission-mode".into()); v.push(p.clone()); }
            if let Some(w) = &c.working_dir { v.push("--add-dir".into()); v.push(w.clone()); }
            if c.isolate {
                v.push("--strict-mcp-config".into());
                v.push("--mcp-config".into());
                v.push("{\"mcpServers\":{}}".into());
            }
        }
        Agent::Codex => {
            v.extend(["exec", "--json", "--skip-git-repo-check"].map(String::from));
            if let Some(m) = &c.model { v.push("-m".into()); v.push(m.clone()); }
            if let Some(s) = &c.sandbox_mode { v.push("-s".into()); v.push(s.clone()); }
            if let Some(w) = &c.working_dir { v.push("-C".into()); v.push(w.clone()); }
        }
    }
    v.extend(c.extra_args.iter().cloned());
    v
}

/// Candidate file names for an agent's binary, most-specific first.
fn bin_candidates(agent: Agent) -> Vec<String> {
    let base = agent.bin();
    if cfg!(windows) {
        vec![format!("{base}.cmd"), format!("{base}.exe"), base.to_string()]
    } else {
        vec![base.to_string()]
    }
}

pub fn resolve_bin_in(dirs: &[PathBuf], agent: Agent) -> Option<PathBuf> {
    for dir in dirs {
        for name in bin_candidates(agent) {
            let p = dir.join(&name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

pub fn resolve_bin(agent: Agent) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    resolve_bin_in(&dirs, agent)
}
```

Add to `src-tauri/src/lib.rs` with the other module declarations:

```rust
pub mod agent;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test agent::tests`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent.rs src-tauri/src/lib.rs
git commit -m "feat(agent): core types, config, _meta persistence, argv + binary resolution"
```

---

### Task 2: Spawn/stream/kill runtime + detection (`agent.rs`, `Cargo.toml`)

The genuinely new part. A testable streaming runner (validated with a portable command), a detection call, and a process registry for cancellation.

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `process`, `io-util` to `tokio` features)
- Modify: `src-tauri/src/agent.rs`
- Test: inline `#[cfg(test)]` in `src-tauri/src/agent.rs` (an async test using the multi-thread runtime)

**Interfaces produced:**
- `build_command(program: &Path, args: &[String]) -> tokio::process::Command` — wraps `cmd /C` on Windows.
- `RunOutcome { code: Option<i32>, timed_out: bool }`.
- `async run_streaming(program, args, cwd, prompt, timeout, on_stdout, on_stderr) -> anyhow::Result<RunOutcome>` where the two callbacks are `FnMut(String) + Send`.
- `AgentStatus { agent: Agent, installed: bool, version: Option<String>, path: Option<String>, error: Option<String> }` — `Serialize`.
- `async detect(agent) -> AgentStatus` (spawns `<bin> --version`, 3s timeout).
- `AgentRegistry(Mutex<HashMap<String, tokio::process::Child>>)` with `insert`, `remove`, `kill(&str)`; plus `new_stream_id(seed: u64) -> String`.

- [ ] **Step 1: Add the tokio features**

In `src-tauri/Cargo.toml`, change the tokio line to:

```toml
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "process", "io-util", "sync"] }
```

- [ ] **Step 2: Write the failing test**

Add to `agent.rs`'s `mod tests`:

```rust
    #[tokio::test]
    async fn run_streaming_captures_output_and_exit() {
        // A portable command that prints two lines and exits 0.
        let (program, args): (std::path::PathBuf, Vec<String>) = if cfg!(windows) {
            (std::path::PathBuf::from("cmd"),
             vec!["/C".into(), "echo one& echo two".into()])
        } else {
            (std::path::PathBuf::from("/bin/sh"),
             vec!["-c".into(), "echo one; echo two".into()])
        };
        let mut lines: Vec<String> = Vec::new();
        let outcome = run_streaming(
            &program, &args, None, "",
            std::time::Duration::from_secs(30),
            |l| lines.push(l),
            |_e| {},
        ).await.unwrap();
        assert!(!outcome.timed_out);
        assert_eq!(outcome.code, Some(0));
        assert!(lines.iter().any(|l| l.trim() == "one"));
        assert!(lines.iter().any(|l| l.trim() == "two"));
    }

    #[tokio::test]
    async fn run_streaming_times_out_and_kills() {
        let (program, args): (std::path::PathBuf, Vec<String>) = if cfg!(windows) {
            // ping sleeps ~2s; timeout is 100ms so it must be killed.
            (std::path::PathBuf::from("cmd"),
             vec!["/C".into(), "ping -n 3 127.0.0.1 >NUL".into()])
        } else {
            (std::path::PathBuf::from("/bin/sh"), vec!["-c".into(), "sleep 2".into()])
        };
        let outcome = run_streaming(
            &program, &args, None, "",
            std::time::Duration::from_millis(100),
            |_l| {}, |_e| {},
        ).await.unwrap();
        assert!(outcome.timed_out);
    }
```

Note for the implementer: on Windows the test passes `program = "cmd"` directly, so `build_command` must NOT re-wrap a program that is already `cmd`. Keep `build_command`'s wrapping for real agent binaries; these tests call `run_streaming` with `cmd`/`sh` directly, and `run_streaming` should spawn `program` as given without re-wrapping (do the `cmd /C` wrapping in the higher-level agent spawn, or make `build_command` idempotent for `cmd`). Simplest: `run_streaming` spawns `program` + `args` verbatim; the agent-spawn command (Task 3) is responsible for building the `cmd /C <bin>` program+args before calling `run_streaming`.

- [ ] **Step 3: Write the implementation**

Add to `agent.rs` (imports at the top of the file):

```rust
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug)]
pub struct RunOutcome {
    pub code: Option<i32>,
    pub timed_out: bool,
}

/// Spawn `program args…` verbatim (no shell wrapping here), pipe `prompt` to
/// stdin, stream stdout/stderr lines to the callbacks, and enforce `timeout`.
pub async fn run_streaming(
    program: &Path,
    args: &[String],
    cwd: Option<&Path>,
    prompt: &str,
    timeout: std::time::Duration,
    mut on_stdout: impl FnMut(String) + Send,
    mut on_stderr: impl FnMut(String) + Send,
) -> anyhow::Result<RunOutcome> {
    use tokio::process::Command;
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes()).await;
        let _ = stdin.shutdown().await; // EOF so the CLI proceeds
    }
    let mut out = BufReader::new(child.stdout.take().expect("stdout piped")).lines();
    let mut errr = BufReader::new(child.stderr.take().expect("stderr piped")).lines();

    let pump = async {
        let mut out_open = true;
        let mut err_open = true;
        while out_open || err_open {
            tokio::select! {
                r = out.next_line(), if out_open => match r? {
                    Some(l) => on_stdout(l),
                    None => out_open = false,
                },
                r = errr.next_line(), if err_open => match r? {
                    Some(l) => on_stderr(l),
                    None => err_open = false,
                },
            }
        }
        let status = child.wait().await?;
        Ok::<RunOutcome, anyhow::Error>(RunOutcome { code: status.code(), timed_out: false })
    };

    match tokio::time::timeout(timeout, pump).await {
        Ok(res) => res,
        Err(_) => {
            // pump was dropped, which drops `child`; kill defensively is not possible
            // after the move, so kill happens by dropping (kill_on_drop). Enable it:
            Ok(RunOutcome { code: None, timed_out: true })
        }
    }
}
```

Important: for the timeout path to actually kill the process, set `cmd.kill_on_drop(true)` right after `Command::new` (add it to the builder chain). When the `pump` future is dropped on timeout, the owned `child` is dropped and killed. Add `.kill_on_drop(true)` to the `cmd` builder.

Now detection and the registry:

```rust
#[derive(Serialize, Debug, Clone)]
pub struct AgentStatus {
    pub agent: Agent,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub error: Option<String>,
}

pub async fn detect(agent: Agent) -> AgentStatus {
    let resolved = resolve_bin(agent);
    let path = resolved.as_ref().map(|p| p.to_string_lossy().to_string());
    if resolved.is_none() {
        return AgentStatus { agent, installed: false, version: None, path: None,
            error: Some(format!("`{}` not found on PATH", agent.bin())) };
    }
    // Run `<bin> --version` verbatim via run_streaming with a short timeout.
    let (program, args) = spawn_target(resolved.as_ref().unwrap(), &["--version".to_string()]);
    let mut lines: Vec<String> = Vec::new();
    let outcome = run_streaming(&program, &args, None, "",
        std::time::Duration::from_secs(3),
        |l| lines.push(l), |_e| {}).await;
    match outcome {
        Ok(o) if !o.timed_out && o.code == Some(0) => AgentStatus {
            agent, installed: true, version: Some(lines.join(" ").trim().to_string()),
            path, error: None },
        Ok(o) if o.timed_out => AgentStatus { agent, installed: false, version: None, path,
            error: Some("`--version` timed out after 3s".into()) },
        Ok(o) => AgentStatus { agent, installed: false, version: None, path,
            error: Some(format!("`--version` exited with {:?}", o.code)) },
        Err(e) => AgentStatus { agent, installed: false, version: None, path,
            error: Some(e.to_string()) },
    }
}

/// Turn a resolved binary path + args into the actual (program, args) to spawn,
/// wrapping through `cmd /C` on Windows so `.cmd` shims execute.
pub fn spawn_target(bin: &Path, args: &[String]) -> (PathBuf, Vec<String>) {
    if cfg!(windows) {
        let mut a = vec!["/C".to_string(), bin.to_string_lossy().to_string()];
        a.extend(args.iter().cloned());
        (PathBuf::from("cmd"), a)
    } else {
        (bin.to_path_buf(), args.to_vec())
    }
}

#[derive(Default)]
pub struct AgentRegistry(pub Mutex<HashMap<String, tokio::process::Child>>);

impl AgentRegistry {
    pub fn insert(&self, id: String, child: tokio::process::Child) {
        if let Ok(mut g) = self.0.lock() { g.insert(id, child); }
    }
    pub fn remove(&self, id: &str) -> Option<tokio::process::Child> {
        self.0.lock().ok().and_then(|mut g| g.remove(id))
    }
    pub async fn kill(&self, id: &str) -> bool {
        if let Some(mut child) = self.remove(id) { let _ = child.start_kill(); true } else { false }
    }
}

/// A stream id derived from a caller-supplied seed (no time/random in library code).
pub fn new_stream_id(seed: u64) -> String {
    format!("agent-{seed:016x}")
}
```

Note: the registry-based cancel (Task 3) uses a spawn path that keeps the `Child` in the registry rather than `run_streaming` (which owns its child). Task 3 documents the command-level spawn that emits events and registers the child; `run_streaming` remains the unit-tested core for detection and the timeout test.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test agent::` (runs the sync + async tests)
Expected: PASS. If the timeout test flakes, confirm `kill_on_drop(true)` is set.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/agent.rs
git commit -m "feat(agent): streaming spawn runtime, detection, and process registry"
```

---

### Task 3: Commands swap + remove the old llm module (`commands.rs`, `lib.rs`, delete `llm.rs`)

Replace the `*_llm_*` commands with agent commands, register them, delete `llm.rs`, and best-effort clean up any leftover `llm_key_*` keychain entries.

**Files:**
- Modify: `src-tauri/src/commands.rs` (remove `use crate::llm;` and the 5 llm commands; add `use crate::agent;`, an `AgentRegistry` field on `AppState`, and the agent commands)
- Modify: `src-tauri/src/lib.rs` (remove `pub mod llm;`; swap the 5 llm handler entries for the agent ones; add `agents: agent::AgentRegistry::default()` to the managed `AppState`)
- Delete: `src-tauri/src/llm.rs`

**Interfaces produced (Tauri commands):** `get_agent_settings`, `set_agent_settings`, `detect_agents`, `agent_run`, `agent_cancel`.

- [ ] **Step 1: Update `AppState` and remove llm wiring**

- In `commands.rs`, delete `use crate::llm;` and the five `*_llm_*` command fns (`get_llm_settings`, `set_llm_settings`, `set_llm_key`, `clear_llm_key`, `test_llm_connection`).
- Add `use crate::agent::{self, Agent, AgentRegistry};`.
- Add a field to `AppState`: `pub agents: AgentRegistry,`.
- In `lib.rs`, in the `.manage(AppState { … })` initializer, add `agents: agent::AgentRegistry::default(),`. Remove `pub mod llm;`.

- [ ] **Step 2: Add the agent commands to `commands.rs`**

```rust
// ── Local CLI agents ──────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct AgentSettingsView {
    pub active_agent: Option<Agent>,
    pub claude_code: agent::AgentConfig,
    pub codex: agent::AgentConfig,
    pub statuses: Vec<agent::AgentStatus>,
}

#[tauri::command]
pub async fn get_agent_settings(state: State<'_, AppState>) -> CmdResult<AgentSettingsView> {
    let settings = with_store(state.inner(), |s| agent::AgentSettings::load(s))?;
    let mut statuses = Vec::new();
    for a in Agent::all() {
        statuses.push(agent::detect(a).await);
    }
    Ok(AgentSettingsView {
        active_agent: settings.active_agent,
        claude_code: settings.claude_code.clone(),
        codex: settings.codex.clone(),
        statuses,
    })
}

#[tauri::command]
pub async fn set_agent_settings(
    state: State<'_, AppState>,
    settings: agent::AgentSettings,
) -> CmdResult<()> {
    with_store(state.inner(), |s| {
        settings.save(s)?;
        s.audit(&who(state.inner()), "settings.agent.update", None, None)?;
        Ok(())
    })
}

#[tauri::command]
pub async fn detect_agents() -> CmdResult<Vec<agent::AgentStatus>> {
    let mut v = Vec::new();
    for a in Agent::all() { v.push(agent::detect(a).await); }
    Ok(v)
}

#[tauri::command]
pub async fn agent_run(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_kind: Agent,
    prompt: String,
) -> CmdResult<String> {
    // Load config + resolve the binary without holding the store lock across spawn.
    let settings = with_store(state.inner(), |s| agent::AgentSettings::load(s))?;
    let config = settings.config(agent_kind).clone();
    let bin = agent::resolve_bin(agent_kind)
        .ok_or_else(|| format!("`{}` is not installed", agent_kind.bin()))?;
    let argv = agent::build_argv(agent_kind, &config);
    let (program, args) = agent::spawn_target(&bin, &argv);

    let stream_id = agent::new_stream_id(state.next_stream_seed());
    with_store(state.inner(), |s| {
        s.audit(&who(state.inner()), "agent.run", Some(agent_kind.as_str()), None)
    })?;

    // Spawn a background task that streams events. The prompt is piped to stdin.
    let app2 = app.clone();
    let sid = stream_id.clone();
    let cwd = config.working_dir.clone();
    let timeout = std::time::Duration::from_secs(config.timeout_minutes.max(1) * 60);
    tauri::async_runtime::spawn(async move {
        let sid_out = sid.clone();
        let sid_err = sid.clone();
        let app_out = app2.clone();
        let app_err = app2.clone();
        let outcome = agent::run_streaming(
            &program, &args, cwd.as_deref().map(std::path::Path::new), &prompt, timeout,
            move |line| { let _ = app_out.emit("agent:output", serde_json::json!({"stream_id": sid_out, "line": line})); },
            move |line| { let _ = app_err.emit("agent:error", serde_json::json!({"stream_id": sid_err, "line": line})); },
        ).await;
        let (code, timed_out) = match outcome { Ok(o) => (o.code, o.timed_out), Err(_) => (None, false) };
        let _ = app2.emit("agent:exit", serde_json::json!({"stream_id": sid, "code": code, "timed_out": timed_out}));
    });

    Ok(stream_id)
}

#[tauri::command]
pub async fn agent_cancel(state: State<'_, AppState>, stream_id: String) -> CmdResult<()> {
    state.agents.kill(&stream_id).await;
    Ok(())
}
```

Implementation note on cancellation and `next_stream_seed`: because `run_streaming` owns its child, the simplest correct cancel is to make `agent_run`'s background task honor a cancel flag/token stored in the registry keyed by `stream_id`, OR switch `agent_run` to a spawn that stores the `Child` in `state.agents` and does the streaming without `run_streaming`. Pick ONE and keep it consistent:
- Preferred: add `AgentRegistry` a `CancellationToken` per id (use `tokio::sync::Notify` or an `AtomicBool` map — no new crate) and have the pump `select!` on it; `agent_cancel` triggers it. If you keep `kill_on_drop(true)`, dropping the task via an aborted `JoinHandle` also kills the child. So: store the `JoinHandle` (abort handle) in `state.agents` keyed by `stream_id`, and `agent_cancel` calls `handle.abort()` — the dropped task drops the child, which is killed by `kill_on_drop`. Adjust `AgentRegistry` to hold `Mutex<HashMap<String, tokio::task::JoinHandle<()>>>` instead of `Child`, with `kill` = `abort()` + remove. Update Task 2's registry type accordingly if you choose this (it is the recommended shape).
- `state.next_stream_seed()`: add a `stream_seq: std::sync::atomic::AtomicU64` to `AppState` and a method returning `self.stream_seq.fetch_add(1, Relaxed)` — a monotonic seed without time/random.

Make the registry hold abort handles (recommended). Update Task 2's `AgentRegistry` to store `JoinHandle<()>`; `insert(id, handle)`, `kill(id)` = remove + `abort()`. `agent_run` inserts the handle after spawning.

- [ ] **Step 3: Register commands + build**

In `lib.rs` `generate_handler!`, replace the five `commands::*_llm_*` entries with:

```rust
            commands::get_agent_settings,
            commands::set_agent_settings,
            commands::detect_agents,
            commands::agent_run,
            commands::agent_cancel,
```

Run: `cd src-tauri && cargo build` — fix compile errors (the deleted `llm.rs` must have no remaining references; `AppState` initializer must include `agents` and `stream_seq`).

- [ ] **Step 4: Best-effort keychain cleanup**

In `lib.rs` `setup` (or a one-shot in `AppState` construction), best-effort delete leftover keys, ignoring errors:

```rust
for name in ["llm_key_anthropic","llm_key_openai","llm_key_google","llm_key_custom"] {
    let _ = secrets::Secrets::default_service().delete(name);
}
```

- [ ] **Step 5: Run the full suite**

Run: `cd src-tauri && cargo test`
Expected: PASS (agent tests + pre-existing suites; no llm tests remain).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git rm src-tauri/src/llm.rs
git commit -m "feat(agent): agent commands + registry; remove llm provider module"
```

---

### Task 4: Frontend API — swap llm bindings for agent bindings (`api.ts`, `api.test.ts`)

**Files:**
- Modify: `src/api.ts` (remove the llm block; add the agent block + event listeners)
- Modify: `src/api.test.ts` (replace the llm wrappers test)

**Interfaces produced:** types `Agent`, `AgentConfig`, `AgentStatus`, `AgentSettingsView`, `AgentSettings`; const `AGENTS`; fns `getAgentSettings`, `setAgentSettings`, `detectAgents`, `agentRun`, `agentCancel`; listeners `onAgentOutput`, `onAgentError`, `onAgentExit`.

- [ ] **Step 1: Replace the llm test with the agent test**

In `src/api.test.ts`, remove the `it("llm settings wrappers …")` block and add:

```ts
  it("agent wrappers use the exact command names", async () => {
    invoke.mockResolvedValue(undefined);
    const cfg = { model: null, working_dir: null, permission_mode: "plan", sandbox_mode: null,
      timeout_minutes: 10, isolate: false, extra_args: [] };
    const settings = { active_agent: "claude-code" as const, claude_code: cfg, codex: cfg };
    await api.getAgentSettings();
    await api.setAgentSettings(settings);
    await api.detectAgents();
    await api.agentRun("claude-code", "hello");
    await api.agentCancel("agent-0001");
    expect(invoke.mock.calls).toEqual([
      ["get_agent_settings"],
      ["set_agent_settings", { settings }],
      ["detect_agents"],
      ["agent_run", { agentKind: "claude-code", prompt: "hello" }],
      ["agent_cancel", { streamId: "agent-0001" }],
    ]);
    expect([...api.AGENTS]).toEqual(["claude-code", "codex"]);
  });
```

Note: Tauri maps snake_case command params to camelCase JS keys, so the Rust `agent_kind`/`stream_id` params are sent as `agentKind`/`streamId`. Verify against the existing wrappers' convention when implementing (the existing `set_field_withheld` wrapper shows the arg-key mapping).

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm test -- api.test`
Expected: FAIL.

- [ ] **Step 3: Swap the bindings in `api.ts`**

Remove the llm block (the `LlmProvider`/`PROVIDERS`/`Llm*`/`getLlmSettings`/… additions). Add:

```ts
export type Agent = "claude-code" | "codex";
export const AGENTS: Agent[] = ["claude-code", "codex"];

export interface AgentConfig {
  model: string | null; working_dir: string | null;
  permission_mode: string | null; sandbox_mode: string | null;
  timeout_minutes: number; isolate: boolean; extra_args: string[];
}
export interface AgentStatus {
  agent: Agent; installed: boolean; version: string | null; path: string | null; error: string | null;
}
export interface AgentSettingsView {
  active_agent: Agent | null; claude_code: AgentConfig; codex: AgentConfig; statuses: AgentStatus[];
}
export interface AgentSettings {
  active_agent: Agent | null; claude_code: AgentConfig; codex: AgentConfig;
}

export const getAgentSettings = () => invoke<AgentSettingsView>("get_agent_settings");
export const setAgentSettings = (settings: AgentSettings) => invoke<void>("set_agent_settings", { settings });
export const detectAgents = () => invoke<AgentStatus[]>("detect_agents");
export const agentRun = (agentKind: Agent, prompt: string) =>
  invoke<string>("agent_run", { agentKind, prompt });
export const agentCancel = (streamId: string) => invoke<void>("agent_cancel", { streamId });

export const onAgentOutput = (cb: (p: { stream_id: string; line: string }) => void): Promise<UnlistenFn> =>
  listen<{ stream_id: string; line: string }>("agent:output", (e) => cb(e.payload));
export const onAgentError = (cb: (p: { stream_id: string; line: string }) => void): Promise<UnlistenFn> =>
  listen<{ stream_id: string; line: string }>("agent:error", (e) => cb(e.payload));
export const onAgentExit = (cb: (p: { stream_id: string; code: number | null; timed_out: boolean }) => void): Promise<UnlistenFn> =>
  listen<{ stream_id: string; code: number | null; timed_out: boolean }>("agent:exit", (e) => cb(e.payload));
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm test -- api.test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/api.ts src/api.test.ts
git commit -m "feat(agent): typed api bindings and stream listeners; drop llm bindings"
```

---

### Task 5: Settings page rewrite (`SettingsPage.tsx`)

**Files:**
- Modify: `src/pages/SettingsPage.tsx` (rewrite the AI Agent section for local agents)

- [ ] **Step 1: Rewrite `SettingsPage.tsx`**

Replace the file body with a local-agent UI. Requirements:
- On mount, `getAgentSettings()`; render a detection card per agent from `view.statuses` (installed ✓ + version, or "not found" + `error`), with a **Re-check** button calling `detectAgents()` and updating `statuses`.
- An agent selector bound to `active_agent`; for the selected agent, config fields: `model` (text, empty ⇒ null), `working_dir` (text, empty ⇒ null), `timeout_minutes` (number), `isolate` (checkbox); for `claude-code` a `permission_mode` `<Select>` (plan/default/acceptEdits/bypassPermissions); for `codex` a `sandbox_mode` `<Select>` (read-only/workspace-write/danger-full-access). **Save** calls `setAgentSettings` with `active_agent` set to the selected agent.
- A **Test run** panel: a prompt `<Textarea>`, a **Run** button that calls `agentRun(selected, prompt)` and stores the returned `streamId`; subscribe via `onAgentOutput`/`onAgentError`/`onAgentExit` (filter by `streamId`) and append lines to a scrolling `<pre>`; show exit status; a **Cancel** button calls `agentCancel(streamId)`. Register listeners in a `useEffect` and return their unlisten fns for cleanup.
- No API-key field, no egress warning. Reuse `Button, Alert, Card, Field, Input, Select, Textarea` and `PageTitle`. Match the `{...}={undefined}` prop convention used elsewhere for the repo's `checkJs`.

Because this is UI with live streaming, write it to satisfy `npm run typecheck` and verify behavior manually (next step). Follow `SegmentsPage.tsx` for component usage patterns and the existing `onScanProgress` usage in `api.ts`/pages for the listen/unlisten pattern.

- [ ] **Step 2: Typecheck**

Run: `npm run typecheck`
Expected: 0 errors. Adjust component props to the real design-system APIs if needed (`Textarea` is exported from `../design-system`).

- [ ] **Step 3: Manual verification (real app)**

Run: `npm run tauri dev`
Confirm:
1. Settings shows both agents; Claude Code and Codex both detected with versions (they are installed).
2. Set Claude Code active, permission mode `plan`, enter a **Test run** prompt like "Say hello in one word", click Run → streamed `agent:output` JSON lines appear, then an exit status.
3. Cancel works mid-run; a long run hitting the timeout reports a timeout exit.
4. Switch to Codex, sandbox `read-only`, run a trivial prompt → streams and exits.
5. Reload the app → settings persisted; Audit page shows `settings.agent.update` and `agent.run` rows (no prompt text in detail).

- [ ] **Step 4: Commit**

```bash
git add src/pages/SettingsPage.tsx
git commit -m "feat(agent): Settings page — detection, per-agent config, streaming test run"
```

---

## Final verification

- [ ] `cd src-tauri && cargo test` green; `npm test` green; `npm run typecheck` clean.
- [ ] `git grep -n "llm" src-tauri/src src/api.ts` returns nothing meaningful (module fully removed).
- [ ] `git status` shows only intended files changed on `feat/llm-provider-settings`.

## Self-Review notes (author)

- **Spec coverage:** §4 config → T1; §5.2 resolve → T1; §5.3 argv → T1; §5.4 spawn/stream/kill → T2/T3; §5.5 commands → T3; §6.1 api → T4; §6.2 SettingsPage → T5; §9 cleanup (delete llm.rs, remove commands/bindings, keychain cleanup) → T3/T4.
- **Compiles every task:** T1/T2 add `agent.rs` beside `llm.rs`; T3 swaps commands and deletes `llm.rs` in the same task so no dangling references; T4/T5 are frontend-only.
- **Cancellation decision:** `AgentRegistry` stores task **abort handles**; `agent_cancel` aborts, and `kill_on_drop(true)` kills the child when the aborted task drops it. T2 builds the registry shape; T3 wires it. Implementers must keep these consistent (noted in both tasks).
- **No secrets:** nothing writes a keychain key; T3 best-effort deletes leftover `llm_key_*`.
