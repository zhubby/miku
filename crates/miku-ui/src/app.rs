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
    PodResourcePanel, ResourceActionOutcome, ResourceActionRequest, ResourceLoadRequest,
    ResourceUiEvent,
};
use crate::resources::ResourceNavItem;
use crate::state::{AppState, RuntimeMode};
use crate::tabs::{AppTab, AppTabViewer};

pub struct MikuApp {
    pub(crate) state: AppState,
    pub(crate) clusters: Vec<ClusterSummary>,
    pub(crate) cluster_load_in_flight: bool,
    pub(crate) cluster_load_error: Option<String>,
    pub(crate) new_cluster_form: NewClusterForm,
    pub(crate) left_dock_state: DockState<AppTab>,
    pub(crate) center_dock_state: DockState<AppTab>,
    pub(crate) right_dock_state: DockState<AppTab>,
    pub(crate) next_inspector_id: usize,
    pub(crate) services: Option<Arc<dyn MikuServices>>,
    pub(crate) pod_resource_panel: PodResourcePanel,
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

impl MikuApp {
    pub fn new(runtime_mode: RuntimeMode) -> Self {
        tracing::debug!(?runtime_mode, "creating Miku app");
        #[cfg(not(target_arch = "wasm32"))]
        let (cluster_event_sender, cluster_event_receiver) = mpsc::channel();
        let (resource_event_sender, resource_event_receiver) = resource_mpsc::channel();
        let left_dock_state = DockState::new(vec![AppTab::Clusters, AppTab::Resources]);
        let right_dock_state = DockState::new(vec![AppTab::Inspector(1)]);
        let center_dock_state = DockState::new(vec![AppTab::Workspace(1)]);

        Self {
            state: AppState::new(runtime_mode),
            clusters: Vec::new(),
            cluster_load_in_flight: false,
            cluster_load_error: None,
            new_cluster_form: NewClusterForm::default(),
            left_dock_state,
            center_dock_state,
            right_dock_state,
            next_inspector_id: 2,
            services: None,
            pod_resource_panel: PodResourcePanel::default(),
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
        app.clusters.push(ClusterSummary {
            id: miku_core::ClusterId::new("server"),
            name: "Server".to_owned(),
            context: "server".to_owned(),
            current: true,
        });
        app.state.select_cluster("Server");
        app
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }
}

impl eframe::App for MikuApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.process_cluster_events();
        self.process_resource_events();
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
                    cluster_load_in_flight: self.cluster_load_in_flight,
                    cluster_load_error: self.cluster_load_error.as_deref(),
                    closeable: false,
                    allow_windows: false,
                    add_tab: None,
                    add_requested: false,
                    new_cluster_requested: false,
                    selected_cluster_name: None,
                    selected_resource: None,
                    selected_cluster_id: self.selected_cluster_id(),
                    pod_resource_panel: None,
                    resource_load_requests: Vec::new(),
                    resource_action_requests: Vec::new(),
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
                let selected_cluster_name = tab_viewer.selected_cluster_name;
                let selected_resource = tab_viewer.selected_resource;

                if let Some(cluster_name) = selected_cluster_name {
                    self.state.select_cluster(cluster_name);
                }
                if let Some(resource) = selected_resource {
                    self.state.select_resource(resource);
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
                    cluster_load_in_flight: self.cluster_load_in_flight,
                    cluster_load_error: self.cluster_load_error.as_deref(),
                    closeable: true,
                    allow_windows: false,
                    add_tab: Some(AppTab::Inspector(self.next_inspector_id)),
                    add_requested: false,
                    new_cluster_requested: false,
                    selected_cluster_name: None,
                    selected_resource: None,
                    selected_cluster_id: self.selected_cluster_id(),
                    pod_resource_panel: None,
                    resource_load_requests: Vec::new(),
                    resource_action_requests: Vec::new(),
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
            let selected_cluster_id = self.selected_cluster_id();
            let mut tab_viewer = AppTabViewer {
                state: &self.state,
                clusters: &self.clusters,
                cluster_load_in_flight: self.cluster_load_in_flight,
                cluster_load_error: self.cluster_load_error.as_deref(),
                closeable: true,
                allow_windows: true,
                add_tab: None,
                add_requested: false,
                new_cluster_requested: false,
                selected_cluster_name: None,
                selected_resource: None,
                selected_cluster_id,
                pod_resource_panel: Some(&mut self.pod_resource_panel),
                resource_load_requests: Vec::new(),
                resource_action_requests: Vec::new(),
            };

            show_dock_region(ui, |ui| {
                DockArea::new(&mut self.center_dock_state)
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
            for request in resource_load_requests {
                self.request_resource_load(request);
            }
            for request in resource_action_requests {
                self.request_resource_action(request);
            }
        });

        self.show_new_cluster_dialog(ui.ctx());
    }
}

impl MikuApp {
    fn open_resource_tab(&mut self, resource: ResourceNavItem) {
        let tab = AppTab::Resource(resource);
        if let Some((node, tab_index)) = self.center_dock_state.main_surface().find_tab(&tab) {
            let node_path = NodePath {
                surface: SurfaceIndex::main(),
                node,
            };
            let _ = self
                .center_dock_state
                .set_active_tab(TabPath::from((node_path, tab_index)));
            self.center_dock_state
                .set_focused_node_and_surface(node_path);
            return;
        }

        self.center_dock_state.push_to_first_leaf(tab);
    }

    fn selected_cluster_id(&self) -> Option<miku_core::ClusterId> {
        let selected_name = self.state.selected_cluster_name()?;
        self.clusters
            .iter()
            .find(|cluster| cluster.name == selected_name)
            .map(|cluster| cluster.id.clone())
    }

    fn process_resource_events(&mut self) {
        while let Ok(event) = self.resource_event_receiver.try_recv() {
            self.pod_resource_panel.apply_event(event);
        }
    }

    fn request_resource_load(&mut self, request: ResourceLoadRequest) {
        let Some(services) = self.services.clone() else {
            self.pod_resource_panel
                .apply_event(ResourceUiEvent::ResourcesLoaded {
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
                self.pod_resource_panel
                    .apply_event(ResourceUiEvent::ResourcesLoaded {
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
            self.pod_resource_panel
                .apply_event(ResourceUiEvent::ResourceActionCompleted {
                    request,
                    result: Err("resource services are not available".to_owned()),
                });
            return;
        };
        let sender = self.resource_event_sender.clone();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(runtime) = self.runtime.as_ref() else {
                self.pod_resource_panel
                    .apply_event(ResourceUiEvent::ResourceActionCompleted {
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

    Err(miku_core::MikuError::UnsupportedRuntime(
        "unknown resource action".to_owned(),
    ))
}
