use eframe::egui;

pub(crate) fn copy_name_menu_item(ui: &mut egui::Ui, name: impl AsRef<str>) -> bool {
    if ui
        .button(format!("{} Copy Name", egui_phosphor::regular::COPY))
        .clicked()
    {
        ui.ctx().copy_text(name.as_ref().to_owned());
        ui.close();
        true
    } else {
        false
    }
}
