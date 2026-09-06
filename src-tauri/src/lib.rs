use crate::engine::SharedEngine;
use crate::paths::DataPaths;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, PhysicalPosition, WebviewWindow,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

pub mod audio;
pub mod autostart;
pub mod backtrack;
pub mod build_info;
pub mod catalog;
pub mod cli;
pub mod commands;
pub mod config;
pub mod cues;
pub mod db;
pub mod dictation;
pub mod dictionary;
pub mod disk;
pub mod download;
pub mod engine;
pub mod error;
pub mod eval;
pub mod format;
pub mod history;
pub mod injection;
pub mod instance;
pub mod integrity;
pub mod journal;
pub mod llm;
pub mod macos_stt;
pub mod media;
pub mod paths;
pub mod permissions;
pub mod personalization;
pub mod phrases;
pub mod pipeline;
pub mod profiles;
pub mod runtime;
pub mod sanitize;
pub mod screenlock;
pub mod snippets;
pub mod stt;
pub mod uninstall;
pub mod uttlog;
pub mod vad;
pub mod whisper_stt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = DataPaths::detect();
    if let Err(err) = instance::acquire_gui_lock(&paths) {
        if !err.to_string().contains("(activated)") {
            instance::notify_already_running(&err.to_string());
        }
        std::process::exit(0);
    }
    let engine = engine::AppEngine::open(paths).expect("open LocalFlow data directory");
    let shared: SharedEngine = Arc::new(Mutex::new(engine));
    let capture = audio::CaptureHub::spawn();
    let watcher = shared.clone();
    std::thread::Builder::new()
        .name("localflow-settings".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if let Ok(mut eng) = watcher.lock() {
                eng.reload_settings_file();
            }
        })
        .ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(shared.clone())
        .manage(capture.clone())
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
            commands::disk_usage,
            commands::verify_model,
            commands::download_model,
            commands::list_model_status,
            commands::set_active_model,
            commands::last_utterance_ready,
            commands::repeat_last_utterance,
            commands::get_hotkey_status,
            commands::dictation_stop,
            commands::dictation_cancel,
            commands::get_last_transcript,
            commands::copy_last_transcript,
            commands::paste_last_transcript,
            commands::clear_last_transcript,
            commands::import_dictionary,
            commands::search_dictionary,
            commands::list_snippets,
            commands::upsert_snippet,
            commands::remove_snippet,
            commands::list_profiles,
            commands::save_profiles,
            commands::get_active_context,
            commands::record_correction,
            commands::list_suggestions,
            commands::accept_suggestion,
            commands::dismiss_suggestion,
            commands::delete_history_item,
            commands::update_history_output,
            commands::retry_history,
            commands::history_to_snippet,
            commands::copy_text,
            commands::paste_text,
            commands::uninstall_localflow,
            commands::reset_stats,
            commands::reset_settings,
            commands::get_stats,
            commands::export_history_timecodes,
            commands::install_dictate_macro,
            commands::export_stats_csv,
            commands::is_screen_locked,
            commands::open_privacy_pane,
            commands::permission_status
        ])
        .setup(move |app| {
            let show = MenuItem::with_id(app, "show", "Open LocalFlow", true, None::<&str>)?;
            let copy_last =
                MenuItem::with_id(app, "copy-last", "Copy Last Transcript", true, None::<&str>)?;
            let paste_last = MenuItem::with_id(
                app,
                "paste-last",
                "Paste Last Transcript",
                true,
                None::<&str>,
            )?;
            let cancel_item = MenuItem::with_id(
                app,
                "cancel-dictation",
                "Cancel Dictation",
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu =
                Menu::with_items(app, &[&show, &copy_last, &paste_last, &cancel_item, &quit])?;
            if let Some(tray) = app.tray_by_id("localflow") {
                tray.set_menu(Some(menu))?;
                tray.set_show_menu_on_left_click(true)?;
                tray.set_icon_as_template(true)?;
                tray.on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "copy-last" => dictation::enqueue(dictation::DictationCmd::CopyLast),
                    "paste-last" => dictation::enqueue(dictation::DictationCmd::PasteLast),
                    "cancel-dictation" => {
                        dictation::enqueue(dictation::DictationCmd::Cancel);
                    }
                    _ => {}
                });
            } else {
                TrayIconBuilder::with_id("localflow")
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .icon_as_template(true)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "quit" => app.exit(0),
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "copy-last" => dictation::enqueue(dictation::DictationCmd::CopyLast),
                        "paste-last" => dictation::enqueue(dictation::DictationCmd::PasteLast),
                        "cancel-dictation" => {
                            dictation::enqueue(dictation::DictationCmd::Cancel);
                        }
                        _ => {}
                    })
                    .build(app)?;
            }

            if let Some(bar) = app.get_webview_window("bar") {
                position_flow_bar(&bar);
                let _ = bar.hide();
            }

            dictation::start_worker(app.handle().clone(), shared.clone(), capture.clone());
            commands::spawn_required_stt_download(app.handle().clone(), shared.clone());

            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |app, shortcut, event| {
                        let pressed =
                            event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed;
                        let released =
                            event.state == tauri_plugin_global_shortcut::ShortcutState::Released;
                        if shortcut_matches(shortcut, "Escape") && pressed {
                            dictation::enqueue(dictation::DictationCmd::Cancel);
                            return;
                        }
                        let (talk, copy, paste, edit) = dictation::bound_hotkeys();
                        if shortcut_matches(shortcut, &copy) && pressed {
                            dictation::enqueue(dictation::DictationCmd::CopyLast);
                            return;
                        }
                        if shortcut_matches(shortcut, &paste) && pressed {
                            dictation::enqueue(dictation::DictationCmd::PasteLast);
                            return;
                        }
                        if shortcut_matches(shortcut, &talk) || shortcut_matches(shortcut, &edit) {
                            if pressed {
                                if dictation::is_busy() {
                                    return;
                                }
                                dictation::notify_hotkey(app, "pressed");
                                dictation::enqueue(dictation::DictationCmd::Pressed);
                            }
                            if released {
                                dictation::notify_hotkey(app, "released");
                                dictation::enqueue(dictation::DictationCmd::Released);
                            }
                        }
                    })
                    .build(),
            )?;
            apply_shortcuts(app.handle(), &shared);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running LocalFlow")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
            ) {
                let (talk, copy, paste, edit) = dictation::bound_hotkeys();
                for shortcut in [talk, copy, paste, edit, "Escape".into()] {
                    if let Ok(parsed) = shortcut.parse::<Shortcut>() {
                        let _ = app.global_shortcut().unregister(parsed);
                    }
                }
            }
        });
}

pub(crate) fn position_flow_bar(bar: &WebviewWindow) {
    if let Ok(Some(monitor)) = bar.primary_monitor() {
        let scale = monitor.scale_factor();
        let size = monitor.size();
        let origin = monitor.position();
        let bar_w = (420.0 * scale) as i32;
        let x = origin.x + ((size.width as i32) - bar_w).max(0) / 2;
        let y = origin.y + (36.0 * scale) as i32;
        let _ = bar.set_position(PhysicalPosition::new(x, y));
    } else {
        let _ = bar.set_position(PhysicalPosition::new(200, 48));
    }
}

fn shortcut_matches(event: &Shortcut, configured: &str) -> bool {
    configured
        .parse::<Shortcut>()
        .ok()
        .is_some_and(|parsed| parsed.key == event.key && parsed.mods == event.mods)
}

pub fn apply_shortcuts(app: &AppHandle, engine: &SharedEngine) -> Option<String> {
    let (talk, copy, paste, edit, previous, hands_free, vad, mic) = match engine.lock() {
        Ok(eng) => (
            eng.settings.hotkey.clone(),
            eng.settings.copy_last_hotkey.clone(),
            eng.settings.paste_last_hotkey.clone(),
            eng.settings.edit_hotkey.clone(),
            eng.hotkey_registered.clone(),
            eng.settings.hands_free,
            eng.settings.vad_threshold,
            eng.settings.microphone_name.clone(),
        ),
        Err(_) => return Some("engine lock poisoned".into()),
    };
    dictation::remember_microphone(mic);
    dictation::remember_hands_free(hands_free);
    dictation::remember_vad(vad);
    let already = dictation::bound_hotkeys();
    if previous.as_deref() == Some(talk.as_str())
        && already == (talk.clone(), copy.clone(), paste.clone(), edit.clone())
    {
        return None;
    }
    for old in [
        previous.as_deref(),
        Some(talk.as_str()),
        Some(copy.as_str()),
        Some(paste.as_str()),
        Some(edit.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        let _ = app.global_shortcut().unregister(old);
    }
    let candidates = [talk.as_str(), "Control+Shift+Space", "Command+Shift+D"];
    let mut registered = None;
    let mut last_err = None;
    for shortcut in candidates {
        match app.global_shortcut().register(shortcut) {
            Ok(()) => {
                registered = Some(shortcut.to_string());
                last_err = None;
                break;
            }
            Err(err) => {
                last_err = Some(format!(
                    "Hotkey {shortcut} is already used by macOS or another app ({err})"
                ));
            }
        }
    }
    let _ = app.global_shortcut().register(copy.as_str());
    let _ = app.global_shortcut().register(paste.as_str());
    let _ = app.global_shortcut().register(edit.as_str());
    let _ = app.global_shortcut().register("Escape");
    let talk_active = registered.clone().unwrap_or(talk);
    dictation::remember_hotkeys(talk_active.clone(), copy, paste, edit);
    if let Ok(mut eng) = engine.lock() {
        eng.hotkey_registered = registered.clone();
        if let Some(active) = &registered {
            eng.settings.hotkey = active.clone();
        }
        eng.hotkey_error = last_err.clone();
    }
    last_err
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn talk_hotkey_matches_plugin_display_order() {
        let event: Shortcut = "shift+control+Space".parse().unwrap();
        assert!(shortcut_matches(&event, "Control+Shift+Space"));
        assert!(shortcut_matches(&event, "Ctrl+Shift+Space"));
        assert!(!shortcut_matches(&event, "Command+Shift+D"));
    }

    #[test]
    fn copy_hotkey_matches_command_control() {
        let event: Shortcut = "Command+Control+C".parse().unwrap();
        assert!(shortcut_matches(&event, "Command+Control+C"));
        assert_eq!(event.to_string(), "control+super+KeyC");
    }
}
