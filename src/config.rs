use anyhow::Result;
use serde::Deserialize;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub bind: String,
    pub entry: String,
    pub plugins: Vec<PluginConfig>,
}

#[derive(Debug, Deserialize)]
pub struct PluginConfig {
    pub tag: String,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub args: Option<serde_yaml::Value>,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let config: Config = serde_yaml::from_reader(file)?;
        Ok(config)
    }
}
