use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub hotkey: String,
    /// TTS engine: "edge" (default, neural, online) or "native" (offline fallback).
    #[serde(default = "default_engine")]
    pub engine: String,
    /// Voice name as reported by the active engine. Empty = engine default.
    #[serde(default)]
    pub voice: String,
    /// Playback speed multiplier. 1.0 = normal, 1.5 = 50% faster.
    #[serde(default = "default_speed")]
    pub speed: f32,
    pub autostart: bool,
}

fn default_engine() -> String {
    "edge".into()
}

fn default_speed() -> f32 {
    1.0
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "alt+r".into(),
            engine: default_engine(),
            voice: String::new(),
            speed: 1.0,
            autostart: true,
        }
    }
}

impl Config {
    pub fn load_or_default() -> Result<Self> {
        let path = config_path()?;
        if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let cfg: Config = serde_json::from_str(&raw)
                .with_context(|| format!("parsing {}", path.display()))?;
            Ok(cfg)
        } else {
            let cfg = Config::default();
            cfg.save()?;
            Ok(cfg)
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

pub fn app_data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("no platform data dir")?;
    Ok(base.join("TTSRead"))
}

fn config_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("config.json"))
}
