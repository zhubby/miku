use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, MikuError>;

#[derive(Debug, thiserror::Error)]
pub enum MikuError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("kubernetes error: {0}")]
    Kubernetes(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error("operation is not supported in this runtime: {0}")]
    UnsupportedRuntime(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ClusterId(String);

impl ClusterId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ClusterId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct GroupVersionKind {
    pub group: Option<String>,
    pub version: String,
    pub kind: String,
}

impl GroupVersionKind {
    pub fn core(version: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            group: None,
            version: version.into(),
            kind: kind.into(),
        }
    }

    pub fn grouped(
        group: impl Into<String>,
        version: impl Into<String>,
        kind: impl Into<String>,
    ) -> Self {
        Self {
            group: Some(group.into()),
            version: version.into(),
            kind: kind.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum ResourceScope {
    Cluster,
    Namespaced(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    pub group: Option<String>,
    pub version: String,
    pub plural: String,
    pub scope: ResourceScope,
}

impl ResourceRef {
    pub fn core(version: impl Into<String>, plural: impl Into<String>) -> Self {
        Self {
            group: None,
            version: version.into(),
            plural: plural.into(),
            scope: ResourceScope::Cluster,
        }
    }

    pub fn grouped(
        group: impl Into<String>,
        version: impl Into<String>,
        plural: impl Into<String>,
    ) -> Self {
        Self {
            group: Some(group.into()),
            version: version.into(),
            plural: plural.into(),
            scope: ResourceScope::Cluster,
        }
    }

    pub fn namespaced(mut self, namespace: impl Into<String>) -> Self {
        self.scope = ResourceScope::Namespaced(namespace.into());
        self
    }

    pub fn cluster_scoped(mut self) -> Self {
        self.scope = ResourceScope::Cluster;
        self
    }

    pub fn api_path(&self) -> String {
        let prefix = match &self.group {
            Some(group) => format!("/apis/{group}/{}", self.version),
            None => format!("/api/{}", self.version),
        };

        match &self.scope {
            ResourceScope::Cluster => format!("{prefix}/{}", self.plural),
            ResourceScope::Namespaced(namespace) => {
                format!("{prefix}/namespaces/{namespace}/{}", self.plural)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MikuPaths {
    root: PathBuf,
}

impl MikuPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn root_path(&self) -> &Path {
        &self.root
    }

    pub fn database_path(&self) -> PathBuf {
        self.root.join("miku.db")
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_ref_builds_stable_api_path() {
        let pods = ResourceRef::core("v1", "pods").namespaced("default");
        let deployments = ResourceRef::grouped("apps", "v1", "deployments").cluster_scoped();

        assert_eq!(pods.api_path(), "/api/v1/namespaces/default/pods");
        assert_eq!(deployments.api_path(), "/apis/apps/v1/deployments");
    }

    #[test]
    fn app_config_paths_are_rooted_under_miku_dir() {
        let root = std::path::PathBuf::from("/tmp/miku-test");
        let paths = MikuPaths::new(root.clone());

        assert_eq!(paths.root(), root);
        assert_eq!(
            paths.database_path(),
            std::path::PathBuf::from("/tmp/miku-test/miku.db")
        );
        assert_eq!(
            paths.config_path(),
            std::path::PathBuf::from("/tmp/miku-test/config.toml")
        );
        assert_eq!(
            paths.cache_dir(),
            std::path::PathBuf::from("/tmp/miku-test/cache")
        );
        assert_eq!(
            paths.logs_dir(),
            std::path::PathBuf::from("/tmp/miku-test/logs")
        );
    }
}
