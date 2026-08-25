//! Windows Credential Manager access. The webview never sees any of this.

use anyhow::{Context, Result};
use keyring::v1::{Entry, Error as KeyringError};

pub const TOKENS: &str = "salesforce_tokens";
pub const DB_KEY: &str = "db_key";
const SERVICE: &str = "emanuel-customer-intelligence";

#[derive(Clone, Debug)]
pub struct Secrets {
    service: String,
}

impl Secrets {
    pub fn default_service() -> Secrets {
        Secrets::new(SERVICE)
    }
    pub fn new(service: &str) -> Secrets {
        Secrets {
            service: service.to_string(),
        }
    }

    fn entry(&self, name: &str) -> Result<Entry> {
        Entry::new(&self.service, name).context("keychain entry")
    }

    pub fn get(&self, name: &str) -> Result<Option<String>> {
        match self.entry(name)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("keychain read failed: {e}")),
        }
    }

    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        self.entry(name)?
            .set_password(value)
            .context("keychain write")
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("keychain delete failed: {e}")),
        }
    }

    /// The SQLCipher key: 32 random bytes, hex, generated once and kept in the keychain.
    pub fn db_key(&self) -> Result<String> {
        if let Some(k) = self.get(DB_KEY)? {
            return Ok(k);
        }
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).context("os random")?;
        let k = hex::encode(bytes);
        self.set(DB_KEY, &k)?;
        Ok(k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_secrets() -> Secrets {
        Secrets::new("emanuel-customer-intelligence-test")
    }

    #[test]
    fn roundtrip_and_delete() {
        let s = test_secrets();
        s.delete("rt").unwrap();
        assert_eq!(s.get("rt").unwrap(), None);
        s.set("rt", "{\"a\":1}").unwrap();
        assert_eq!(s.get("rt").unwrap().as_deref(), Some("{\"a\":1}"));
        s.delete("rt").unwrap();
        assert_eq!(s.get("rt").unwrap(), None);
    }

    #[test]
    fn db_key_is_generated_once_and_is_64_hex() {
        let s = test_secrets();
        s.delete(DB_KEY).unwrap();
        let k1 = s.db_key().unwrap();
        let k2 = s.db_key().unwrap();
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 64);
        assert!(k1.chars().all(|c| c.is_ascii_hexdigit()));
        s.delete(DB_KEY).unwrap();
    }
}
