//! Runtime configuration. The consumer key is a public client id; it is still
//! kept out of git via .env. Loaded from the process env, falling back to a
//! .env file in the cwd or its parent (tauri dev runs from src-tauri/).

use anyhow::{anyhow, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub client_id: String,
    pub login_url: String,
}

impl Config {
    pub fn from_env() -> Result<Config> {
        let _ = dotenvy::from_filename(".env").or_else(|_| dotenvy::from_filename("../.env"));
        let client_id = std::env::var("SF_CLIENT_ID")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow!("SF_CLIENT_ID is not set (see .env.example)"))?;
        let login_url = std::env::var("SF_LOGIN_URL")
            .unwrap_or_else(|_| "https://login.salesforce.com".to_string());
        Ok(Config::new(client_id, login_url))
    }

    pub fn new(client_id: impl Into<String>, login_url: impl Into<String>) -> Config {
        let login_url = login_url.into().trim().trim_end_matches('/').to_string();
        Config {
            client_id: client_id.into(),
            login_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_strips_trailing_slash_and_whitespace() {
        let c = Config::new("id", " https://x.my.salesforce.com/ ");
        assert_eq!(c.login_url, "https://x.my.salesforce.com");
        assert_eq!(c.client_id, "id");
    }
}
