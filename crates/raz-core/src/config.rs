//! Persisted profile + token cache, the raz analogue of az's `~/.azure` directory.
//!
//! Everything lives under `~/.raz/profile.json`. The Azure CLI keeps a richer set of
//! files (azureProfile.json, msal_token_cache.json, ...); here a single file holds the
//! active subscription, tenant, and the cached access token.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{RazError, Result};

/// A cached bearer token plus its absolute expiry (Unix seconds).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedToken {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Expiry as a Unix timestamp in seconds.
    pub expires_on: i64,
}

impl CachedToken {
    /// True if the token is expired or within `skew` seconds of expiring.
    pub fn is_expired(&self, now_unix: i64, skew: i64) -> bool {
        now_unix + skew >= self.expires_on
    }
}

/// A subscription as discovered after login (subset of the ARM subscription model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
    #[serde(default)]
    pub is_default: bool,
}

/// Persisted defaults (`raz configure`), the az `defaults.*` analogue. Applied when a command
/// doesn't override them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
}

/// The whole persisted profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
    #[serde(default)]
    pub token: Option<CachedToken>,
    #[serde(default)]
    pub defaults: Defaults,
}

impl Profile {
    /// `~/.raz` directory, created on demand.
    pub fn dir() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| RazError::Io("no home directory".into()))?;
        Ok(home.join(".raz"))
    }

    /// Path to the profile file.
    pub fn path() -> Result<PathBuf> {
        Ok(Self::dir()?.join("profile.json"))
    }

    /// Load the profile, returning an empty default if none exists yet.
    pub fn load() -> Result<Profile> {
        let path = Self::path()?;
        match std::fs::read_to_string(&path) {
            Ok(s) => Ok(serde_json::from_str(&s)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Profile::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Persist the profile to disk, creating `~/.raz` if needed.
    pub fn save(&self) -> Result<()> {
        let dir = Self::dir()?;
        std::fs::create_dir_all(&dir)?;
        let path = Self::path()?;
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Remove the persisted profile (used by `raz logout`).
    pub fn clear() -> Result<()> {
        let path = Self::path()?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// The default (active) subscription, if any.
    pub fn default_subscription(&self) -> Option<&Subscription> {
        self.subscriptions
            .iter()
            .find(|s| s.is_default)
            .or_else(|| self.subscriptions.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_expiry_respects_skew() {
        let tok = CachedToken {
            access_token: "x".into(),
            refresh_token: None,
            expires_on: 1000,
        };
        assert!(!tok.is_expired(800, 100));
        assert!(tok.is_expired(901, 100));
        assert!(tok.is_expired(1000, 0));
    }

    #[test]
    fn profile_roundtrips_through_json() {
        let p = Profile {
            tenant_id: Some("tid".into()),
            subscriptions: vec![Subscription {
                id: "sub1".into(),
                name: "Dev".into(),
                tenant_id: "tid".into(),
                is_default: true,
            }],
            token: Some(CachedToken {
                access_token: "abc".into(),
                refresh_token: Some("ref".into()),
                expires_on: 42,
            }),
            defaults: Default::default(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: Profile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.subscriptions.len(), 1);
        assert_eq!(back.default_subscription().unwrap().id, "sub1");
        assert_eq!(back.token.unwrap().access_token, "abc");
    }

    #[test]
    fn default_subscription_prefers_flagged_then_first() {
        let mut p = Profile::default();
        assert!(p.default_subscription().is_none());
        p.subscriptions.push(Subscription {
            id: "a".into(),
            name: "A".into(),
            tenant_id: "t".into(),
            is_default: false,
        });
        p.subscriptions.push(Subscription {
            id: "b".into(),
            name: "B".into(),
            tenant_id: "t".into(),
            is_default: true,
        });
        assert_eq!(p.default_subscription().unwrap().id, "b");
    }
}
