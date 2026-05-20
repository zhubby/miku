use miku_core::ClusterId;

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum ClusterConnectionState {
    #[default]
    Idle,
    Initializing,
    Ready {
        info: miku_api::ClusterConnectionInfo,
    },
    Failed {
        error: String,
    },
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl ClusterConnectionState {
    pub(crate) fn should_initialize(&self) -> bool {
        matches!(self, Self::Idle | Self::Failed { .. })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeMode {
    Native,
    Web,
}

#[derive(Clone, Debug)]
pub struct AppState {
    runtime_mode: RuntimeMode,
    selected_cluster_id: Option<ClusterId>,
    selected_cluster_name: Option<String>,
}

impl AppState {
    pub fn new(runtime_mode: RuntimeMode) -> Self {
        Self {
            runtime_mode,
            selected_cluster_id: None,
            selected_cluster_name: None,
        }
    }

    pub fn runtime_mode(&self) -> RuntimeMode {
        self.runtime_mode
    }

    pub fn selected_cluster_id(&self) -> Option<&ClusterId> {
        self.selected_cluster_id.as_ref()
    }

    pub fn selected_cluster_name(&self) -> Option<&str> {
        self.selected_cluster_name.as_deref()
    }

    pub(crate) fn select_cluster(&mut self, id: ClusterId, name: impl Into<String>) {
        self.selected_cluster_id = Some(id);
        self.selected_cluster_name = Some(name.into());
    }

    pub fn status_message(&self) -> &str {
        match self.selected_cluster_id {
            Some(_) => "Connected",
            None => "No cluster selected",
        }
    }
}

pub fn app_title(runtime_mode: RuntimeMode) -> &'static str {
    match runtime_mode {
        RuntimeMode::Native => "Miku - Native",
        RuntimeMode::Web => "Miku - Web",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_starts_in_empty_cluster_view() {
        let state = AppState::new(RuntimeMode::Native);

        assert_eq!(state.runtime_mode(), RuntimeMode::Native);
        assert_eq!(state.selected_cluster_id(), None);
        assert_eq!(state.selected_cluster_name(), None);
        assert_eq!(state.status_message(), "No cluster selected");
    }

    #[test]
    fn app_state_tracks_selected_cluster() {
        let mut state = AppState::new(RuntimeMode::Native);

        state.select_cluster(ClusterId::new("kind-a"), "kind-miku");

        assert_eq!(state.selected_cluster_id(), Some(&ClusterId::new("kind-a")));
        assert_eq!(state.selected_cluster_name(), Some("kind-miku"));
        assert_eq!(state.status_message(), "Connected");
    }

    #[test]
    fn app_state_tracks_cluster_identity_when_names_match() {
        let mut state = AppState::new(RuntimeMode::Native);

        state.select_cluster(ClusterId::new("kind-a"), "kind-miku");
        state.select_cluster(ClusterId::new("kind-b"), "kind-miku");

        assert_eq!(state.selected_cluster_id(), Some(&ClusterId::new("kind-b")));
        assert_eq!(state.selected_cluster_name(), Some("kind-miku"));
    }

    #[test]
    fn app_title_names_runtime_mode() {
        assert_eq!(app_title(RuntimeMode::Native), "Miku - Native");
        assert_eq!(app_title(RuntimeMode::Web), "Miku - Web");
    }

    #[test]
    fn only_idle_and_failed_cluster_states_should_initialize() {
        assert!(ClusterConnectionState::Idle.should_initialize());
        assert!(
            ClusterConnectionState::Failed {
                error: "timeout".to_owned()
            }
            .should_initialize()
        );
        assert!(!ClusterConnectionState::Initializing.should_initialize());
        assert!(
            !ClusterConnectionState::Ready {
                info: miku_api::ClusterConnectionInfo {
                    version: "v1.35.0".to_owned(),
                    platform: None,
                }
            }
            .should_initialize()
        );
    }
}
