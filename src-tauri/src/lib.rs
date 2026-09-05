use crate::engine::SharedEngine;
use crate::paths::DataPaths;
use crate::pipeline::PipelineState;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

pub mod audio;
pub mod build_info;
pub mod catalog;
pub mod commands;
pub mod config;
pub mod db;
pub mod dictionary;
pub mod engine;
pub mod error;
pub mod history;
pub mod injection;
pub mod integrity;
pub mod llm;
pub mod paths;
pub mod personalization;
pub mod pipeline;
pub mod runtime;
pub mod stt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = DataPaths::detect();
    let engine = engine::AppEngine::open(paths).expect("open LocalFlow data directory");
    let shared: SharedEngine = Arc::new(Mutex::new(engine));

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(shared.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_build_info,
            commands::get_snapshot,
            commands::get_settings,
            commands::save_settings,
            commands::list_models,
            commands::list_microphones,
            commands::list_dictionary,
            commands::upsert_dictionary_entry,
            commands::remove_dictionary_entry,
            commands::export_configuration,
            commands::import_configuration,
            commands::list_history,
            commands::delete_history,
            commands::reset_personalization,
            commands::process_transcript,
            commands::complete_onboarding,
            commands::privacy_summary,
            commands::verify_model
        ])
        .setup(move |app| {
            let show = MenuItem::with_id(app, "show", "Open LocalFlow", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            let handle = app.handle().clone();
            let engine_for_hotkey = shared.clone();
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |_app, _shortcut, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            if let Ok(mut eng) = engine_for_hotkey.lock() {
                                let _ = eng.snapshot.transition(PipelineState::Recording);
                            }
                        }
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Released {
                            if let Some(window) = handle.get_webview_window("main") {
                                let _ = window.emit("localflow://hotkey-released", ());
                            }
                        }
                    })
                    .build(),
            )?;
            app.global_shortcut().register("Alt+Space")?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running LocalFlow");
}
