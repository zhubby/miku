#[cfg(not(target_arch = "wasm32"))]
use eframe::egui;
use miku_api::{
    ClusterConnectionInfo, ClusterInitializeRequest, ClusterSummary, CreateClusterRequest,
};

use crate::app::MikuApp;
#[cfg(not(target_arch = "wasm32"))]
use crate::native::read_config_file;
use crate::state::ClusterConnectionState;

pub(crate) enum ClusterUiEvent {
    ClustersLoaded(Result<Vec<ClusterSummary>, String>),
    ClusterCreated(Result<ClusterSummary, String>),
    ClusterInitialized {
        cluster_id: miku_core::ClusterId,
        result: Result<ClusterConnectionInfo, String>,
    },
    #[cfg(not(target_arch = "wasm32"))]
    ConfigFileLoaded(Result<String, String>),
}

impl MikuApp {
    pub(crate) fn request_cluster_refresh(&mut self) {
        let Some(services) = self.services.clone() else {
            return;
        };
        #[cfg(not(target_arch = "wasm32"))]
        let Some(runtime) = self.runtime.as_ref() else {
            return;
        };

        self.cluster_load_in_flight = true;
        self.cluster_load_error = None;
        let sender = self.cluster_event_sender.clone();

        #[cfg(not(target_arch = "wasm32"))]
        runtime.spawn(async move {
            let result = services
                .list_clusters()
                .await
                .map_err(|error| error.to_string());
            let _ = sender.send(ClusterUiEvent::ClustersLoaded(result));
        });

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let result = services
                .list_clusters()
                .await
                .map_err(|error| error.to_string());
            let _ = sender.send(ClusterUiEvent::ClustersLoaded(result));
        });
    }

    pub(crate) fn submit_new_cluster(&mut self) {
        let Some(services) = self.services.clone() else {
            self.new_cluster_form
                .save_failed("cluster storage is not available");
            return;
        };

        let context = self.new_cluster_form.context.trim().to_owned();
        let config = self.new_cluster_form.config.clone();
        if context.is_empty() {
            self.new_cluster_form.save_failed("context is required");
            return;
        }
        if config.trim().is_empty() {
            self.new_cluster_form.save_failed("config is required");
            return;
        }

        #[cfg(not(target_arch = "wasm32"))]
        let Some(runtime) = self.runtime.as_ref() else {
            self.new_cluster_form
                .save_failed("cluster storage runtime is not available");
            return;
        };

        self.new_cluster_form.save_started();
        self.cluster_load_in_flight = true;
        let sender = self.cluster_event_sender.clone();

        #[cfg(not(target_arch = "wasm32"))]
        runtime.spawn(async move {
            let result = services
                .create_cluster(CreateClusterRequest { context, config })
                .await
                .map_err(|error| error.to_string());
            let _ = sender.send(ClusterUiEvent::ClusterCreated(result));
        });

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let result = services
                .create_cluster(CreateClusterRequest { context, config })
                .await
                .map_err(|error| error.to_string());
            let _ = sender.send(ClusterUiEvent::ClusterCreated(result));
        });
    }

    pub(crate) fn request_cluster_initialization(&mut self, cluster_id: miku_core::ClusterId) {
        let current_state = self
            .cluster_connection_states
            .entry(cluster_id.clone())
            .or_default();
        if !current_state.should_initialize() {
            return;
        }
        *current_state = ClusterConnectionState::Initializing;

        let Some(services) = self.services.clone() else {
            self.cluster_connection_states.insert(
                cluster_id,
                ClusterConnectionState::Failed {
                    error: "cluster services are not available".to_owned(),
                },
            );
            return;
        };

        let sender = self.cluster_event_sender.clone();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(runtime) = self.runtime.as_ref() else {
                self.cluster_connection_states.insert(
                    cluster_id,
                    ClusterConnectionState::Failed {
                        error: "cluster runtime is not available".to_owned(),
                    },
                );
                return;
            };
            runtime.spawn(async move {
                let result = services
                    .initialize_cluster(ClusterInitializeRequest {
                        cluster_id: cluster_id.clone(),
                    })
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender.send(ClusterUiEvent::ClusterInitialized { cluster_id, result });
            });
        }

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let result = services
                .initialize_cluster(ClusterInitializeRequest {
                    cluster_id: cluster_id.clone(),
                })
                .await
                .map_err(|error| error.to_string());
            let _ = sender.send(ClusterUiEvent::ClusterInitialized { cluster_id, result });
        });
    }

    pub(crate) fn process_cluster_events(&mut self) {
        while let Ok(event) = self.cluster_event_receiver.try_recv() {
            match event {
                ClusterUiEvent::ClustersLoaded(result) => {
                    self.cluster_load_in_flight = false;
                    match result {
                        Ok(clusters) => {
                            self.clusters = clusters;
                            self.cluster_load_error = None;
                        }
                        Err(error) => self.cluster_load_error = Some(error),
                    }
                }
                ClusterUiEvent::ClusterCreated(result) => match result {
                    Ok(cluster) => {
                        self.new_cluster_form.save_succeeded();
                        self.clusters.push(cluster);
                        self.request_cluster_refresh();
                    }
                    Err(error) => {
                        self.cluster_load_in_flight = false;
                        self.new_cluster_form.save_failed(error);
                    }
                },
                ClusterUiEvent::ClusterInitialized { cluster_id, result } => {
                    let state = match result {
                        Ok(info) => ClusterConnectionState::Ready { info },
                        Err(error) => ClusterConnectionState::Failed { error },
                    };
                    self.cluster_connection_states.insert(cluster_id, state);
                }
                #[cfg(not(target_arch = "wasm32"))]
                ClusterUiEvent::ConfigFileLoaded(result) => match result {
                    Ok(config) => {
                        self.new_cluster_form.config = config;
                        self.new_cluster_form.error = None;
                    }
                    Err(error) => self.new_cluster_form.save_failed(error),
                },
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn update_file_dialog(&mut self, ctx: &egui::Context) {
        self.file_dialog.update(ctx);
        if let Some(path) = self.file_dialog.take_picked() {
            let sender = self.cluster_event_sender.clone();
            let Some(runtime) = self.runtime.as_ref() else {
                self.new_cluster_form
                    .save_failed("cluster storage runtime is not available");
                return;
            };
            runtime.spawn_blocking(move || {
                let result = read_config_file(&path);
                let _ = sender.send(ClusterUiEvent::ConfigFileLoaded(result));
            });
        }
    }
}
