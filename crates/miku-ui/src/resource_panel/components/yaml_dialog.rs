use eframe::egui;
use egui_extras::syntax_highlighting::{CodeTheme, highlight};

const RESOURCE_YAML_DIALOG_WIDTH: f32 = 760.0;
const RESOURCE_YAML_CONTENT_HEIGHT: f32 = 480.0;

pub(in crate::resource_panel) struct ResourceYamlViewDialog<'a> {
    pub(in crate::resource_panel) id: egui::Id,
    pub(in crate::resource_panel) title: String,
    pub(in crate::resource_panel) yaml: &'a str,
    pub(in crate::resource_panel) open: &'a mut bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::resource_panel) struct ResourceYamlViewDialogResponse {
    pub(in crate::resource_panel) open: bool,
}

pub(in crate::resource_panel) struct ResourceYamlEditDialog<'a> {
    pub(in crate::resource_panel) id: egui::Id,
    pub(in crate::resource_panel) title: String,
    pub(in crate::resource_panel) yaml: &'a mut String,
    pub(in crate::resource_panel) error: Option<&'a str>,
    pub(in crate::resource_panel) save_enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::resource_panel) struct ResourceYamlEditDialogResponse {
    pub(in crate::resource_panel) cancel_clicked: bool,
    pub(in crate::resource_panel) save_clicked: bool,
}

impl ResourceYamlViewDialog<'_> {
    pub(in crate::resource_panel) fn show(
        self,
        ctx: &egui::Context,
    ) -> ResourceYamlViewDialogResponse {
        let mut response = ResourceYamlViewDialogResponse::default();

        egui::Window::new(self.title)
            .id(self.id)
            .open(self.open)
            .collapsible(false)
            .resizable(true)
            .default_width(RESOURCE_YAML_DIALOG_WIDTH)
            .show(ctx, |ui| {
                yaml_viewer(ui, self.id.with("viewer"), self.yaml);
            });
        response.open = *self.open;

        response
    }
}

impl ResourceYamlEditDialog<'_> {
    pub(in crate::resource_panel) fn show(
        self,
        ctx: &egui::Context,
    ) -> ResourceYamlEditDialogResponse {
        let mut response = ResourceYamlEditDialogResponse::default();

        egui::Window::new(self.title)
            .id(self.id)
            .collapsible(false)
            .resizable(true)
            .default_width(RESOURCE_YAML_DIALOG_WIDTH)
            .show(ctx, |ui| {
                if let Some(error) = self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    ui.separator();
                }

                yaml_editor(ui, self.yaml);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        response.cancel_clicked = true;
                    }
                    if ui
                        .add_enabled(self.save_enabled, egui::Button::new("Save"))
                        .clicked()
                    {
                        response.save_clicked = true;
                    }
                });
            });

        response
    }
}

fn yaml_editor(ui: &mut egui::Ui, yaml: &mut String) {
    let theme = CodeTheme::from_memory(ui.ctx(), ui.style());
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
        let mut job = highlight(ui.ctx(), ui.style(), &theme, text.as_str(), "yaml");
        job.wrap.max_width = wrap_width;
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };

    let width = ui.available_width();
    ui.add_sized(
        [width, RESOURCE_YAML_CONTENT_HEIGHT],
        egui::TextEdit::multiline(yaml)
            .font(egui::TextStyle::Monospace)
            .code_editor()
            .desired_width(width)
            .lock_focus(true)
            .layouter(&mut layouter),
    );
}

fn yaml_viewer(ui: &mut egui::Ui, id: egui::Id, yaml: &str) {
    let theme = CodeTheme::from_memory(ui.ctx(), ui.style());
    let job = highlight(ui.ctx(), ui.style(), &theme, yaml, "yaml");
    ui.allocate_ui(
        [ui.available_width(), RESOURCE_YAML_CONTENT_HEIGHT].into(),
        |ui| {
            egui::ScrollArea::both()
                .id_salt(id)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(egui::Label::new(job).selectable(true));
                });
        },
    );
}
