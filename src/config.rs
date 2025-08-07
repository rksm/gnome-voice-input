use dirs::config_dir;
use eyre::{bail, OptionExt, Result, WrapErr};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub deepgram_api_key: String,
    pub hotkey: HotkeyConfig,
    pub audio: AudioConfig,
    #[serde(default)]
    pub transcription: TranscriptionConfig,
    #[serde(default)]
    pub ui: UiConfig,
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
    #[serde(default = "default_audio_chunk_ms")]
    pub audio_chunk_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    #[serde(default = "default_use_interim_results")]
    pub use_interim_results: bool,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_smart_format")]
    pub smart_format: bool,
    #[serde(default = "default_punctuate")]
    pub punctuate: bool,
}

fn default_audio_chunk_ms() -> u32 {
    25 // 25ms chunks
}

fn default_use_interim_results() -> bool {
    true
}

fn default_model() -> String {
    "nova-3".to_string()
}

fn default_language() -> String {
    "en".to_string()
}

fn default_smart_format() -> bool {
    true
}

fn default_punctuate() -> bool {
    true
}

fn default_show_tray_icon() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_tray_icon: true,
        }
    }
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            use_interim_results: true,
            model: default_model(),
            language: default_language(),
            smart_format: default_smart_format(),
            punctuate: default_punctuate(),
        }
    }
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
                audio_chunk_ms: 25,
            },
            transcription: TranscriptionConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

impl Config {
    pub fn get_config_path(custom_path: Option<PathBuf>) -> Result<PathBuf> {
        match custom_path {
            Some(path) => {
                if !path.exists() {
                    bail!(
                        "Config file not found at specified path: {}",
                        path.display()
                    );
                }
                // Convert to absolute path for consistent handling
                Ok(path.canonicalize()?)
            }
            None => Self::config_path(),
        }
    }

    pub fn load(custom_path: Option<PathBuf>) -> Result<Self> {
        let config_path = match custom_path {
            Some(path) => {
                // Use the provided custom config path
                if !path.exists() {
                    bail!(
                        "Config file not found at specified path: {}",
                        path.display()
                    );
                }
                // Convert to absolute path for consistent handling
                path.canonicalize()?
            }
            None => {
                // Use the default config path
                let default_path = Self::config_path()?;
                if !default_path.exists() {
                    let config = Self::default();
                    config.save()?;
                    bail!(
                        "Created default config at {}. Please add your Deepgram API key.",
                        default_path.display()
                    );
                }
                default_path
            }
        };

        info!("Loading config from {}", config_path.display());

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
