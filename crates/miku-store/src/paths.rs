use std::path::{Path, PathBuf};

use directories::BaseDirs;
use miku_core::{MikuError, MikuPaths};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorePaths {
    inner: MikuPaths,
}

impl StorePaths {
    pub fn default_for_user() -> miku_core::Result<Self> {
        let base_dirs = BaseDirs::new()
            .ok_or_else(|| MikuError::Config("could not resolve the user home directory".into()))?;
        Ok(Self::from_root(base_dirs.home_dir().join(".miku")))
    }

    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        Self {
            inner: MikuPaths::new(root),
        }
    }

    pub fn root(&self) -> &Path {
        self.inner.root_path()
    }

    pub fn database_path(&self) -> PathBuf {
        self.inner.database_path()
    }

    pub fn config_path(&self) -> PathBuf {
        self.inner.config_path()
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.inner.cache_dir()
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.inner.logs_dir()
    }
}
