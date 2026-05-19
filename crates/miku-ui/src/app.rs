#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, mpsc};

use eframe::egui;
use egui_dock::{DockArea, DockState, Style};
use miku_api::ClusterSummary;

#[cfg(not(target_arch = "wasm32"))]
use miku_api::MikuServices;

#[cfg(not(target_arch = "wasm32"))]
use crate::cluster_events::ClusterUiEvent;
use crate::dock::show_dock_region;
use crate::forms::NewClusterForm;
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
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) services: Option<Arc<dyn MikuServices>>,
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
        let left_dock_state = DockState::new(vec![AppTab::Clusters]);
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
            #[cfg(not(target_arch = "wasm32"))]
            services: None,
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

    pub fn state(&self) -> &AppState {
        &self.state
    }
}

impl eframe::App for MikuApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.process_cluster_events();
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
        });

        self.show_new_cluster_dialog(ui.ctx());
    }
}
