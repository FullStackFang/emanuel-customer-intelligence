//! Local CLI agents (Claude Code, Codex): config, persistence, arg building,
//! binary resolution. Auth is the CLI's own login — no API key is handled here.

use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    ClaudeCode,
    Codex,
}

impl Agent {
    pub fn all() -> [Agent; 2] {
        [Agent::ClaudeCode, Agent::Codex]
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude-code",
            Agent::Codex => "codex",
        }
    }
    pub fn bin(&self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude",
            Agent::Codex => "codex",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentConfig {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub sandbox_mode: Option<String>,
    pub timeout_minutes: u64,
    #[serde(default)]
    pub isolate: bool,
    #[serde(default)]
    pub extra_args: Vec<String>,
}

impl AgentConfig {
    pub fn default_for(a: Agent) -> AgentConfig {
        match a {
            Agent::ClaudeCode => AgentConfig {
                model: None,
                working_dir: None,
                permission_mode: Some("plan".into()),
                sandbox_mode: None,
                timeout_minutes: 10,
                isolate: false,
                extra_args: vec![],
            },
            Agent::Codex => AgentConfig {
                model: None,
                working_dir: None,
                permission_mode: None,
                sandbox_mode: Some("read-only".into()),
                timeout_minutes: 10,
                isolate: false,
                extra_args: vec![],
            },
        }
    }
}

fn dflt_claude() -> AgentConfig {
    AgentConfig::default_for(Agent::ClaudeCode)
}
fn dflt_codex() -> AgentConfig {
    AgentConfig::default_for(Agent::Codex)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentSettings {
    #[serde(default)]
    pub active_agent: Option<Agent>,
    #[serde(default = "dflt_claude")]
    pub claude_code: AgentConfig,
    #[serde(default = "dflt_codex")]
    pub codex: AgentConfig,
}

impl Default for AgentSettings {
    fn default() -> Self {
        AgentSettings {
            active_agent: None,
            claude_code: dflt_claude(),
            codex: dflt_codex(),
        }
    }
}

pub const META_KEY: &str = "agent_settings";

impl AgentSettings {
    pub fn config(&self, a: Agent) -> &AgentConfig {
        match a {
            Agent::ClaudeCode => &self.claude_code,
            Agent::Codex => &self.codex,
        }
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
            if let Some(m) = &c.model {
                v.push("--model".into());
                v.push(m.clone());
            }
            if let Some(p) = &c.permission_mode {
                v.push("--permission-mode".into());
                v.push(p.clone());
            }
            if let Some(w) = &c.working_dir {
                v.push("--add-dir".into());
                v.push(w.clone());
            }
            if c.isolate {
                v.push("--strict-mcp-config".into());
                v.push("--mcp-config".into());
                v.push("{\"mcpServers\":{}}".into());
            }
        }
        Agent::Codex => {
            v.extend(["exec", "--json", "--skip-git-repo-check"].map(String::from));
            if let Some(m) = &c.model {
                v.push("-m".into());
                v.push(m.clone());
            }
            if let Some(s) = &c.sandbox_mode {
                v.push("-s".into());
                v.push(s.clone());
            }
            if let Some(w) = &c.working_dir {
                v.push("-C".into());
                v.push(w.clone());
            }
        }
    }
    v.extend(c.extra_args.iter().cloned());
    v
}

/// Candidate file names for an agent's binary, most-specific first.
fn bin_candidates(agent: Agent) -> Vec<String> {
    let base = agent.bin();
    if cfg!(windows) {
        vec![
            format!("{base}.cmd"),
            format!("{base}.exe"),
            base.to_string(),
        ]
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
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        // Write concurrently with the stdout/stderr pump below (and thus inside the
        // overall timeout): a prompt larger than the OS pipe buffer would otherwise
        // deadlock here, blocked on a stdin write while nothing drains stdout yet.
        let prompt_owned = prompt.to_string();
        tokio::spawn(async move {
            let _ = stdin.write_all(prompt_owned.as_bytes()).await;
            let _ = stdin.shutdown().await; // EOF so the CLI proceeds
        });
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
        Ok::<RunOutcome, anyhow::Error>(RunOutcome {
            code: status.code(),
            timed_out: false,
        })
    };

    match tokio::time::timeout(timeout, pump).await {
        Ok(res) => res,
        Err(_) => {
            // pump was dropped, which drops `child` (kill_on_drop(true) above
            // ensures the OS process is killed when that happens).
            Ok(RunOutcome {
                code: None,
                timed_out: true,
            })
        }
    }
}

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
        return AgentStatus {
            agent,
            installed: false,
            version: None,
            path: None,
            error: Some(format!("`{}` not found on PATH", agent.bin())),
        };
    }
    // Run `<bin> --version` verbatim via run_streaming with a short timeout.
    let (program, args) = spawn_target(resolved.as_ref().unwrap(), &["--version".to_string()]);
    let mut lines: Vec<String> = Vec::new();
    let outcome = run_streaming(
        &program,
        &args,
        None,
        "",
        std::time::Duration::from_secs(3),
        |l| lines.push(l),
        |_e| {},
    )
    .await;
    match outcome {
        Ok(o) if !o.timed_out && o.code == Some(0) => AgentStatus {
            agent,
            installed: true,
            version: Some(lines.join(" ").trim().to_string()),
            path,
            error: None,
        },
        Ok(o) if o.timed_out => AgentStatus {
            agent,
            installed: false,
            version: None,
            path,
            error: Some("`--version` timed out after 3s".into()),
        },
        Ok(o) => AgentStatus {
            agent,
            installed: false,
            version: None,
            path,
            error: Some(format!("`--version` exited with {:?}", o.code)),
        },
        Err(e) => AgentStatus {
            agent,
            installed: false,
            version: None,
            path,
            error: Some(e.to_string()),
        },
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

/// Registry of in-flight agent runs, keyed by stream id, storing each run's
/// task abort handle so a run can be cancelled from a Tauri command.
#[derive(Default)]
pub struct AgentRegistry(pub Mutex<HashMap<String, tokio::task::JoinHandle<()>>>);

impl AgentRegistry {
    pub fn insert(&self, id: String, handle: tokio::task::JoinHandle<()>) {
        if let Ok(mut g) = self.0.lock() {
            g.insert(id, handle);
        }
    }
    pub fn remove(&self, id: &str) -> Option<tokio::task::JoinHandle<()>> {
        self.0.lock().ok().and_then(|mut g| g.remove(id))
    }
    pub async fn kill(&self, id: &str) -> bool {
        if let Some(handle) = self.remove(id) {
            handle.abort();
            true
        } else {
            false
        }
    }
}

/// A stream id derived from a caller-supplied seed (no time/random in library code).
pub fn new_stream_id(seed: u64) -> String {
    format!("agent-{seed:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store;

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    #[test]
    fn agent_wire_strings_and_bins() {
        assert_eq!(
            serde_json::to_string(&Agent::ClaudeCode).unwrap(),
            "\"claude-code\""
        );
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
        assert!(a.starts_with(&[
            "-p".to_string(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into()
        ]));
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
        assert!(a.starts_with(&[
            "exec".to_string(),
            "--json".into(),
            "--skip-git-repo-check".into()
        ]));
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
        let name = if cfg!(windows) {
            "claude.cmd"
        } else {
            "claude"
        };
        std::fs::write(root.join(name), b"x").unwrap();
        let found = resolve_bin_in(&[root.to_path_buf()], Agent::ClaudeCode).unwrap();
        assert_eq!(found.file_name().unwrap().to_string_lossy(), name);
        // absent agent yields None
        assert!(resolve_bin_in(&[root.to_path_buf()], Agent::Codex).is_none());
    }

    #[tokio::test]
    async fn run_streaming_captures_output_and_exit() {
        // A portable command that prints two lines and exits 0.
        let (program, args): (std::path::PathBuf, Vec<String>) = if cfg!(windows) {
            (
                std::path::PathBuf::from("cmd"),
                vec!["/C".into(), "echo one& echo two".into()],
            )
        } else {
            (
                std::path::PathBuf::from("/bin/sh"),
                vec!["-c".into(), "echo one; echo two".into()],
            )
        };
        let mut lines: Vec<String> = Vec::new();
        let outcome = run_streaming(
            &program,
            &args,
            None,
            "",
            std::time::Duration::from_secs(30),
            |l| lines.push(l),
            |_e| {},
        )
        .await
        .unwrap();
        assert!(!outcome.timed_out);
        assert_eq!(outcome.code, Some(0));
        assert!(lines.iter().any(|l| l.trim() == "one"));
        assert!(lines.iter().any(|l| l.trim() == "two"));
    }

    #[tokio::test]
    async fn registry_kill_aborts_an_in_flight_run() {
        // The mechanism behind `chat_cancel`: killing a registered run aborts its task so it
        // never completes (which, for a real CLI run, kills the subprocess via kill_on_drop).
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let reg = AgentRegistry::default();
        let completed = Arc::new(AtomicBool::new(false));
        let flag = completed.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            flag.store(true, Ordering::Relaxed);
        });
        reg.insert("run-1".to_string(), handle);
        assert!(reg.kill("run-1").await, "kill finds and aborts the run");
        assert!(!reg.kill("run-1").await, "the run is removed after being killed");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            !completed.load(Ordering::Relaxed),
            "the aborted task never ran to completion"
        );
    }

    #[tokio::test]
    async fn run_streaming_times_out_and_kills() {
        let (program, args): (std::path::PathBuf, Vec<String>) = if cfg!(windows) {
            // ping sleeps ~2s; timeout is 100ms so it must be killed.
            (
                std::path::PathBuf::from("cmd"),
                vec!["/C".into(), "ping -n 3 127.0.0.1 >NUL".into()],
            )
        } else {
            (
                std::path::PathBuf::from("/bin/sh"),
                vec!["-c".into(), "sleep 2".into()],
            )
        };
        let outcome = run_streaming(
            &program,
            &args,
            None,
            "",
            std::time::Duration::from_millis(100),
            |_l| {},
            |_e| {},
        )
        .await
        .unwrap();
        assert!(outcome.timed_out);
    }
}
