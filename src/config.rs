use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SoundSetting {
    System(String),
    File(PathBuf),
    TerminalBell,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub font: String,
    pub notification: bool,
    pub sound: SoundSetting,
    pub restart_key: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font: "standard".into(),
            notification: true,
            sound: SoundSetting::System("Glass".into()),
            restart_key: "r".into(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub font: Option<String>,
    pub notification: Option<bool>,
    pub sound: Option<SoundSetting>,
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let directories = ProjectDirs::from("", "", "clck")
            .context("could not determine the platform configuration directory")?;
        Ok(directories.config_dir().join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        Self::load_from(&Self::path()?)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path)
            .with_context(|| format!("could not read configuration {}", path.display()))?;
        toml::from_str(&contents)
            .with_context(|| format!("could not parse configuration {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        let contents = toml::to_string_pretty(self).context("could not serialize configuration")?;
        fs::write(path, contents)
            .with_context(|| format!("could not write configuration {}", path.display()))
    }

    pub fn resolve(&self, overrides: Overrides) -> Self {
        Self {
            font: overrides.font.unwrap_or_else(|| self.font.clone()),
            notification: overrides.notification.unwrap_or(self.notification),
            sound: overrides.sound.unwrap_or_else(|| self.sound.clone()),
            restart_key: self.restart_key.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, Overrides, SoundSetting};

    #[test]
    fn command_line_overrides_saved_configuration() {
        let saved = Config {
            font: "box".into(),
            notification: false,
            sound: SoundSetting::System("Glass".into()),
            restart_key: "r".into(),
        };
        let resolved = saved.resolve(Overrides {
            font: Some("banner".into()),
            notification: Some(true),
            sound: None,
        });
        assert_eq!(resolved.font, "banner");
        assert!(resolved.notification);
        assert_eq!(resolved.sound, SoundSetting::System("Glass".into()));
    }

    #[test]
    fn missing_configuration_uses_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let config = Config::load_from(&directory.path().join("missing.toml")).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn configuration_round_trips_as_toml() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let config = Config::default();
        config.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path).unwrap(), config);
    }
}
