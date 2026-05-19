use eframe::egui;

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

pub(crate) fn show_dock_region<R>(
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
