use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::task::Profile;

/// `way`'s configuration - profiles, and pillar-specific settings like the
/// mind pillar's check-in cadence/prompts. Lives at `~/.config/way/config.toml`,
/// hand-editable, sane defaults when the file or a field is absent. redb is
/// reserved for actual app data (tasks, journal entries, routines, etc.) -
/// see issue #20's config-architecture decision.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub active_profile: Option<String>,
    /// `None` = no check-in cadence enforced (default) - matches the
    /// existing `phases`-empty-means-off convention.
    #[serde(default)]
    pub checkin_cadence_days: Option<u32>,
    #[serde(default = "default_checkin_prompts")]
    pub checkin_prompts: Vec<String>,
}

fn default_checkin_prompts() -> Vec<String> {
    vec![
        "What has your attention lately?".to_string(),
        "How's progress against your goals?".to_string(),
        "Any impact to your long-term goals?".to_string(),
    ]
}

impl Config {
    /// Ensures a fresh config always has a usable profile, so existing
    /// pillar commands keep working immediately with no manual setup step -
    /// mirrors the redb-era `bootstrap_default_profile`.
    fn ensure_bootstrapped(mut self) -> Self {
        if self.profiles.is_empty() {
            self.profiles.push(Profile::default_personal());
        }
        if self.active_profile.is_none() {
            self.active_profile = self.profiles.first().map(|p| p.name.clone());
        }
        self
    }

    pub fn find_profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn active_profile(&self) -> Result<Profile> {
        let name = self.active_profile.as_deref().ok_or_else(|| anyhow!("no active profile set"))?;
        self.find_profile(name).cloned().ok_or_else(|| anyhow!("active profile '{name}' not found"))
    }
}

thread_local! {
    /// Test-only override, thread-local rather than an env var so
    /// concurrently-running `#[test]`s (cargo test gives each its own OS
    /// thread) can't race on a process-global setting.
    static CONFIG_PATH_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub fn set_config_path_override(path: Option<PathBuf>) {
    CONFIG_PATH_OVERRIDE.with(|cell| *cell.borrow_mut() = path);
}

pub fn config_path() -> Result<PathBuf> {
    if let Some(path) = CONFIG_PATH_OVERRIDE.with(|cell| cell.borrow().clone()) {
        return Ok(path);
    }
    let base = dirs::config_dir().ok_or_else(|| anyhow!("could not resolve a config directory"))?;
    Ok(base.join("way").join("config.toml"))
}

pub fn load_config() -> Result<Config> {
    let path = config_path()?;
    let config = if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        toml::from_str(&raw)?
    } else {
        Config::default()
    };
    let config = config.ensure_bootstrapped();
    if !path.exists() {
        save_config(&config)?;
    }
    Ok(config)
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_str = toml::to_string_pretty(config)?;
    std::fs::write(&path, toml_str)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_bootstraps_personal_profile() {
        let config = Config::default().ensure_bootstrapped();
        assert_eq!(config.profiles.len(), 1);
        assert_eq!(config.profiles[0].name, "personal");
        assert_eq!(config.active_profile.as_deref(), Some("personal"));
    }

    #[test]
    fn partial_toml_fills_defaults() {
        let parsed: Config = toml::from_str("active_profile = \"personal\"\n").unwrap();
        assert!(parsed.profiles.is_empty()); // bootstrapped later by load_config, not parsing itself
        assert_eq!(parsed.checkin_cadence_days, None);
        assert_eq!(parsed.checkin_prompts, default_checkin_prompts());
    }

    #[test]
    fn round_trips_through_toml() {
        let config = Config::default().ensure_bootstrapped();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let reparsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(reparsed.active_profile, config.active_profile);
        assert_eq!(reparsed.profiles.len(), config.profiles.len());
    }
}
