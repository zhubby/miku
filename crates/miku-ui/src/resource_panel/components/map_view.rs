use eframe::egui::{self, TextWrapMode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceMapEntry {
    pub(crate) key: String,
    pub(crate) value: String,
}

impl ResourceMapEntry {
    pub(crate) fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }

    fn selector(&self) -> String {
        format!("{}={}", self.key, self.value)
    }
}

pub(crate) struct ResourceMapView<'a> {
    pub(crate) id_salt: &'a str,
    pub(crate) icon: &'a str,
    pub(crate) title: &'a str,
    pub(crate) entries: &'a [ResourceMapEntry],
    pub(crate) empty_label: &'a str,
}

impl ResourceMapView<'_> {
    pub(crate) fn show(self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(self.icon);
            ui.strong(self.title);
            count_badge(ui, self.entries.len());
        });

        if self.entries.is_empty() {
            ui.label(egui::RichText::new(self.empty_label).weak());
            return;
        }

        let compact = self.entries.iter().all(ResourceMapEntry::is_chip_friendly);
        if compact {
            ui.horizontal(|ui| {
                for entry in self.entries {
                    map_chip(ui, entry);
                }
            });
        } else {
            map_table(ui, self.id_salt, self.entries);
        }
    }
}

impl ResourceMapEntry {
    fn is_chip_friendly(&self) -> bool {
        self.key.chars().count() <= 36
            && self.value.chars().count() <= 42
            && !self.value.contains('\n')
    }
}

fn count_badge(ui: &mut egui::Ui, count: usize) {
    let text = if count == 1 {
        "1 item".to_owned()
    } else {
        format!("{count} items")
    };
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).weak().small());
        });
}

fn map_chip(ui: &mut egui::Ui, entry: &ResourceMapEntry) {
    let text_color = ui.visuals().text_color();
    let key_color = ui.visuals().weak_text_color();
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(egui::Stroke::new(
            1.0,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add(
                    egui::Label::new(egui::RichText::new(&entry.key).color(key_color).monospace())
                        .wrap_mode(TextWrapMode::Extend)
                        .selectable(true),
                );
                ui.label(egui::RichText::new("=").color(key_color).monospace());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&entry.value)
                            .color(text_color)
                            .monospace(),
                    )
                    .wrap_mode(TextWrapMode::Extend)
                    .selectable(true),
                );
            });
        })
        .response
        .on_hover_text(entry.selector());
}

fn map_table(ui: &mut egui::Ui, id_salt: &str, entries: &[ResourceMapEntry]) {
    egui::Grid::new((id_salt, "resource-map"))
        .num_columns(2)
        .spacing([14.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            for entry in entries {
                ui.add_sized(
                    [240.0, 0.0],
                    egui::Label::new(egui::RichText::new(&entry.key).weak().monospace())
                        .wrap_mode(TextWrapMode::Extend)
                        .selectable(true),
                );
                ui.add_sized(
                    [720.0, 0.0],
                    egui::Label::new(egui::RichText::new(&entry.value).monospace())
                        .wrap_mode(TextWrapMode::Extend)
                        .selectable(true),
                );
                ui.end_row();
            }
        });
}
