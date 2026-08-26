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
