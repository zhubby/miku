use std::collections::HashMap;
use std::sync::{Arc, mpsc as resource_mpsc};

use eframe::egui;
use egui_dock::{DockArea, DockState, NodePath, Style, SurfaceIndex, TabPath};
use futures::StreamExt;
use miku_api::{ClusterSummary, MikuServices, PodAttachInput};

use crate::cluster_events::ClusterUiEvent;
use crate::dock::show_dock_region;
use crate::forms::NewClusterForm;
#[cfg(not(target_arch = "wasm32"))]
use crate::resource_panel::ResourceWatchKey;
use crate::resource_panel::{
    PodAttachInputRequest, PodAttachRequest, PodLogRequest, PodResourcePanel, ResourceActionKind,
    ResourceActionOutcome, ResourceActionRequest, ResourceLoadRequest, ResourceUiEvent,
    ResourceWatchRequest,
};
use crate::resources::ResourceNavItem;
use crate::state::{AppState, ClusterConnectionState, RuntimeMode};
use crate::tabs::{AppTab, AppTabViewer, ClusterStatusLoadRequest, ClusterStatusPanel};

const MAX_RESOURCE_EVENTS_PER_PASS: usize = 8;

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
    pub(crate) pod_attach_inputs:
        HashMap<u64, futures::channel::mpsc::UnboundedSender<PodAttachInput>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) resource_watch_tasks: HashMap<ResourceWatchKey, tokio::task::JoinHandle<()>>,
    pub(crate) status_event_sender: resource_mpsc::Sender<ClusterStatusUiEvent>,
    pub(crate) status_event_receiver: resource_mpsc::Receiver<ClusterStatusUiEvent>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) runtime: Option<tokio::runtime::Handle>,
    pub(crate) cluster_event_sender: resource_mpsc::Sender<ClusterUiEvent>,
    pub(crate) cluster_event_receiver: resource_mpsc::Receiver<ClusterUiEvent>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) file_dialog: egui_file_dialog::FileDialog,
}

#[derive(Debug)]
pub(crate) struct ClusterWorkspace {
    pub(crate) dock_state: DockState<AppTab>,
    pub(crate) selected_resource: Option<ResourceNavItem>,
    pub(crate) status_panel: ClusterStatusPanel,
    pub(crate) pod_resource_panel: PodResourcePanel,
}

impl Default for ClusterWorkspace {
    fn default() -> Self {
        Self {
            dock_state: DockState::new(vec![AppTab::Workspace(1)]),
            selected_resource: None,
            status_panel: ClusterStatusPanel::default(),
            pod_resource_panel: PodResourcePanel::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ClusterStatusUiEvent {
    Loaded {
        request: ClusterStatusLoadRequest,
        result: Result<miku_api::ClusterStatusReport, String>,
    },
}

impl MikuApp {
    pub fn new(runtime_mode: RuntimeMode) -> Self {
        tracing::debug!(?runtime_mode, "creating Miku app");
        let (cluster_event_sender, cluster_event_receiver) = resource_mpsc::channel();
        let (resource_event_sender, resource_event_receiver) = resource_mpsc::channel();
        let (status_event_sender, status_event_receiver) = resource_mpsc::channel();
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
            pod_attach_inputs: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            resource_watch_tasks: HashMap::new(),
            status_event_sender,
            status_event_receiver,
            #[cfg(not(target_arch = "wasm32"))]
            runtime: None,
            cluster_event_sender,
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
        app.request_cluster_refresh();
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
            self.process_resource_events(ui.ctx());
            self.process_status_events();
        }
        self.update_file_dialog(ui.ctx());

        #[cfg(not(target_arch = "wasm32"))]
        {
            egui::Panel::top("menu_bar").show_inside(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    self.show_menu_bar(ui);
                });
            });
        }

        egui::Panel::bottom("status_bar")
            .exact_size(24.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    egui_theme_switch::global_theme_switch(ui);
                    ui.separator();
                    self.show_status_bar_connection(ui);
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
                    cluster_status_panel: None,
                    pod_resource_panel: None,
                    status_load_requests: Vec::new(),
                    resource_load_requests: Vec::new(),
                    resource_watch_requests: Vec::new(),
                    resource_action_requests: Vec::new(),
                    pod_log_requests: Vec::new(),
                    pod_attach_requests: Vec::new(),
                    pod_attach_input_requests: Vec::new(),
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
                    cluster_status_panel: None,
                    pod_resource_panel: None,
                    status_load_requests: Vec::new(),
                    resource_load_requests: Vec::new(),
                    resource_watch_requests: Vec::new(),
                    resource_action_requests: Vec::new(),
                    pod_log_requests: Vec::new(),
                    pod_attach_requests: Vec::new(),
                    pod_attach_input_requests: Vec::new(),
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
                cluster_status_panel: Some(&mut workspace.status_panel),
                pod_resource_panel: Some(&mut workspace.pod_resource_panel),
                status_load_requests: Vec::new(),
                resource_load_requests: Vec::new(),
                resource_watch_requests: Vec::new(),
                resource_action_requests: Vec::new(),
                pod_log_requests: Vec::new(),
                pod_attach_requests: Vec::new(),
                pod_attach_input_requests: Vec::new(),
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
            let resource_watch_requests = tab_viewer.resource_watch_requests;
            let resource_action_requests = tab_viewer.resource_action_requests;
            let pod_log_requests = tab_viewer.pod_log_requests;
            let pod_attach_requests = tab_viewer.pod_attach_requests;
            let pod_attach_input_requests = tab_viewer.pod_attach_input_requests;
            let status_load_requests = tab_viewer.status_load_requests;
            if let Some(resource) = tab_viewer.selected_resource {
                workspace.selected_resource = Some(resource);
            }
            for request in status_load_requests {
                self.request_cluster_status(request);
            }
            for request in resource_load_requests {
                self.request_resource_load(request);
            }
            for request in resource_watch_requests {
                self.request_resource_watch(request, ui.ctx().clone());
            }
            for request in resource_action_requests {
                self.request_resource_action(request);
            }
            for request in pod_log_requests {
                self.request_pod_logs(request);
            }
            for request in pod_attach_requests {
                self.request_pod_attach(request);
            }
            for request in pod_attach_input_requests {
                self.send_pod_attach_input(request);
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

    fn show_status_bar_connection(&self, ui: &mut egui::Ui) {
        let summary = status_bar_connection_summary(
            self.state.selected_cluster_name(),
            self.state
                .selected_cluster_id()
                .and_then(|cluster_id| self.cluster_connection_states.get(cluster_id)),
        );
        let color = match summary.tone {
            StatusBarConnectionTone::Default => ui.visuals().text_color(),
            StatusBarConnectionTone::Weak => ui.visuals().weak_text_color(),
            StatusBarConnectionTone::Accent => ui.visuals().hyperlink_color,
            StatusBarConnectionTone::Error => ui.visuals().error_fg_color,
        };
        let response = ui.label(egui::RichText::new(summary.text).color(color));
        if let Some(hover) = summary.hover {
            response.on_hover_text(hover);
        }
    }

    fn process_resource_events(&mut self, ctx: &egui::Context) {
        for _ in 0..MAX_RESOURCE_EVENTS_PER_PASS {
            match self.resource_event_receiver.try_recv() {
                Ok(event) => self.apply_resource_event(event),
                Err(resource_mpsc::TryRecvError::Empty) => return,
                Err(resource_mpsc::TryRecvError::Disconnected) => return,
            }
        }

        ctx.request_repaint();
    }

    fn process_status_events(&mut self) {
        while let Ok(event) = self.status_event_receiver.try_recv() {
            self.apply_status_event(event);
        }
    }

    fn apply_status_event(&mut self, event: ClusterStatusUiEvent) {
        match event {
            ClusterStatusUiEvent::Loaded { request, result } => {
                self.ensure_workspace(request.cluster_id.clone())
                    .status_panel
                    .apply_result(&request, result);
            }
        }
    }

    fn request_cluster_status(&mut self, request: ClusterStatusLoadRequest) {
        let Some(services) = self.services.clone() else {
            self.apply_status_event(ClusterStatusUiEvent::Loaded {
                request,
                result: Err("cluster status services are not available".to_owned()),
            });
            return;
        };
        let sender = self.status_event_sender.clone();
        let api_request = miku_api::ClusterStatusRequest {
            cluster_id: request.cluster_id.clone(),
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(runtime) = self.runtime.as_ref() else {
                self.apply_status_event(ClusterStatusUiEvent::Loaded {
                    request,
                    result: Err("cluster status runtime is not available".to_owned()),
                });
                return;
            };
            runtime.spawn(async move {
                let result = services
                    .get_cluster_status(api_request)
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender.send(ClusterStatusUiEvent::Loaded { request, result });
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let result = services
                    .get_cluster_status(api_request)
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender.send(ClusterStatusUiEvent::Loaded { request, result });
            });
        }
    }

    fn apply_resource_event(&mut self, event: ResourceUiEvent) {
        match &event {
            ResourceUiEvent::PodAttachConnected {
                request,
                result: Ok(input),
            } => {
                self.pod_attach_inputs
                    .insert(request.request_id, input.clone());
            }
            ResourceUiEvent::PodAttachOutput {
                request,
                result: Ok(miku_api::PodAttachOutput::Closed),
            } => {
                self.pod_attach_inputs.remove(&request.request_id);
            }
            _ => {}
        }
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

    fn request_resource_watch(&mut self, request: ResourceWatchRequest, repaint: egui::Context) {
        let Some(services) = self.services.clone() else {
            self.apply_resource_event(ResourceUiEvent::ResourceWatchUpdated {
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
                self.apply_resource_event(ResourceUiEvent::ResourceWatchUpdated {
                    request,
                    result: Err("resource runtime is not available".to_owned()),
                });
                return;
            };
            if let Some(task) = self.resource_watch_tasks.remove(&request.key()) {
                task.abort();
            }
            let task_request = request.clone();
            let task = runtime.spawn(async move {
                let stream = services.watch_resources(query).await;
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(error) => {
                        let _ = sender.send(ResourceUiEvent::ResourceWatchUpdated {
                            request: task_request,
                            result: Err(error.to_string()),
                        });
                        repaint.request_repaint();
                        return;
                    }
                };

                while let Some(result) = stream.next().await {
                    let _ = sender.send(ResourceUiEvent::ResourceWatchUpdated {
                        request: task_request.clone(),
                        result: result.map_err(|error| error.to_string()),
                    });
                    repaint.request_repaint();
                }
            });
            self.resource_watch_tasks.insert(request.key(), task);
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let stream = services.watch_resources(query).await;
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(error) => {
                        let _ = sender.send(ResourceUiEvent::ResourceWatchUpdated {
                            request,
                            result: Err(error.to_string()),
                        });
                        repaint.request_repaint();
                        return;
                    }
                };

                while let Some(result) = stream.next().await {
                    let _ = sender.send(ResourceUiEvent::ResourceWatchUpdated {
                        request: request.clone(),
                        result: result.map_err(|error| error.to_string()),
                    });
                    repaint.request_repaint();
                }
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

    fn request_pod_attach(&mut self, request: PodAttachRequest) {
        let Some(services) = self.services.clone() else {
            self.apply_resource_event(ResourceUiEvent::PodAttachConnected {
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
                self.apply_resource_event(ResourceUiEvent::PodAttachConnected {
                    request,
                    result: Err("resource runtime is not available".to_owned()),
                });
                return;
            };
            runtime.spawn(async move {
                match services.attach_pod(query).await {
                    Ok(mut session) => {
                        let input = session.input.clone();
                        let _ = sender.send(ResourceUiEvent::PodAttachConnected {
                            request: request.clone(),
                            result: Ok(input),
                        });
                        while let Some(output) = session.output.next().await {
                            let result = output.map_err(|error| error.to_string());
                            let close = matches!(result, Ok(miku_api::PodAttachOutput::Closed));
                            let _ = sender.send(ResourceUiEvent::PodAttachOutput {
                                request: request.clone(),
                                result,
                            });
                            if close {
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(ResourceUiEvent::PodAttachConnected {
                            request,
                            result: Err(error.to_string()),
                        });
                    }
                }
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let result = services
                    .attach_pod(query)
                    .await
                    .map(|session| session.input)
                    .map_err(|error| error.to_string());
                let _ = sender.send(ResourceUiEvent::PodAttachConnected { request, result });
            });
        }
    }

    fn send_pod_attach_input(&mut self, request: PodAttachInputRequest) {
        if matches!(request.input, PodAttachInput::Close) {
            self.pod_attach_inputs.remove(&request.request_id);
        }

        let Some(sender) = self.pod_attach_inputs.get(&request.request_id) else {
            return;
        };
        let _ = sender.unbounded_send(request.input);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StatusBarConnectionTone {
    Default,
    Weak,
    Accent,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StatusBarConnectionSummary {
    text: String,
    hover: Option<String>,
    tone: StatusBarConnectionTone,
}

fn status_bar_connection_summary(
    cluster_name: Option<&str>,
    connection_state: Option<&ClusterConnectionState>,
) -> StatusBarConnectionSummary {
    let Some(cluster_name) = cluster_name else {
        return StatusBarConnectionSummary {
            text: format!("{} No cluster selected", egui_phosphor::regular::CIRCLE),
            hover: None,
            tone: StatusBarConnectionTone::Weak,
        };
    };

    let text = |icon: &str, version: &str, status: &str| {
        format!("{icon} {cluster_name} | {version} | {status}")
    };

    match connection_state.unwrap_or(&ClusterConnectionState::Idle) {
        ClusterConnectionState::Idle => StatusBarConnectionSummary {
            text: text(
                egui_phosphor::regular::CIRCLE,
                "version unknown",
                "Not connected",
            ),
            hover: Some("Cluster has not been initialized yet".to_owned()),
            tone: StatusBarConnectionTone::Weak,
        },
        ClusterConnectionState::Initializing => StatusBarConnectionSummary {
            text: text(
                egui_phosphor::regular::CIRCLE_NOTCH,
                "version unknown",
                "Connecting",
            ),
            hover: Some("Initializing cluster connection".to_owned()),
            tone: StatusBarConnectionTone::Default,
        },
        ClusterConnectionState::Ready { info } => StatusBarConnectionSummary {
            text: text(
                egui_phosphor::regular::CHECK_CIRCLE,
                status_bar_cluster_version(&info.version),
                "Connected",
            ),
            hover: info
                .platform
                .as_ref()
                .map(|platform| format!("Platform: {platform}")),
            tone: StatusBarConnectionTone::Accent,
        },
        ClusterConnectionState::Failed { error } => StatusBarConnectionSummary {
            text: text(
                egui_phosphor::regular::WARNING_CIRCLE,
                "version unknown",
                "Connection failed",
            ),
            hover: Some(error.clone()),
            tone: StatusBarConnectionTone::Error,
        },
    }
}

fn status_bar_cluster_version(version: &str) -> &str {
    if version.is_empty() {
        "version unknown"
    } else {
        version
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

    #[test]
    fn status_bar_connection_summary_includes_selected_cluster_version_and_status() {
        let summary = status_bar_connection_summary(
            Some("kind-miku"),
            Some(&ClusterConnectionState::Ready {
                info: miku_api::ClusterConnectionInfo {
                    version: "v1.35.0".to_owned(),
                    platform: Some("darwin/arm64".to_owned()),
                },
            }),
        );

        assert_eq!(
            summary.text,
            format!(
                "{} kind-miku | v1.35.0 | Connected",
                egui_phosphor::regular::CHECK_CIRCLE
            )
        );
        assert_eq!(summary.hover.as_deref(), Some("Platform: darwin/arm64"));
        assert_eq!(summary.tone, StatusBarConnectionTone::Accent);
    }

    #[test]
    fn status_bar_connection_summary_reports_failed_connection() {
        let summary = status_bar_connection_summary(
            Some("kind-miku"),
            Some(&ClusterConnectionState::Failed {
                error: "forbidden".to_owned(),
            }),
        );

        assert_eq!(
            summary.text,
            format!(
                "{} kind-miku | version unknown | Connection failed",
                egui_phosphor::regular::WARNING_CIRCLE
            )
        );
        assert_eq!(summary.hover.as_deref(), Some("forbidden"));
        assert_eq!(summary.tone, StatusBarConnectionTone::Error);
    }
}
