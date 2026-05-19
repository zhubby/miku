use std::sync::Arc;

use eframe::egui;

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "LXGW WenKai".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/lxgw-wenkai/LXGWWenKai-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "LXGW WenKai Mono".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/lxgw-wenkai/LXGWWenKaiMono-Regular.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "LXGW WenKai".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "LXGW WenKai Mono".to_owned());
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

pub fn install_icon_fonts(ctx: &egui::Context) {
    install_fonts(ctx);
}
