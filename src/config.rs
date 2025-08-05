use dirs::config_dir;
use eyre::{OptionExt, Result, WrapErr};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub deepgram_api_key: String,
    pub hotkey: HotkeyConfig,
    pub audio: AudioConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub modifiers: Vec<String>,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            deepgram_api_key: String::new(),
            hotkey: HotkeyConfig {
                modifiers: vec!["super".to_string()],
                key: "v".to_string(),
            },
            audio: AudioConfig {
                sample_rate: 16000,
                channels: 1,
                buffer_size: 1024,
            },
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        debug!("Loading config from {}", config_path.display());

        if !config_path.exists() {
            let config = Self::default();
            config.save()?;
            bail!(
                "Created default config at {}. Please add your Deepgram API key.",
                config_path.display()
            );
        }

        let contents = fs::read_to_string(&config_path)
            .wrap_err_with(|| format!("Failed to read config from {}", config_path.display()))?;

        let config: Config = toml::from_str(&contents).wrap_err("Failed to parse config file")?;

        if config.deepgram_api_key.is_empty() {
            bail!("Deepgram API key not set in config file");
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).wrap_err_with(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        let contents = toml::to_string_pretty(self).wrap_err("Failed to serialize config")?;

        fs::write(&config_path, contents)
            .wrap_err_with(|| format!("Failed to write config to {}", config_path.display()))?;

        Ok(())
    }

    fn config_path() -> Result<PathBuf> {
        let config_dir = config_dir().ok_or_eyre("Failed to get config directory")?;
        Ok(config_dir.join("gnome-voice-input").join("config.toml"))
    }
}
