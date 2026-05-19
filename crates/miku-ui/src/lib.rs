#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, mpsc};

use eframe::egui;
use egui_dock::{DockArea, DockState, Style, TabViewer};
use miku_api::ClusterSummary;

#[cfg(not(target_arch = "wasm32"))]
use miku_api::{CreateClusterRequest, MikuServices};

const NEW_CLUSTER_DIALOG_WIDTH: f32 = 420.0;
const CONFIG_TEXT_HEIGHT: f32 = 180.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeMode {
    Native,
    Web,
}

#[derive(Clone, Debug)]
pub struct AppState {
    runtime_mode: RuntimeMode,
    selected_cluster_name: Option<String>,
}

impl AppState {
    pub fn new(runtime_mode: RuntimeMode) -> Self {
        Self {
            runtime_mode,
            selected_cluster_name: None,
        }
    }

    pub fn runtime_mode(&self) -> RuntimeMode {
        self.runtime_mode
    }

    pub fn selected_cluster_name(&self) -> Option<&str> {
        self.selected_cluster_name.as_deref()
    }

    pub fn status_message(&self) -> &str {
        match self.selected_cluster_name {
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

pub fn install_icon_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

fn dock_region_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(4))
        .outer_margin(egui::Margin::same(2))
        .corner_radius(egui::CornerRadius::same(2))
        .fill(ui.visuals().panel_fill)
        .stroke(egui::Stroke::new(
            1.0,
            ui.visuals().widgets.inactive.bg_stroke.color,
        ))
}

fn show_dock_region<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.painter().rect_filled(
        ui.max_rect(),
        egui::CornerRadius::ZERO,
        ui.visuals().panel_fill,
    );
    dock_region_frame(ui).show(ui, add_contents)
}

pub struct MikuApp {
    state: AppState,
    clusters: Vec<ClusterSummary>,
    cluster_load_in_flight: bool,
    cluster_load_error: Option<String>,
    new_cluster_form: NewClusterForm,
    left_dock_state: DockState<AppTab>,
    center_dock_state: DockState<AppTab>,
    right_dock_state: DockState<AppTab>,
    next_inspector_id: usize,
    #[cfg(not(target_arch = "wasm32"))]
    services: Option<Arc<dyn MikuServices>>,
    #[cfg(not(target_arch = "wasm32"))]
    runtime: Option<tokio::runtime::Handle>,
    #[cfg(not(target_arch = "wasm32"))]
    cluster_event_sender: mpsc::Sender<ClusterUiEvent>,
    #[cfg(not(target_arch = "wasm32"))]
    cluster_event_receiver: mpsc::Receiver<ClusterUiEvent>,
    #[cfg(not(target_arch = "wasm32"))]
    file_dialog: egui_file_dialog::FileDialog,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct NewClusterForm {
    open: bool,
    context: String,
    config: String,
    error: Option<String>,
}

impl NewClusterForm {
    fn open(&mut self) {
        self.open = true;
        self.error = None;
    }

    fn cancel(&mut self) {
        *self = Self::default();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save_started(&mut self) {
        self.error = None;
    }

    fn save_failed(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save_succeeded(&mut self) {
        self.cancel();
    }
}

#[cfg(not(target_arch = "wasm32"))]
enum ClusterUiEvent {
    ClustersLoaded(Result<Vec<ClusterSummary>, String>),
    ClusterCreated(Result<ClusterSummary, String>),
    ConfigFileLoaded(Result<String, String>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AppTab {
    Clusters,
    Workspace(usize),
    Inspector(usize),
}

struct AppTabViewer<'a> {
    state: &'a AppState,
    clusters: &'a [ClusterSummary],
    cluster_load_in_flight: bool,
    cluster_load_error: Option<&'a str>,
    closeable: bool,
    allow_windows: bool,
    add_tab: Option<AppTab>,
    add_requested: bool,
    new_cluster_requested: bool,
}

impl TabViewer for AppTabViewer<'_> {
    type Tab = AppTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            AppTab::Clusters => "Clusters",
            AppTab::Workspace(1) => "Workspace",
            AppTab::Workspace(id) => return format!("Workspace {id}").into(),
            AppTab::Inspector(1) => "Inspector",
            AppTab::Inspector(id) => return format!("Inspector {id}").into(),
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            AppTab::Clusters => {
                ui.heading("Clusters");
                ui.separator();
                if ui
                    .add_sized(
                        [ui.available_width(), 30.0],
                        egui::Button::new("New Cluster"),
                    )
                    .clicked()
                {
                    self.new_cluster_requested = true;
                }
                ui.separator();

                if self.cluster_load_in_flight {
                    ui.label("Loading clusters...");
                } else if let Some(error) = self.cluster_load_error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                } else if self.clusters.is_empty() {
                    ui.label("No clusters loaded yet.");
                } else {
                    for cluster in self.clusters {
                        ui.label(&cluster.name)
                            .on_hover_text(format!("Context: {}", cluster.context));
                    }
                }
            }
            AppTab::Workspace(_) => {
                ui.heading("Kubernetes workspace");
                ui.label("Select a cluster to inspect namespaces, workloads, services, and logs.");
                ui.separator();
                ui.label(self.state.status_message());
            }
            AppTab::Inspector(_) => {
                ui.heading("Inspector");
                ui.separator();
                ui.label("Select a resource to inspect details.");
            }
        }
    }

    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        self.closeable && !matches!(tab, AppTab::Clusters)
    }

    fn on_add(&mut self, _path: egui_dock::NodePath) {
        self.add_requested = self.add_tab.is_some();
    }

    fn allowed_in_windows(&self, _tab: &mut Self::Tab) -> bool {
        self.allow_windows
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

impl MikuApp {
    #[cfg(not(target_arch = "wasm32"))]
    fn request_cluster_refresh(&mut self) {
        let Some(services) = self.services.clone() else {
            return;
        };
        let Some(runtime) = self.runtime.as_ref() else {
            return;
        };
        self.cluster_load_in_flight = true;
        self.cluster_load_error = None;
        let sender = self.cluster_event_sender.clone();
        runtime.spawn(async move {
            let result = services
                .list_clusters()
                .await
                .map_err(|error| error.to_string());
            let _ = sender.send(ClusterUiEvent::ClustersLoaded(result));
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn submit_new_cluster(&mut self) {
        let Some(services) = self.services.clone() else {
            self.new_cluster_form
                .save_failed("cluster storage is not available");
            return;
        };
        let Some(runtime) = self.runtime.as_ref() else {
            self.new_cluster_form
                .save_failed("cluster storage runtime is not available");
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

        self.new_cluster_form.save_started();
        self.cluster_load_in_flight = true;
        let sender = self.cluster_event_sender.clone();
        runtime.spawn(async move {
            let result = services
                .create_cluster(CreateClusterRequest { context, config })
                .await
                .map_err(|error| error.to_string());
            let _ = sender.send(ClusterUiEvent::ClusterCreated(result));
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn submit_new_cluster(&mut self) {
        self.new_cluster_form
            .save_failed("cluster storage is not available in web mode");
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn process_cluster_events(&mut self) {
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

    #[cfg(target_arch = "wasm32")]
    fn process_cluster_events(&mut self) {}

    #[cfg(not(target_arch = "wasm32"))]
    fn update_file_dialog(&mut self, ctx: &egui::Context) {
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

    #[cfg(target_arch = "wasm32")]
    fn update_file_dialog(&mut self, _ctx: &egui::Context) {}

    fn show_new_cluster_dialog(&mut self, ctx: &egui::Context) {
        if !self.new_cluster_form.open {
            return;
        }

        let mut open = self.new_cluster_form.open;
        egui::Window::new("New Cluster")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(NEW_CLUSTER_DIALOG_WIDTH);

                ui.label("Context");
                ui.text_edit_singleline(&mut self.new_cluster_form.context);
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Config");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Choose File").clicked() {
                            self.pick_config_file();
                        }
                    });
                });
                egui::ScrollArea::vertical()
                    .id_salt("new_cluster_config_scroll")
                    .max_height(CONFIG_TEXT_HEIGHT)
                    .min_scrolled_height(CONFIG_TEXT_HEIGHT)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.new_cluster_form.config)
                                .desired_width(NEW_CLUSTER_DIALOG_WIDTH)
                                .desired_rows(10),
                        );
                    });

                if let Some(error) = &self.new_cluster_form.error {
                    ui.add_space(8.0);
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.new_cluster_form.cancel();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let save_enabled = !self.cluster_load_in_flight;
                        if ui
                            .add_enabled(save_enabled, egui::Button::new("Save"))
                            .clicked()
                        {
                            self.submit_new_cluster();
                        }
                    });
                });
            });

        if !open {
            self.new_cluster_form.cancel();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_config_file(&mut self) {
        self.file_dialog.pick_file();
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_config_file(&mut self) {
        self.new_cluster_form
            .save_failed("file selection is only available in native mode");
    }

    fn show_menu_bar(&self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            if ui.button("Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        ui.menu_button("View", |ui| {
            ui.label("Workspace");
            ui.label("Logs");
        });

        ui.add_space(8.0);

        let drag_response = ui.interact(
            ui.available_rect_before_wrap(),
            ui.id().with("title_bar_drag_region"),
            egui::Sense::click_and_drag(),
        );
        if drag_response.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(egui_phosphor::regular::X)
                .on_hover_text("Close")
                .clicked()
            {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }

            if ui
                .button(egui_phosphor::regular::SQUARE)
                .on_hover_text("Maximize")
                .clicked()
            {
                let maximized = ui
                    .ctx()
                    .input(|input| input.viewport().maximized.unwrap_or(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }

            if ui
                .button(egui_phosphor::regular::MINUS)
                .on_hover_text("Minimize")
                .clicked()
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_config_file(path: &std::path::Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_native_app(
    services: Arc<dyn MikuServices>,
    runtime: tokio::runtime::Handle,
) -> eframe::Result {
    tracing::info!("launching native egui app");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        app_title(RuntimeMode::Native),
        options,
        Box::new(move |cc| {
            install_icon_fonts(&cc.egui_ctx);
            Ok(Box::new(MikuApp::native(services.clone(), runtime.clone())))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_starts_in_empty_cluster_view() {
        let state = AppState::new(RuntimeMode::Native);

        assert_eq!(state.runtime_mode(), RuntimeMode::Native);
        assert_eq!(state.selected_cluster_name(), None);
        assert_eq!(state.status_message(), "No cluster selected");
    }

    #[test]
    fn app_title_names_runtime_mode() {
        assert_eq!(app_title(RuntimeMode::Native), "Miku - Native");
        assert_eq!(app_title(RuntimeMode::Web), "Miku - Web");
    }

    #[test]
    fn new_cluster_form_cancel_clears_state() {
        let mut form = NewClusterForm {
            open: true,
            context: "kind-miku".to_owned(),
            config: "apiVersion: v1".to_owned(),
            error: Some("failed".to_owned()),
        };

        form.cancel();

        assert_eq!(form, NewClusterForm::default());
    }

    #[test]
    fn new_cluster_form_save_success_closes_and_clears_state() {
        let mut form = NewClusterForm {
            open: true,
            context: "kind-miku".to_owned(),
            config: "apiVersion: v1".to_owned(),
            error: None,
        };

        form.save_succeeded();

        assert_eq!(form, NewClusterForm::default());
    }

    #[test]
    fn new_cluster_form_save_failure_keeps_input() {
        let mut form = NewClusterForm {
            open: true,
            context: "kind-miku".to_owned(),
            config: "apiVersion: v1".to_owned(),
            error: None,
        };

        form.save_failed("duplicate context");

        assert!(form.open);
        assert_eq!(form.context, "kind-miku");
        assert_eq!(form.config, "apiVersion: v1");
        assert_eq!(form.error.as_deref(), Some("duplicate context"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn read_config_file_returns_file_contents() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "apiVersion: v1").unwrap();

        let content = read_config_file(temp.path()).unwrap();

        assert_eq!(content, "apiVersion: v1");
    }
}
