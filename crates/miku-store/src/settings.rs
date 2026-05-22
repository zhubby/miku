use std::fs;

use async_trait::async_trait;
use miku_api::{LlmProviderSettings, LlmSettingsStore};
use serde::{Deserialize, Serialize};

use crate::store::SqliteStore;
use crate::util::to_storage_error;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct FileSettings {
    #[serde(default)]
    llm: LlmProviderSettings,
}

#[async_trait]
impl LlmSettingsStore for SqliteStore {
    #[tracing::instrument(name = "settings.get_llm", skip(self))]
    async fn get_llm_settings(&self) -> miku_core::Result<LlmProviderSettings> {
        let path = self.paths.config_path();
        if !path.exists() {
            return Ok(LlmProviderSettings::default());
        }

        let contents = fs::read_to_string(&path).map_err(to_storage_error)?;
        let settings: FileSettings = toml::from_str(&contents).map_err(to_storage_error)?;
        Ok(settings.llm)
    }

    #[tracing::instrument(name = "settings.set_llm", skip(self, settings))]
    async fn set_llm_settings(&self, settings: LlmProviderSettings) -> miku_core::Result<()> {
        fs::create_dir_all(self.paths.root()).map_err(to_storage_error)?;
        let serialized =
            toml::to_string_pretty(&FileSettings { llm: settings }).map_err(to_storage_error)?;
        write_config_file(&self.paths.config_path(), &serialized)
    }
}

#[cfg(unix)]
fn write_config_file(path: &std::path::Path, contents: &str) -> miku_core::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(to_storage_error)?;
    file.write_all(contents.as_bytes())
        .map_err(to_storage_error)
}

#[cfg(not(unix))]
fn write_config_file(path: &std::path::Path, contents: &str) -> miku_core::Result<()> {
    fs::write(path, contents).map_err(to_storage_error)
}
