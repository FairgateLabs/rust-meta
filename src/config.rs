use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
pub struct MetaConfig {
    pub workspace: WorkspaceConfig,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceConfig {
    pub members: Vec<String>,
}

impl MetaConfig {
    pub fn load() -> Result<Self> {
        let content = fs::read_to_string("Meta.toml").context(
            "Failed to read Meta.toml. Make sure you are in the root of the meta-workspace.",
        )?;
        let config: MetaConfig =
            toml_edit::de::from_str(&content).context("Failed to parse Meta.toml")?;
        Ok(config)
    }
}
