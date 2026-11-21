#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod protocol;
mod recorder;
mod state;
mod theme;
mod worker;

use eframe::egui;
use std::sync::Arc;

const APP_ICON_PNG: &[u8] = include_bytes!("../assets/voxa-icon-256.png");

fn load_app_icon() -> Option<egui::IconData> {
    eframe::icon_data::from_png_bytes(APP_ICON_PNG).ok()
}

fn load_multilingual_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let candidates: &[(&str, &str, u32)] = &[
        ("win_segoeui", r"C:\Windows\Fonts\segoeui.ttf", 0),
        ("win_nirmala", r"C:\Windows\Fonts\Nirmala.ttf", 0),
        ("win_malgun", r"C:\Windows\Fonts\malgun.ttf", 0),
        ("win_msyh", r"C:\Windows\Fonts\msyh.ttc", 0),
        ("win_yugoth", r"C:\Windows\Fonts\YuGothR.ttc", 0),
    ];

    let mut loaded_names = Vec::new();
    for &(name, path, index) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut data = egui::FontData::from_owned(bytes);
            data.index = index;
            fonts.font_data.insert(name.to_string(), Arc::new(data));
            loaded_names.push(name.to_string());
        }
    }

    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        // Insert Windows fonts in priority order ahead of built-in fallbacks
        for name in loaded_names.into_iter().rev() {
            family.insert(0, name);
        }
    }

    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Voxa Studio — Neural Voice & Speech Studio")
        .with_inner_size([1180.0, 780.0])
        .with_min_inner_size([860.0, 600.0])
        .with_drag_and_drop(true);
    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Voxa Studio",
        options,
        Box::new(|cc| {
            load_multilingual_fonts(&cc.egui_ctx);
            Ok(Box::new(app::VoxaStudioApp::new(cc)))
        }),
    )
}
