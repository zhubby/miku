use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;
use std::sync::{Arc, mpsc as resource_mpsc};

use eframe::egui;
use egui_dock::{DockArea, DockState, NodePath, Style, SurfaceIndex, TabPath};
use miku_api::{ClusterSummary, MikuServices};

#[cfg(not(target_arch = "wasm32"))]
use crate::cluster_events::ClusterUiEvent;
use crate::dock::show_dock_region;
use crate::forms::NewClusterForm;
use crate::resource_panel::{
    PodLogRequest, PodResourcePanel, ResourceActionKind, ResourceActionOutcome,
    ResourceActionRequest, ResourceLoadRequest, ResourceUiEvent,
};
use crate::resources::ResourceNavItem;
use crate::state::{AppState, ClusterConnectionState, RuntimeMode};
use crate::tabs::{AppTab, AppTabViewer};

pub struct MikuApp {
    pub(crate) state: AppState,
    pub(crate) clusters: Vec<ClusterSummary>,
    pub(crate) cluster_connection_states: HashMap<miku_core::ClusterId, ClusterConnectionState>,
    pub(crate) cluster_load_in_flight: bool,
    pub(crate) cluster_load_error: Option<String>,
    pub(crate) new_cluster_form: NewClusterForm,
    pub(crate) left_dock_state: DockState<AppTab>,
    pub(crate) right_dock_state: DockState<AppTab>,
    pub(crate) workspaces: HashMap<miku_core::ClusterId, ClusterWorkspace>,
    pub(crate) next_inspector_id: usize,
    pub(crate) services: Option<Arc<dyn MikuServices>>,
    pub(crate) resource_event_sender: resource_mpsc::Sender<ResourceUiEvent>,
    pub(crate) resource_event_receiver: resource_mpsc::Receiver<ResourceUiEvent>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) runtime: Option<tokio::runtime::Handle>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) cluster_event_sender: mpsc::Sender<ClusterUiEvent>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) cluster_event_receiver: mpsc::Receiver<ClusterUiEvent>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) file_dialog: egui_file_dialog::FileDialog,
}

#[derive(Debug)]
pub(crate) struct ClusterWorkspace {
    pub(crate) dock_state: DockState<AppTab>,
    pub(crate) selected_resource: Option<ResourceNavItem>,
    pub(crate) pod_resource_panel: PodResourcePanel,
}

impl Default for ClusterWorkspace {
    fn default() -> Self {
        Self {
            dock_state: DockState::new(vec![AppTab::Workspace(1)]),
            selected_resource: None,
            pod_resource_panel: PodResourcePanel::default(),
        }
    }
}

impl MikuApp {
    pub fn new(runtime_mode: RuntimeMode) -> Self {
        tracing::debug!(?runtime_mode, "creating Miku app");
        #[cfg(not(target_arch = "wasm32"))]
        let (cluster_event_sender, cluster_event_receiver) = mpsc::channel();
        let (resource_event_sender, resource_event_receiver) = resource_mpsc::channel();
        let left_dock_state = DockState::new(vec![AppTab::Clusters, AppTab::Resources]);
        let right_dock_state = DockState::new(vec![AppTab::Inspector(1)]);

        Self {
            state: AppState::new(runtime_mode),
            clusters: Vec::new(),
            cluster_connection_states: HashMap::new(),
            cluster_load_in_flight: false,
            cluster_load_error: None,
            new_cluster_form: NewClusterForm::default(),
            left_dock_state,
            right_dock_state,
            workspaces: HashMap::new(),
            next_inspector_id: 2,
            services: None,
            resource_event_sender,
            resource_event_receiver,
            #[cfg(not(target_arch = "wasm32"))]
            runtime: None,
            #[cfg(not(target_arch = "wasm32"))]
            cluster_event_sender,
            #[cfg(not(target_arch = "wasm32"))]
            cluster_event_receiver,
            #[cfg(not(target_arch = "wasm32"))]
            file_dialog: egui_file_dialog::FileDialog::new(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native(services: Arc<dyn MikuServices>, runtime: tokio::runtime::Handle) -> Self {
        let mut app = Self::new(RuntimeMode::Native);
        app.services = Some(services);
        app.runtime = Some(runtime);
        app.request_cluster_refresh();
        app
    }

    pub fn web() -> Self {
        Self::new(RuntimeMode::Web)
    }

    pub fn web_with_services(services: Arc<dyn MikuServices>) -> Self {
        let mut app = Self::new(RuntimeMode::Web);
        app.services = Some(services);
        let cluster = ClusterSummary {
            id: miku_core::ClusterId::new("server"),
            name: "Server".to_owned(),
            context: "server".to_owned(),
            current: true,
        };
        app.state
            .select_cluster(cluster.id.clone(), cluster.name.clone());
        app.cluster_connection_states.insert(
            cluster.id.clone(),
            ClusterConnectionState::Ready {
                info: miku_api::ClusterConnectionInfo {
                    version: "server".to_owned(),
                    platform: None,
                },
            },
        );
        app.ensure_workspace(cluster.id.clone());
        app.clusters.push(cluster);
        app
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }
}

impl eframe::App for MikuApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(debug_assertions)]
        ui.ctx()
            .global_style_mut(|style| style.debug.warn_if_rect_changes_id = false);

        if ui.ctx().current_pass_index() == 0 {
            self.process_cluster_events();
            self.process_resource_events();
        }
        self.update_file_dialog(ui.ctx());

        egui::Panel::top("menu_bar").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                self.show_menu_bar(ui);
            });
        });

        egui::Panel::bottom("status_bar")
            .exact_size(24.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    egui_theme_switch::global_theme_switch(ui);
                    ui.separator();
                    ui.label(self.state.status_message());
                });
            });

        let dock_style = Style::from_egui(ui.style().as_ref());

        egui::Panel::left("left_sidebar")
            .resizable(true)
            .default_size(220.0)
            .size_range(160.0..=360.0)
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| {
                let mut tab_viewer = AppTabViewer {
                    state: &self.state,
                    clusters: &self.clusters,
                    cluster_connection_states: &self.cluster_connection_states,
                    cluster_load_in_flight: self.cluster_load_in_flight,
                    cluster_load_error: self.cluster_load_error.as_deref(),
                    closeable: false,
                    allow_windows: false,
                    add_tab: None,
                    add_requested: false,
                    new_cluster_requested: false,
                    selected_cluster: None,
                    active_resource: self.selected_workspace_resource(),
                    selected_resource: None,
                    selected_cluster_id: self.selected_cluster_id(),
                    pod_resource_panel: None,
                    resource_load_requests: Vec::new(),
                    resource_action_requests: Vec::new(),
                    pod_log_requests: Vec::new(),
                };

                show_dock_region(ui, |ui| {
                    DockArea::new(&mut self.left_dock_state)
                        .id(egui::Id::new("left_sidebar_dock"))
                        .style(dock_style.clone())
                        .draggable_tabs(false)
                        .show_close_buttons(false)
                        .show_leaf_close_all_buttons(false)
                        .show_leaf_collapse_buttons(false)
                        .show_inside(ui, &mut tab_viewer);
                });

                if tab_viewer.new_cluster_requested {
                    self.new_cluster_form.open();
                }
                let selected_cluster = tab_viewer.selected_cluster;
                let selected_resource = tab_viewer.selected_resource;

                if let Some(cluster) = selected_cluster {
                    self.select_cluster(cluster);
                }
                if let Some(resource) = selected_resource {
                    self.open_resource_tab(resource);
                }
            });

        egui::Panel::right("right_sidebar")
            .resizable(true)
            .default_size(260.0)
            .size_range(180.0..=420.0)
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| {
                let mut tab_viewer = AppTabViewer {
                    state: &self.state,
                    clusters: &self.clusters,
                    cluster_connection_states: &self.cluster_connection_states,
                    cluster_load_in_flight: self.cluster_load_in_flight,
                    cluster_load_error: self.cluster_load_error.as_deref(),
                    closeable: true,
                    allow_windows: false,
                    add_tab: Some(AppTab::Inspector(self.next_inspector_id)),
                    add_requested: false,
                    new_cluster_requested: false,
                    selected_cluster: None,
                    active_resource: self.selected_workspace_resource(),
                    selected_resource: None,
                    selected_cluster_id: self.selected_cluster_id(),
                    pod_resource_panel: None,
                    resource_load_requests: Vec::new(),
                    resource_action_requests: Vec::new(),
                    pod_log_requests: Vec::new(),
                };

                show_dock_region(ui, |ui| {
                    DockArea::new(&mut self.right_dock_state)
                        .id(egui::Id::new("right_sidebar_dock"))
                        .style(dock_style.clone())
                        .draggable_tabs(false)
                        .show_add_buttons(true)
                        .show_close_buttons(true)
                        .show_leaf_close_all_buttons(false)
                        .show_leaf_collapse_buttons(false)
                        .show_inside(ui, &mut tab_viewer);
                });

                if tab_viewer.add_requested {
                    self.right_dock_state
                        .push_to_focused_leaf(AppTab::Inspector(self.next_inspector_id));
                    self.next_inspector_id += 1;
                }
            });

        egui::CentralPanel::no_frame().show_inside(ui, |ui| {
            let Some(selected_cluster_id) = self.selected_cluster_id() else {
                show_dock_region(ui, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label("Select a cluster to open its workspace.");
                    });
                });
                return;
            };

            let active_resource = self
                .workspaces
                .get(&selected_cluster_id)
                .and_then(|workspace| workspace.selected_resource);
            let state_snapshot = self.state.clone();
            let clusters_snapshot = self.clusters.clone();
            let cluster_connection_states_snapshot = self.cluster_connection_states.clone();
            let cluster_load_in_flight = self.cluster_load_in_flight;
            let cluster_load_error = self.cluster_load_error.clone();
            let workspace = self.ensure_workspace(selected_cluster_id.clone());
            let mut tab_viewer = AppTabViewer {
                state: &state_snapshot,
                clusters: &clusters_snapshot,
                cluster_connection_states: &cluster_connection_states_snapshot,
                cluster_load_in_flight,
                cluster_load_error: cluster_load_error.as_deref(),
                closeable: true,
                allow_windows: true,
                add_tab: None,
                add_requested: false,
                new_cluster_requested: false,
                selected_cluster: None,
                active_resource,
                selected_resource: None,
                selected_cluster_id: Some(selected_cluster_id),
                pod_resource_panel: Some(&mut workspace.pod_resource_panel),
                resource_load_requests: Vec::new(),
                resource_action_requests: Vec::new(),
                pod_log_requests: Vec::new(),
            };

            show_dock_region(ui, |ui| {
                DockArea::new(&mut workspace.dock_state)
                    .id(egui::Id::new("center_workspace_dock"))
                    .style(dock_style)
                    .draggable_tabs(true)
                    .show_close_buttons(true)
                    .show_leaf_close_all_buttons(false)
                    .show_leaf_collapse_buttons(false)
                    .show_inside(ui, &mut tab_viewer);
            });

            let resource_load_requests = tab_viewer.resource_load_requests;
            let resource_action_requests = tab_viewer.resource_action_requests;
            let pod_log_requests = tab_viewer.pod_log_requests;
            if let Some(resource) = tab_viewer.selected_resource {
                workspace.selected_resource = Some(resource);
            }
            for request in resource_load_requests {
                self.request_resource_load(request);
            }
            for request in resource_action_requests {
                self.request_resource_action(request);
            }
            for request in pod_log_requests {
                self.request_pod_logs(request);
            }
        });

        self.show_new_cluster_dialog(ui.ctx());
    }
}

impl MikuApp {
    fn select_cluster(&mut self, cluster: ClusterSummary) {
        self.state
            .select_cluster(cluster.id.clone(), cluster.name.clone());
        self.ensure_workspace(cluster.id.clone());
        #[cfg(not(target_arch = "wasm32"))]
        self.request_cluster_initialization(cluster.id);
    }

    fn ensure_workspace(&mut self, cluster_id: miku_core::ClusterId) -> &mut ClusterWorkspace {
        self.workspaces.entry(cluster_id).or_default()
    }

    fn selected_workspace_resource(&self) -> Option<ResourceNavItem> {
        self.state
            .selected_cluster_id()
            .and_then(|cluster_id| self.workspaces.get(cluster_id))
            .and_then(|workspace| workspace.selected_resource)
    }

    fn open_resource_tab(&mut self, resource: ResourceNavItem) {
        let Some(cluster_id) = self.selected_cluster_id() else {
            return;
        };
        let workspace = self.ensure_workspace(cluster_id);
        workspace.selected_resource = Some(resource);
        let tab = AppTab::Resource(resource);
        if let Some((node, tab_index)) = workspace.dock_state.main_surface().find_tab(&tab) {
            let node_path = NodePath {
                surface: SurfaceIndex::main(),
                node,
            };
            let _ = workspace
                .dock_state
                .set_active_tab(TabPath::from((node_path, tab_index)));
            workspace.dock_state.set_focused_node_and_surface(node_path);
            return;
        }

        workspace.dock_state.push_to_first_leaf(tab);
    }

    fn selected_cluster_id(&self) -> Option<miku_core::ClusterId> {
        self.state.selected_cluster_id().cloned()
    }

    fn process_resource_events(&mut self) {
        while let Ok(event) = self.resource_event_receiver.try_recv() {
            self.apply_resource_event(event);
        }
    }

    fn apply_resource_event(&mut self, event: ResourceUiEvent) {
        let cluster_id = event.cluster_id().clone();
        self.ensure_workspace(cluster_id)
            .pod_resource_panel
            .apply_event(event);
    }

    fn request_resource_load(&mut self, request: ResourceLoadRequest) {
        let Some(services) = self.services.clone() else {
            self.apply_resource_event(ResourceUiEvent::ResourcesLoaded {
                request,
                result: Err("resource services are not available".to_owned()),
            });
            return;
        };
        let sender = self.resource_event_sender.clone();
        let query = request.query();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(runtime) = self.runtime.as_ref() else {
                self.apply_resource_event(ResourceUiEvent::ResourcesLoaded {
                    request,
                    result: Err("resource runtime is not available".to_owned()),
                });
                return;
            };
            runtime.spawn(async move {
                let result = services
                    .list_resources(query)
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender.send(ResourceUiEvent::ResourcesLoaded { request, result });
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let result = services
                    .list_resources(query)
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender.send(ResourceUiEvent::ResourcesLoaded { request, result });
            });
        }
    }

    fn request_resource_action(&mut self, request: ResourceActionRequest) {
        let Some(services) = self.services.clone() else {
            self.apply_resource_event(ResourceUiEvent::ResourceActionCompleted {
                request,
                result: Err("resource services are not available".to_owned()),
            });
            return;
        };
        let sender = self.resource_event_sender.clone();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(runtime) = self.runtime.as_ref() else {
                self.apply_resource_event(ResourceUiEvent::ResourceActionCompleted {
                    request,
                    result: Err("resource runtime is not available".to_owned()),
                });
                return;
            };
            runtime.spawn(async move {
                let result = run_resource_action(services.as_ref(), &request)
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender.send(ResourceUiEvent::ResourceActionCompleted { request, result });
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let result = run_resource_action(services.as_ref(), &request)
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender.send(ResourceUiEvent::ResourceActionCompleted { request, result });
            });
        }
    }

    fn request_pod_logs(&mut self, request: PodLogRequest) {
        let Some(services) = self.services.clone() else {
            self.apply_resource_event(ResourceUiEvent::PodLogsLoaded {
                request,
                result: Err("resource services are not available".to_owned()),
            });
            return;
        };
        let sender = self.resource_event_sender.clone();
        let query = request.query();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(runtime) = self.runtime.as_ref() else {
                self.apply_resource_event(ResourceUiEvent::PodLogsLoaded {
                    request,
                    result: Err("resource runtime is not available".to_owned()),
                });
                return;
            };
            runtime.spawn(async move {
                let result = services
                    .read_logs(query)
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender.send(ResourceUiEvent::PodLogsLoaded { request, result });
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let result = services
                    .read_logs(query)
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender.send(ResourceUiEvent::PodLogsLoaded { request, result });
            });
        }
    }
}

async fn run_resource_action(
    services: &dyn miku_api::MikuServices,
    request: &ResourceActionRequest,
) -> miku_core::Result<ResourceActionOutcome> {
    if let Some(apply_request) = request.apply_request() {
        return services
            .apply_resource(apply_request)
            .await
            .map(ResourceActionOutcome::Applied);
    }

    if let Some(delete_request) = request.delete_request() {
        services.delete_resource(delete_request).await?;
        return Ok(ResourceActionOutcome::Deleted);
    }

    if let Some(delete_requests) = request.batch_delete_requests() {
        for delete_request in delete_requests {
            services.delete_resource(delete_request).await?;
        }
        if let ResourceActionKind::BatchDeletePods { targets } = &request.kind {
            return Ok(ResourceActionOutcome::BatchDeleted(targets.clone()));
        }
    }

    if let Some(evict_request) = request.evict_request() {
        services.evict_pod(evict_request).await?;
        return Ok(ResourceActionOutcome::Evicted);
    }

    Err(miku_core::MikuError::UnsupportedRuntime(
        "unknown resource action".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    use crate::cluster_events::ClusterUiEvent;

    fn cluster(id: &str, name: &str) -> ClusterSummary {
        ClusterSummary {
            id: miku_core::ClusterId::new(id),
            name: name.to_owned(),
            context: id.to_owned(),
            current: false,
        }
    }

    fn pods_resource() -> ResourceNavItem {
        ResourceNavItem { name: "Pods" }
    }

    #[test]
    fn selecting_clusters_creates_separate_workspaces() {
        let mut app = MikuApp::new(RuntimeMode::Native);
        let first = cluster("first", "Cluster");
        let second = cluster("second", "Cluster");

        app.select_cluster(first.clone());
        app.open_resource_tab(pods_resource());
        app.select_cluster(second.clone());

        assert!(
            app.workspaces
                .get(&first.id)
                .unwrap()
                .dock_state
                .find_tab(&AppTab::Resource(pods_resource()))
                .is_some()
        );
        assert!(
            app.workspaces
                .get(&second.id)
                .unwrap()
                .dock_state
                .find_tab(&AppTab::Resource(pods_resource()))
                .is_none()
        );
    }

    #[test]
    fn switching_back_to_cluster_restores_its_workspace_tabs() {
        let mut app = MikuApp::new(RuntimeMode::Native);
        let first = cluster("first", "First");
        let second = cluster("second", "Second");

        app.select_cluster(first.clone());
        app.open_resource_tab(pods_resource());
        app.select_cluster(second.clone());
        app.open_resource_tab(ResourceNavItem { name: "Services" });
        app.select_cluster(first.clone());

        let first_workspace = app.workspaces.get(&first.id).unwrap();
        let second_workspace = app.workspaces.get(&second.id).unwrap();
        assert!(
            first_workspace
                .dock_state
                .find_tab(&AppTab::Resource(pods_resource()))
                .is_some()
        );
        assert!(
            first_workspace
                .dock_state
                .find_tab(&AppTab::Resource(ResourceNavItem { name: "Services" }))
                .is_none()
        );
        assert!(
            second_workspace
                .dock_state
                .find_tab(&AppTab::Resource(ResourceNavItem { name: "Services" }))
                .is_some()
        );
    }

    #[test]
    fn selected_resource_is_scoped_to_the_selected_workspace() {
        let mut app = MikuApp::new(RuntimeMode::Native);
        let first = cluster("first", "First");
        let second = cluster("second", "Second");

        app.select_cluster(first.clone());
        app.open_resource_tab(pods_resource());
        app.select_cluster(second);

        assert_eq!(app.selected_workspace_resource(), None);

        app.select_cluster(first);

        assert_eq!(app.selected_workspace_resource(), Some(pods_resource()));
    }

    #[test]
    fn selecting_cluster_marks_initialization_failed_without_services() {
        let mut app = MikuApp::new(RuntimeMode::Native);
        let first = cluster("first", "First");

        app.select_cluster(first.clone());

        assert_eq!(app.state.selected_cluster_id(), Some(&first.id));
        assert!(app.workspaces.contains_key(&first.id));
        assert!(matches!(
            app.cluster_connection_states.get(&first.id),
            Some(ClusterConnectionState::Failed { error }) if error == "cluster services are not available"
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cluster_initialized_event_updates_matching_cluster_state() {
        let mut app = MikuApp::new(RuntimeMode::Native);
        let cluster_id = miku_core::ClusterId::new("local");

        app.cluster_connection_states
            .insert(cluster_id.clone(), ClusterConnectionState::Initializing);
        app.cluster_event_sender
            .send(ClusterUiEvent::ClusterInitialized {
                cluster_id: cluster_id.clone(),
                result: Ok(miku_api::ClusterConnectionInfo {
                    version: "v1.35.0".to_owned(),
                    platform: Some("darwin/arm64".to_owned()),
                }),
            })
            .unwrap();
        app.process_cluster_events();

        assert!(matches!(
            app.cluster_connection_states.get(&cluster_id),
            Some(ClusterConnectionState::Ready { info })
                if info.version == "v1.35.0"
                    && info.platform.as_deref() == Some("darwin/arm64")
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cluster_initialized_failure_updates_matching_cluster_state() {
        let mut app = MikuApp::new(RuntimeMode::Native);
        let cluster_id = miku_core::ClusterId::new("local");

        app.cluster_event_sender
            .send(ClusterUiEvent::ClusterInitialized {
                cluster_id: cluster_id.clone(),
                result: Err("forbidden".to_owned()),
            })
            .unwrap();
        app.process_cluster_events();

        assert!(matches!(
            app.cluster_connection_states.get(&cluster_id),
            Some(ClusterConnectionState::Failed { error }) if error == "forbidden"
        ));
    }

    #[test]
    fn ready_cluster_state_prevents_duplicate_initialization() {
        let mut app = MikuApp::new(RuntimeMode::Native);
        let first = cluster("first", "First");
        let ready = ClusterConnectionState::Ready {
            info: miku_api::ClusterConnectionInfo {
                version: "v1.35.0".to_owned(),
                platform: None,
            },
        };
        app.cluster_connection_states
            .insert(first.id.clone(), ready.clone());

        app.select_cluster(first.clone());

        assert_eq!(app.cluster_connection_states.get(&first.id), Some(&ready));
    }
}
