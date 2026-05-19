use eframe::egui;

pub(in crate::resource_panel) struct ResourceToolbar<'a> {
    pub(in crate::resource_panel) id_salt: &'static str,
    pub(in crate::resource_panel) namespaces: &'a [String],
    pub(in crate::resource_panel) namespace_filter: &'a mut Option<String>,
    pub(in crate::resource_panel) search_text: &'a mut String,
    pub(in crate::resource_panel) search_hint: &'static str,
    pub(in crate::resource_panel) item_count: usize,
    pub(in crate::resource_panel) loading: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::resource_panel) struct ResourceToolbarResponse {
    pub(in crate::resource_panel) namespace_changed: bool,
    pub(in crate::resource_panel) search_changed: bool,
    pub(in crate::resource_panel) refresh_clicked: bool,
}

impl ResourceToolbar<'_> {
    pub(in crate::resource_panel) fn show(self, ui: &mut egui::Ui) -> ResourceToolbarResponse {
        let mut response = ResourceToolbarResponse::default();

        ui.horizontal(|ui| {
            let selected_label = self
                .namespace_filter
                .as_deref()
                .unwrap_or("All namespaces")
                .to_owned();

            egui::ComboBox::from_id_salt((self.id_salt, "namespace_filter"))
                .selected_text(selected_label)
                .width(220.0)
                .show_ui(ui, |ui| {
                    response.namespace_changed |= ui
                        .selectable_value(self.namespace_filter, None, "All namespaces")
                        .changed();
                    for namespace in self.namespaces {
                        response.namespace_changed |= ui
                            .selectable_value(
                                self.namespace_filter,
                                Some(namespace.clone()),
                                namespace,
                            )
                            .changed();
                    }
                });

            response.search_changed = ui
                .add(
                    egui::TextEdit::singleline(self.search_text)
                        .hint_text(self.search_hint)
                        .desired_width(280.0),
                )
                .changed();

            response.refresh_clicked = ui
                .button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                .on_hover_text("Refresh")
                .clicked();

            ui.separator();
            ui.label(format!("{} items", self.item_count));

            if self.loading {
                ui.label("Loading...");
            }
        });

        response
    }
}
