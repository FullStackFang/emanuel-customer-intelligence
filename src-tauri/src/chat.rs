//! Chat transports behind one `ChatBackend` trait. Two keyless implementations:
//!
//!   * `CliAgentBackend` — drives the Claude Code / Codex CLIs through `agent::run_streaming`,
//!     locked down so their only route to any data is the governed snapshot piped over stdin
//!     (no MCP, no Bash/Read/Write, no data working directory). Auth is each CLI's own login.
//!   * `OllamaBackend` — a keyless streaming client to a local Ollama server's native
//!     `POST /api/chat`. Nothing leaves the machine.
//!
//! The governed snapshot is the sole model input for every backend (see `chat_context`);
//! selecting a different backend never changes what data is exposed.
//!
//! Multi-turn: every backend replays the full composed context each turn (snapshot + prior
//! turns + new message). Replay is the continuity mechanism — uniform across backends and
//! strictly robust — rather than the CLIs' `--resume`, whose session state cannot be verified
//! from here. The CLI backends still capture and surface the agent's own session id so a
//! conversation can record it (`StreamOutcome::session_id`).

use crate::agent::{self, Agent};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// One conversation turn. `role` is "user" or "assistant".
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> ChatMessage {
        ChatMessage { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> ChatMessage {
        ChatMessage { role: "assistant".into(), content: content.into() }
    }
}

/// The three user-facing backends. Claude and ChatGPT are CLI agents; Ollama is local HTTP.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    Ollama,
    Claude,
    ChatGpt,
}

impl BackendKind {
    pub fn all() -> [BackendKind; 3] {
        [BackendKind::Ollama, BackendKind::Claude, BackendKind::ChatGpt]
    }
    /// The wire/persistence string (matches the serde kebab-case representation).
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendKind::Ollama => "ollama",
            BackendKind::Claude => "claude",
            BackendKind::ChatGpt => "chat-gpt",
        }
    }
    /// The CLI agent a backend maps to, or None for the local HTTP backend.
    pub fn agent(&self) -> Option<Agent> {
        match self {
            BackendKind::Claude => Some(Agent::ClaudeCode),
            BackendKind::ChatGpt => Some(Agent::Codex),
            BackendKind::Ollama => None,
        }
    }
}

/// A cooperative cancel flag: set true to ask an in-progress stream to stop emitting and return.
/// (The Tauri command layer also aborts the task, which kills any CLI child via kill_on_drop.)
pub type Cancel = Arc<AtomicBool>;

pub fn new_cancel() -> Cancel {
    Arc::new(AtomicBool::new(false))
}

/// What a completed stream produced: the full assistant text and, for CLI backends, the agent's
/// own session id (captured for the conversation record). `prompt_tokens`/`completion_tokens`
/// are populated only when the backend reports them (Ollama's final chunk does; the CLI agents
/// leave them `None`) and feed the per-turn telemetry event — they are counts only, never content.
#[derive(Debug, Clone, Default)]
pub struct StreamOutcome {
    pub text: String,
    pub session_id: Option<String>,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
}

/// A chat transport. The snapshot is the sole governed data input; history + user_msg are the
/// conversation. Tokens are delivered incrementally through `on_token` as they are produced.
#[allow(async_fn_in_trait)]
pub trait ChatBackend {
    async fn stream(
        &self,
        snapshot: &str,
        history: &[ChatMessage],
        user_msg: &str,
        on_token: &mut (dyn FnMut(&str) + Send),
        cancel: &Cancel,
    ) -> Result<StreamOutcome>;
}

/// Overall wall-clock ceiling for a single CLI chat turn (matches `agent.rs` timeout style).
const CLI_TURN_TIMEOUT: Duration = Duration::from_secs(300);

// ── prompt composition (CLI agents: one stdin prompt) ────────────────────────

/// Compose the single text prompt a CLI agent receives over stdin: the governed snapshot as
/// grounding, the prior turns, then the new question. Replayed in full each turn so context is
/// preserved without relying on the CLI's own session state.
pub fn compose_prompt(snapshot: &str, history: &[ChatMessage], user_msg: &str) -> String {
    let mut p = String::with_capacity(snapshot.len() + user_msg.len() + 512);
    p.push_str(snapshot.trim_end());
    p.push_str("\n\n");
    if !history.is_empty() {
        p.push_str("## Conversation so far\n\n");
        for m in history {
            let who = if m.role == "assistant" { "Assistant" } else { "User" };
            p.push_str(who);
            p.push_str(": ");
            p.push_str(m.content.trim());
            p.push_str("\n\n");
        }
    }
    p.push_str("## New question\n\n");
    p.push_str("User: ");
    p.push_str(user_msg.trim());
    p.push_str("\n\nAssistant:");
    p
}

// ── CLI lockdown argv ────────────────────────────────────────────────────────

/// Claude Code chat argv: streamed JSON with partial-message deltas for token-level streaming,
/// an empty strict MCP config, editing/shell tools disallowed, and NO `--add-dir` — the agent's
/// only input is the snapshot on stdin. Never carries a store path or key.
pub fn claude_chat_argv(model: Option<&str>) -> Vec<String> {
    let mut v: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        // No external agent configuration: strict, empty MCP.
        "--strict-mcp-config",
        "--mcp-config",
        "{}",
        // File-editing and shell tools disabled; the agent answers, it never acts.
        "--disallowed-tools",
        "Bash Edit Write Read WebFetch WebSearch NotebookEdit",
        // Answer normally but take no actions.
        "--permission-mode",
        "default",
    ]
    .map(String::from)
    .to_vec();
    if let Some(m) = model {
        v.push("--model".into());
        v.push(m.to_string());
    }
    v
}

/// Codex chat argv: read-only sandbox, streamed JSON. The working directory is a fresh empty temp
/// dir passed to the process (not `-C`), so it is neither the repo nor the store directory. Never
/// carries a store path or key.
pub fn codex_chat_argv(model: Option<&str>) -> Vec<String> {
    let mut v: Vec<String> = ["exec", "--json", "--skip-git-repo-check", "-s", "read-only"]
        .map(String::from)
        .to_vec();
    if let Some(m) = model {
        v.push("-m".into());
        v.push(m.to_string());
    }
    v
}

fn chat_argv(agent: Agent, model: Option<&str>) -> Vec<String> {
    match agent {
        Agent::ClaudeCode => claude_chat_argv(model),
        Agent::Codex => codex_chat_argv(model),
    }
}

// ── a fresh empty working directory, removed on drop ─────────────────────────

/// An empty scratch directory under the OS temp dir, deleted when dropped. Used as a CLI agent's
/// working directory so it never runs in the repo or the store directory. Not the `tempfile`
/// crate (a dev-dependency only); a unique name from pid + a counter suffices for a scratch cwd.
struct ScratchCwd(PathBuf);

impl ScratchCwd {
    fn new(seed: u64) -> Result<ScratchCwd> {
        let unique = format!("emanuel-chat-{}-{seed:016x}", std::process::id());
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path)?;
        Ok(ScratchCwd(path))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchCwd {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ── CLI agent backend ────────────────────────────────────────────────────────

pub struct CliAgentBackend {
    pub agent: Agent,
    pub model: Option<String>,
    /// A seed for the scratch-dir name (no time/random in library code).
    pub seed: u64,
}

impl ChatBackend for CliAgentBackend {
    async fn stream(
        &self,
        snapshot: &str,
        history: &[ChatMessage],
        user_msg: &str,
        on_token: &mut (dyn FnMut(&str) + Send),
        cancel: &Cancel,
    ) -> Result<StreamOutcome> {
        let bin = agent::resolve_bin(self.agent)
            .ok_or_else(|| anyhow!("`{}` is not installed or not on PATH", self.agent.bin()))?;
        let args = chat_argv(self.agent, self.model.as_deref());
        let (program, spawn_args) = agent::spawn_target(&bin, &args);
        let cwd = ScratchCwd::new(self.seed)?;
        let prompt = compose_prompt(snapshot, history, user_msg);

        let mut parser = AgentParser::new(self.agent);
        let mut stderr_tail: Vec<String> = Vec::new();
        let outcome = agent::run_streaming(
            &program,
            &spawn_args,
            Some(cwd.path()),
            &prompt,
            CLI_TURN_TIMEOUT,
            |line| {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                for tok in parser.push_line(&line) {
                    on_token(&tok);
                }
            },
            |line| {
                // Keep a short tail of stderr for a useful error if the run fails.
                stderr_tail.push(line);
                if stderr_tail.len() > 20 {
                    stderr_tail.remove(0);
                }
            },
        )
        .await?;

        if cancel.load(Ordering::Relaxed) {
            return Ok(StreamOutcome { text: parser.text, session_id: parser.session_id, ..Default::default() });
        }
        if outcome.timed_out {
            return Err(anyhow!("{} timed out", self.agent.bin()));
        }
        if parser.text.trim().is_empty() {
            let tail = stderr_tail.join("\n");
            let hint = if tail.trim().is_empty() {
                format!("{} produced no answer (exit {:?})", self.agent.bin(), outcome.code)
            } else {
                format!("{} produced no answer: {tail}", self.agent.bin())
            };
            return Err(anyhow!(hint));
        }
        Ok(StreamOutcome { text: parser.text, session_id: parser.session_id, ..Default::default() })
    }
}

// ── Ollama backend (keyless local HTTP) ──────────────────────────────────────

pub struct OllamaBackend {
    pub base_url: String,
    pub model: String,
}

/// One newline-delimited JSON object from Ollama's `/api/chat` stream. The final (`done`)
/// chunk also carries `prompt_eval_count` / `eval_count` — the prompt and completion token
/// counts — which the per-turn telemetry event records.
#[derive(Deserialize)]
struct OllamaChunk {
    #[serde(default)]
    message: Option<OllamaMsg>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
    #[serde(default)]
    eval_count: Option<u64>,
}
#[derive(Deserialize)]
struct OllamaMsg {
    #[serde(default)]
    content: String,
}

fn base(url: &str) -> &str {
    url.trim().trim_end_matches('/')
}

impl OllamaBackend {
    /// The `/api/chat` request body: the snapshot as the system message, the prior turns, then
    /// the new user message. Streaming on. Keyless; no auth header.
    fn body(&self, snapshot: &str, history: &[ChatMessage], user_msg: &str) -> serde_json::Value {
        let mut messages = Vec::with_capacity(history.len() + 2);
        messages.push(serde_json::json!({ "role": "system", "content": snapshot }));
        for m in history {
            let role = if m.role == "assistant" { "assistant" } else { "user" };
            messages.push(serde_json::json!({ "role": role, "content": m.content }));
        }
        messages.push(serde_json::json!({ "role": "user", "content": user_msg }));
        serde_json::json!({ "model": self.model, "messages": messages, "stream": true })
    }
}

impl ChatBackend for OllamaBackend {
    async fn stream(
        &self,
        snapshot: &str,
        history: &[ChatMessage],
        user_msg: &str,
        on_token: &mut (dyn FnMut(&str) + Send),
        cancel: &Cancel,
    ) -> Result<StreamOutcome> {
        let url = format!("{}/api/chat", base(&self.base_url));
        let body = self.body(snapshot, history, user_msg);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()?;
        let mut resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("could not reach the local Ollama server at {url}: {e}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!("Ollama returned HTTP {}", resp.status().as_u16()));
        }

        let mut text = String::new();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp.chunk().await? {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            buf.extend_from_slice(&chunk);
            // Ollama streams one JSON object per line (NDJSON); process complete lines.
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = &line[..line.len() - 1];
                if line.is_empty() {
                    continue;
                }
                if let Ok(evt) = serde_json::from_slice::<OllamaChunk>(line) {
                    if let Some(e) = evt.error {
                        return Err(anyhow!("Ollama error: {e}"));
                    }
                    if let Some(m) = evt.message {
                        if !m.content.is_empty() {
                            text.push_str(&m.content);
                            on_token(&m.content);
                        }
                    }
                    if evt.done {
                        return Ok(StreamOutcome {
                            text,
                            session_id: None,
                            prompt_tokens: evt.prompt_eval_count,
                            completion_tokens: evt.eval_count,
                        });
                    }
                }
            }
        }
        Ok(StreamOutcome { text, session_id: None, ..Default::default() })
    }
}

/// Best-effort reachability probe for the local Ollama server (no key). True when `/api/tags`
/// answers 2xx within a short timeout.
pub async fn ollama_reachable(base_url: &str) -> bool {
    let url = format!("{}/api/tags", base(base_url));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    matches!(client.get(&url).send().await, Ok(r) if r.status().is_success())
}

// ── per-agent event parsing ──────────────────────────────────────────────────

/// Extracts assistant text (and the agent's session id) from a CLI agent's JSON event stream.
/// Isolated per agent so a CLI format change touches only this parser. Prefers streamed token
/// deltas; a full message event is used only when no deltas were seen, so text never doubles.
struct AgentParser {
    agent: Agent,
    saw_delta: bool,
    pub text: String,
    pub session_id: Option<String>,
}

impl AgentParser {
    fn new(agent: Agent) -> AgentParser {
        AgentParser { agent, saw_delta: false, text: String::new(), session_id: None }
    }

    /// Parse one stdout line and return any assistant-text fragments to emit.
    fn push_line(&mut self, line: &str) -> Vec<String> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            return Vec::new();
        };
        match self.agent {
            Agent::ClaudeCode => self.push_claude(&v),
            Agent::Codex => self.push_codex(&v),
        }
    }

    fn emit(&mut self, s: &str) -> Vec<String> {
        if s.is_empty() {
            return Vec::new();
        }
        self.text.push_str(s);
        vec![s.to_string()]
    }

    /// Claude Code `stream-json` (with `--include-partial-messages`):
    ///   - `system`/`init` and `result` events carry `session_id`.
    ///   - `stream_event` → `content_block_delta` → `delta.text` are the streamed tokens.
    ///   - a full `assistant` message event is the fallback when no deltas were seen.
    fn push_claude(&mut self, v: &serde_json::Value) -> Vec<String> {
        if let Some(sid) = v.get("session_id").and_then(|s| s.as_str()) {
            self.session_id = Some(sid.to_string());
        }
        match v.get("type").and_then(|t| t.as_str()) {
            Some("stream_event") => {
                let ev = v.get("event");
                let is_delta = ev
                    .and_then(|e| e.get("type"))
                    .and_then(|t| t.as_str())
                    == Some("content_block_delta");
                if is_delta {
                    if let Some(t) = ev
                        .and_then(|e| e.get("delta"))
                        .and_then(|d| d.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        self.saw_delta = true;
                        return self.emit(t);
                    }
                }
                Vec::new()
            }
            Some("assistant") => {
                if self.saw_delta {
                    return Vec::new();
                }
                let mut out = Vec::new();
                if let Some(content) = v
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                {
                    for block in content {
                        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                            if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                                out.extend(self.emit(t));
                            }
                        }
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    }

    /// Codex `exec --json`: events are wrapped in a `msg` object.
    ///   - `session_configured` / a top-level `session_id` carry the session id.
    ///   - `agent_message_delta` → `delta` are the streamed tokens.
    ///   - `agent_message` → `message` is the fallback full text.
    /// Also tolerates the alternate `item.completed`/`item.delta` shape some versions emit.
    fn push_codex(&mut self, v: &serde_json::Value) -> Vec<String> {
        for key in ["session_id", "conversation_id"] {
            if let Some(sid) = v.get(key).and_then(|s| s.as_str()) {
                self.session_id = Some(sid.to_string());
            }
        }
        if let Some(msg) = v.get("msg") {
            if let Some(sid) = msg.get("session_id").and_then(|s| s.as_str()) {
                self.session_id = Some(sid.to_string());
            }
            match msg.get("type").and_then(|t| t.as_str()) {
                Some("agent_message_delta") => {
                    if let Some(d) = msg.get("delta").and_then(|d| d.as_str()) {
                        self.saw_delta = true;
                        return self.emit(d);
                    }
                }
                Some("agent_message") => {
                    if self.saw_delta {
                        return Vec::new();
                    }
                    if let Some(t) = msg.get("message").and_then(|t| t.as_str()) {
                        return self.emit(t);
                    }
                }
                _ => {}
            }
            return Vec::new();
        }
        // Alternate item.* shape.
        match v.get("type").and_then(|t| t.as_str()) {
            Some("item.delta") => {
                if let Some(t) = v.get("delta").and_then(|d| d.get("text")).and_then(|t| t.as_str()) {
                    self.saw_delta = true;
                    return self.emit(t);
                }
            }
            Some("item.completed") => {
                if self.saw_delta {
                    return Vec::new();
                }
                let item = v.get("item");
                let is_msg = item
                    .and_then(|i| i.get("type"))
                    .and_then(|t| t.as_str())
                    .map(|t| t.contains("message"))
                    .unwrap_or(false);
                if is_msg {
                    if let Some(t) = item.and_then(|i| i.get("text")).and_then(|t| t.as_str()) {
                        return self.emit(t);
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 2.7 prompt composition ───────────────────────────────────────────────
    #[test]
    fn compose_prompt_orders_snapshot_history_then_question() {
        let hist = vec![
            ChatMessage::user("who joined most?"),
            ChatMessage::assistant("FY2015."),
        ];
        let p = compose_prompt("SNAPSHOT-BODY", &hist, "and who left?");
        let snap = p.find("SNAPSHOT-BODY").unwrap();
        let hist_hdr = p.find("Conversation so far").unwrap();
        let q = p.find("New question").unwrap();
        assert!(snap < hist_hdr && hist_hdr < q, "order: snapshot, history, question");
        assert!(p.contains("User: who joined most?"));
        assert!(p.contains("Assistant: FY2015."));
        assert!(p.trim_end().ends_with("User: and who left?\n\nAssistant:"));
    }

    #[test]
    fn compose_prompt_without_history_has_no_history_section() {
        let p = compose_prompt("SNAP", &[], "hello");
        assert!(!p.contains("Conversation so far"));
        assert!(p.starts_with("SNAP"));
        assert!(p.contains("User: hello"));
    }

    // ── 2.4 / 2.7 lockdown argv ──────────────────────────────────────────────
    #[test]
    fn claude_lockdown_argv_is_isolated_and_read_only() {
        let a = claude_chat_argv(Some("claude-sonnet-5"));
        assert!(a.iter().any(|x| x == "--strict-mcp-config"));
        // Empty MCP config: no servers.
        let i = a.iter().position(|x| x == "--mcp-config").unwrap();
        assert_eq!(a[i + 1], "{}");
        // File/shell tools disallowed.
        assert!(a.iter().any(|x| x == "--disallowed-tools"));
        assert!(a.iter().any(|x| x.contains("Bash") && x.contains("Write") && x.contains("Read")));
        // Never a data working directory.
        assert!(!a.iter().any(|x| x == "--add-dir"));
        assert!(a.windows(2).any(|w| w == ["--model", "claude-sonnet-5"]));
    }

    #[test]
    fn codex_lockdown_argv_is_read_only_and_has_no_cwd_flag() {
        let a = codex_chat_argv(None);
        assert!(a.starts_with(&["exec".to_string(), "--json".into(), "--skip-git-repo-check".into()]));
        assert!(a.windows(2).any(|w| w == ["-s", "read-only"]));
        // cwd is supplied to the process (a fresh temp dir), never as -C into the repo/store.
        assert!(!a.iter().any(|x| x == "-C"));
    }

    #[test]
    fn lockdown_argv_never_carries_a_store_path_or_key() {
        let store_path = "C:\\Users\\x\\AppData\\Roaming\\app\\mirror.db";
        let key_hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        for argv in [claude_chat_argv(Some("m")), codex_chat_argv(Some("m"))] {
            for arg in &argv {
                assert!(!arg.contains(store_path), "argv must not contain the store path");
                assert!(!arg.contains(key_hex), "argv must not contain the store key");
                assert!(!arg.contains("mirror.db"));
            }
        }
    }

    #[test]
    fn scratch_cwd_is_created_empty_and_removed_on_drop() {
        let path;
        {
            let cwd = ScratchCwd::new(0xabcdef).unwrap();
            path = cwd.path().to_path_buf();
            assert!(path.is_dir());
            assert_eq!(std::fs::read_dir(&path).unwrap().count(), 0, "cwd starts empty");
            // The scratch dir is not the store directory.
            assert!(!path.to_string_lossy().contains("mirror.db"));
        }
        assert!(!path.exists(), "scratch cwd is removed on drop");
    }

    // ── 2.3 Claude stream-json parsing ───────────────────────────────────────
    #[test]
    fn claude_parser_extracts_deltas_and_session_id() {
        let mut p = AgentParser::new(Agent::ClaudeCode);
        let lines = [
            r#"{"type":"system","subtype":"init","session_id":"sess-123"}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Your most "}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"profitable cohort is FY2015."}}}"#,
            // The full assistant event repeats the same text; it must NOT double because deltas were seen.
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Your most profitable cohort is FY2015."}]},"session_id":"sess-123"}"#,
            r#"{"type":"result","subtype":"success","session_id":"sess-123"}"#,
        ];
        let mut emitted = String::new();
        for l in lines {
            for t in p.push_line(l) {
                emitted.push_str(&t);
            }
        }
        assert_eq!(emitted, "Your most profitable cohort is FY2015.");
        assert_eq!(p.text, "Your most profitable cohort is FY2015.");
        assert_eq!(p.session_id.as_deref(), Some("sess-123"));
    }

    #[test]
    fn claude_parser_uses_full_message_when_no_deltas() {
        let mut p = AgentParser::new(Agent::ClaudeCode);
        let out = p.push_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello there."}]}}"#,
        );
        assert_eq!(out, vec!["Hello there.".to_string()]);
        assert_eq!(p.text, "Hello there.");
    }

    // ── 2.3 Codex --json parsing ─────────────────────────────────────────────
    #[test]
    fn codex_parser_extracts_deltas_and_session_id() {
        let mut p = AgentParser::new(Agent::Codex);
        let lines = [
            r#"{"id":"0","msg":{"type":"session_configured","session_id":"cx-9"}}"#,
            r#"{"id":"1","msg":{"type":"agent_message_delta","delta":"FY2015 "}}"#,
            r#"{"id":"1","msg":{"type":"agent_message_delta","delta":"is strongest."}}"#,
            r#"{"id":"1","msg":{"type":"agent_message","message":"FY2015 is strongest."}}"#,
        ];
        let mut emitted = String::new();
        for l in lines {
            for t in p.push_line(l) {
                emitted.push_str(&t);
            }
        }
        assert_eq!(emitted, "FY2015 is strongest.");
        assert_eq!(p.session_id.as_deref(), Some("cx-9"));
    }

    #[test]
    fn codex_parser_full_message_when_no_deltas_and_alternate_shape() {
        let mut p = AgentParser::new(Agent::Codex);
        let out = p.push_line(r#"{"id":"1","msg":{"type":"agent_message","message":"Done."}}"#);
        assert_eq!(out, vec!["Done.".to_string()]);

        // Alternate item.* shape another version may emit.
        let mut q = AgentParser::new(Agent::Codex);
        let d = q.push_line(r#"{"type":"item.delta","delta":{"text":"partial "}}"#);
        assert_eq!(d, vec!["partial ".to_string()]);
        let c = q.push_line(r#"{"type":"item.completed","item":{"type":"agent_message","text":"partial done"}}"#);
        assert!(c.is_empty(), "full item ignored once deltas were seen");
        assert_eq!(q.text, "partial ");
    }

    #[test]
    fn ollama_body_puts_snapshot_as_system_then_history_then_user() {
        let b = OllamaBackend { base_url: "http://localhost:11434".into(), model: "llama3.1".into() };
        let hist = vec![ChatMessage::user("q1"), ChatMessage::assistant("a1")];
        let body = b.body("SNAP", &hist, "q2");
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "SNAP");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "q1");
        assert_eq!(msgs[2]["role"], "assistant");
        assert_eq!(msgs[3]["role"], "user");
        assert_eq!(msgs[3]["content"], "q2");
        assert_eq!(body["stream"], true);
        assert_eq!(body["model"], "llama3.1");
    }

    #[test]
    fn backend_kind_maps_to_agents() {
        assert_eq!(BackendKind::Claude.agent(), Some(Agent::ClaudeCode));
        assert_eq!(BackendKind::ChatGpt.agent(), Some(Agent::Codex));
        assert_eq!(BackendKind::Ollama.agent(), None);
        assert_eq!(serde_json::to_string(&BackendKind::ChatGpt).unwrap(), "\"chat-gpt\"");
        assert_eq!(BackendKind::ChatGpt.as_str(), "chat-gpt");
    }

    // ── 4.5 cooperative cancel halts a stream (the flag path; the task-abort path is tested
    //        against the AgentRegistry in agent.rs) ──────────────────────────────────────
    struct SpinBackend {
        max: usize,
    }
    impl ChatBackend for SpinBackend {
        async fn stream(
            &self,
            _snapshot: &str,
            _history: &[ChatMessage],
            _user_msg: &str,
            on_token: &mut (dyn FnMut(&str) + Send),
            cancel: &Cancel,
        ) -> Result<StreamOutcome> {
            let mut text = String::new();
            for _ in 0..self.max {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                on_token("x");
                text.push('x');
                tokio::task::yield_now().await;
            }
            Ok(StreamOutcome { text, session_id: None, ..Default::default() })
        }
    }

    #[tokio::test]
    async fn cancelled_stream_emits_nothing_and_returns_idle() {
        let cancel = new_cancel();
        cancel.store(true, Ordering::Relaxed);
        let b = SpinBackend { max: 10_000 };
        let mut count = 0usize;
        let mut cb = |_t: &str| count += 1;
        let out = b.stream("s", &[], "u", &mut cb, &cancel).await.unwrap();
        assert_eq!(count, 0, "a cancelled turn emits no tokens");
        assert!(out.text.is_empty());
    }

    #[tokio::test]
    async fn uncancelled_stream_runs_to_completion() {
        let cancel = new_cancel();
        let b = SpinBackend { max: 3 };
        let mut count = 0usize;
        let mut cb = |_t: &str| count += 1;
        let out = b.stream("s", &[], "u", &mut cb, &cancel).await.unwrap();
        assert_eq!(count, 3);
        assert_eq!(out.text, "xxx");
    }
}
