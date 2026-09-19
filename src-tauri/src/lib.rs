use crate::engine::SharedEngine;
use crate::paths::DataPaths;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, PhysicalPosition, WebviewWindow,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

static SHORTCUT_CAPTURE: AtomicBool = AtomicBool::new(false);

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
pub mod diagnostics;
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
pub mod macos_activity;
pub mod macos_stt;
pub mod media;
pub mod native_hotkey;
pub mod paths;
pub mod permissions;
pub mod personalization;
pub mod phrases;
pub mod pipeline;
pub mod platform;
pub mod profiles;
pub mod runtime;
pub mod sanitize;
pub mod screenlock;
pub mod sherpa_stt;
pub mod snippets;
pub mod spoken_tech;
pub mod stt;
pub mod textscan;
pub mod tone_stt;
pub mod uninstall;
pub mod uttlog;
pub mod vad;
pub mod whisper_stt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    crate::diagnostics::startup();
    let paths = DataPaths::detect();
    if let Err(err) = instance::acquire_gui_lock(&paths) {
        crate::diagnostics::error("startup_lock", &err.to_string());
        if !err.to_string().contains("(activated)") {
            instance::notify_already_running(&err.to_string());
        }
        std::process::exit(0);
    }
    let engine = match engine::AppEngine::open(paths) {
        Ok(engine) => engine,
        Err(err) => {
            crate::diagnostics::error("startup", &format!("code={} detail={err}", err.code()));
            std::process::exit(1);
        }
    };
    let shared: SharedEngine = Arc::new(Mutex::new(engine));
    let capture = audio::CaptureHub::spawn();

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
            commands::remove_model,
            commands::remove_unused_models,
            commands::last_utterance_ready,
            commands::repeat_last_utterance,
            commands::begin_audio_upload,
            commands::append_audio_upload,
            commands::transcribe_staged_audio,
            commands::transcribe_audio_file,
            commands::get_transcribe_progress,
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
            commands::permission_status,
            commands::relaunch_app,
            commands::pause_shortcut_capture,
            commands::resume_shortcut_capture,
            commands::read_journal
        ])
        .setup(move |app| {
            let permissions = crate::permissions::status();
            crate::journal::log(
                "permission_status",
                &format!(
                    "microphone_devices={} accessibility_trusted={}",
                    permissions.microphone_device_count, permissions.accessibility_trusted
                ),
            );
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
                tray.set_icon_as_template(cfg!(target_os = "macos"))?;
                tray.on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => show_main_window(app),
                    "copy-last" => dictation::enqueue(dictation::DictationCmd::CopyLast),
                    "paste-last" => dictation::enqueue(dictation::DictationCmd::PasteLast),
                    "cancel-dictation" => {
                        dictation::enqueue(dictation::DictationCmd::Cancel(
                            dictation::CancelSource::TrayMenu,
                        ));
                    }
                    _ => {}
                });
            } else {
                TrayIconBuilder::with_id("localflow")
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .icon_as_template(cfg!(target_os = "macos"))
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "quit" => app.exit(0),
                        "show" => show_main_window(app),
                        "copy-last" => dictation::enqueue(dictation::DictationCmd::CopyLast),
                        "paste-last" => dictation::enqueue(dictation::DictationCmd::PasteLast),
                        "cancel-dictation" => {
                            dictation::enqueue(dictation::DictationCmd::Cancel(
                                dictation::CancelSource::TrayMenu,
                            ));
                        }
                        _ => {}
                    })
                    .build(app)?;
            }

            if let Some(bar) = app.get_webview_window("bar") {
                crate::hide_flow_bar_window(&bar);
            }

            macos_activity::prevent_app_nap();
            crate::platform::current().prompt_accessibility();
            native_hotkey::set_app_handle(app.handle().clone());
            native_hotkey::start();
            dictation::start_worker(app.handle().clone(), shared.clone(), capture.clone());
            let tone_model = shared.lock().ok().and_then(|eng| {
                eng.settings
                    .stt_engine
                    .eq_ignore_ascii_case("tone")
                    .then(|| eng.ready_model_path("stt"))
                    .flatten()
            });
            if let Some(model) = tone_model {
                crate::tone_stt::preload(model);
            }
            commands::spawn_required_model_downloads(app.handle().clone(), shared.clone());

            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |_app, shortcut, event| {
                        if SHORTCUT_CAPTURE.load(Ordering::Relaxed) {
                            return;
                        }
                        let pressed =
                            event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed;
                        let released =
                            event.state == tauri_plugin_global_shortcut::ShortcutState::Released;
                        if shortcut_matches(shortcut, "Escape") && pressed {
                            dictation::enqueue(dictation::DictationCmd::Cancel(
                                dictation::CancelSource::EscapeShortcut,
                            ));
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
                                dictation::enqueue(dictation::DictationCmd::Pressed);
                            }
                            if released {
                                dictation::enqueue(dictation::DictationCmd::Released);
                            }
                        }
                    })
                    .build(),
            )?;
            apply_shortcuts(app.handle(), &shared);
            let watcher = shared.clone();
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("localflow-settings".into())
                .spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    if let Ok(mut eng) = watcher.lock() {
                        eng.reload_settings_file();
                    }
                    apply_shortcuts(&handle, &watcher);
                })
                .ok();
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running LocalFlow")
        .run(|app, event| match event {
            tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. } => {
                let (talk, copy, paste, edit) = dictation::bound_hotkeys();
                for shortcut in [talk, copy, paste, edit, "Escape".into()] {
                    if let Ok(parsed) = shortcut.parse::<Shortcut>() {
                        let _ = app.global_shortcut().unregister(parsed);
                    }
                }
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen {
                has_visible_windows,
                ..
            } => {
                if !has_visible_windows {
                    show_main_window(app);
                }
            }
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } => {
                if label == "main" || label == "bar" {
                    api.prevent_close();
                    if let Some(window) = app.get_webview_window(&label) {
                        let _ = window.hide();
                    }
                }
            }
            _ => {}
        });
}

pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub(crate) fn show_flow_bar(bar: &WebviewWindow, target_pid: Option<i32>) {
    position_flow_bar(bar);
    #[cfg(target_os = "macos")]
    {
        show_macos_overlay_without_activating(bar);
        restore_overlay_target(target_pid);
    }
    #[cfg(windows)]
    {
        let _ = bar.show();
        if let Ok(hwnd) = bar.hwnd() {
            crate::platform::restack_windows_overlay(hwnd.0 as isize);
        }
        restore_overlay_target(target_pid);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = bar.show();
        restore_overlay_target(target_pid);
    }
}

fn restore_overlay_target(target_pid: Option<i32>) {
    if let Some(pid) = target_pid {
        if pid > 0 && pid != crate::injection::own_process_id() {
            let _ = crate::platform::current().activate_pid(pid as u32);
        }
    }
}

#[cfg(target_os = "macos")]
fn show_macos_overlay_without_activating(bar: &WebviewWindow) {
    // Tauri's show() maps to makeKeyAndOrderFront, which takes the caret
    // out of Chrome so the later Cmd+V never reaches the field.
    if let Ok(ptr) = bar.ns_window() {
        if !ptr.is_null() {
            unsafe {
                let window = ptr as *mut objc2::runtime::AnyObject;
                let _: () = objc2::msg_send![window, orderFrontRegardless];
            }
            return;
        }
    }
    let _ = bar.show();
}

pub(crate) fn hide_flow_bar_window(bar: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    if let Ok(ptr) = bar.ns_window() {
        if !ptr.is_null() {
            unsafe {
                let window = ptr as *mut objc2::runtime::AnyObject;
                let _: () = objc2::msg_send![window, orderOut: std::ptr::null::<objc2::runtime::AnyObject>()];
            }
        }
    }
    let _ = bar.hide();
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
    let (talk, copy, paste, edit, previous, hands_free, vad, mic, stt_engine, pending_err) =
        match engine.lock() {
            Ok(eng) => (
                eng.settings.hotkey.clone(),
                eng.settings.copy_last_hotkey.clone(),
                eng.settings.paste_last_hotkey.clone(),
                eng.settings.edit_hotkey.clone(),
                eng.hotkey_registered.clone(),
                eng.settings.hands_free,
                eng.settings.vad_threshold,
                eng.settings.microphone_name.clone(),
                eng.settings.stt_engine.clone(),
                eng.hotkey_error.clone(),
            ),
            Err(_) => return Some("engine lock poisoned".into()),
        };
    if SHORTCUT_CAPTURE.load(Ordering::Relaxed) {
        unregister_known_shortcuts(app, previous.as_deref(), &talk, &copy, &paste, &edit);
        return None;
    }
    dictation::remember_microphone(mic);
    dictation::remember_hands_free(hands_free);
    dictation::remember_vad(vad);
    dictation::remember_stt_engine(&stt_engine);
    native_hotkey::configure(&talk);
    let already = dictation::bound_hotkeys();
    if previous.as_deref() == Some(talk.as_str())
        && already == (talk.clone(), copy.clone(), paste.clone(), edit.clone())
        && pending_err.is_none()
    {
        return None;
    }
    unregister_known_shortcuts(app, previous.as_deref(), &talk, &copy, &paste, &edit);
    let mut registered = None;
    let mut last_err = None;
    if native_hotkey::active_for(&talk) {
        registered = Some(talk.clone());
    } else {
        let fallbacks = crate::platform::talk_hotkey_fallbacks();
        let candidates = [talk.as_str(), fallbacks[0], fallbacks[1]];
        for shortcut in candidates {
            match app.global_shortcut().register(shortcut) {
                Ok(()) => {
                    registered = Some(shortcut.to_string());
                    if shortcut == talk.as_str() {
                        last_err = None;
                    }
                    break;
                }
                Err(err) => {
                    last_err = Some(format!(
                        "Hotkey {shortcut} is already used by the OS or another app ({err})"
                    ));
                }
            }
        }
    }
    let mut extra = Vec::new();
    register_named_shortcut(app, "Copy last", copy.as_str(), &mut extra);
    register_named_shortcut(app, "Paste last", paste.as_str(), &mut extra);
    register_named_shortcut(app, "Edit", edit.as_str(), &mut extra);
    register_named_shortcut(app, "Escape", "Escape", &mut extra);
    if !extra.is_empty() {
        let overlay = extra.join("; ");
        last_err = Some(match last_err {
            Some(talk_err) => format!("{talk_err}; {overlay}"),
            None => overlay,
        });
    }
    if registered.as_deref() != Some(talk.as_str()) {
        if let Some(active) = &registered {
            last_err = Some(match last_err {
                Some(err) => format!("{err} Using {active} until you pick another."),
                None => format!("Talk shortcut {talk} is unavailable. Using {active}."),
            });
        }
    }
    let talk_active = registered.clone().unwrap_or(talk);
    dictation::remember_hotkeys(talk_active, copy, paste, edit);
    if let Ok(mut eng) = engine.lock() {
        eng.hotkey_registered = registered.clone();
        eng.hotkey_error = last_err.clone();
    }
    last_err
}

fn unregister_known_shortcuts(
    app: &AppHandle,
    previous: Option<&str>,
    talk: &str,
    copy: &str,
    paste: &str,
    edit: &str,
) {
    for old in [
        previous,
        Some(talk),
        Some(copy),
        Some(paste),
        Some(edit),
        Some("Escape"),
    ]
    .into_iter()
    .flatten()
    {
        let _ = app.global_shortcut().unregister(old);
    }
}

pub fn pause_shortcuts(app: &AppHandle, engine: &SharedEngine) {
    SHORTCUT_CAPTURE.store(true, Ordering::Relaxed);
    native_hotkey::set_capture(true);
    let _ = apply_shortcuts(app, engine);
    let app = app.clone();
    let engine = engine.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));
        if SHORTCUT_CAPTURE.load(Ordering::Relaxed) {
            resume_shortcuts(&app, &engine);
        }
    });
}

pub(crate) fn shortcut_capture_active() -> bool {
    SHORTCUT_CAPTURE.load(Ordering::Relaxed)
}

pub fn resume_shortcuts(app: &AppHandle, engine: &SharedEngine) {
    SHORTCUT_CAPTURE.store(false, Ordering::Relaxed);
    native_hotkey::set_capture(false);
    if let Ok(mut eng) = engine.lock() {
        eng.hotkey_registered = None;
    }
    let _ = apply_shortcuts(app, engine);
}

fn register_named_shortcut(app: &AppHandle, name: &str, shortcut: &str, errors: &mut Vec<String>) {
    if shortcut.is_empty() {
        return;
    }
    if let Err(err) = app.global_shortcut().register(shortcut) {
        errors.push(format!(
            "{name} ({shortcut}) is already used by the OS or another app ({err})"
        ));
    }
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

    #[test]
    fn overlay_show_restores_the_remembered_app() {
        let src = include_str!("lib.rs").split("#[cfg(test)]").next().unwrap();
        assert!(
            src.contains("restore_overlay_target"),
            "Win/Linux show() steals focus unless the remembered pid is activated again"
        );
        assert!(
            !src.contains("let _ = target_pid;"),
            "do not discard the insert target when showing the flow bar"
        );
    }

    #[test]
    fn overlay_shortcuts_surface_register_errors() {
        let src = include_str!("lib.rs")
            .split("fn register_named_shortcut")
            .next()
            .unwrap();
        assert!(
            src.contains("register_named_shortcut"),
            "Copy/Paste/Edit/Escape failures must reach hotkey_error"
        );
        assert!(
            !src.contains("let _ = app.global_shortcut().register(copy"),
            "do not swallow overlay shortcut errors"
        );
        assert!(
            src.contains("apply_shortcuts(&handle, &watcher)"),
            "a settings.json rewrite must rebind live hotkeys"
        );
        assert!(
            !src.contains("eng.settings.hotkey = active"),
            "a failed bind must not overwrite the user's chosen talk shortcut"
        );
        assert!(
            src.contains("SHORTCUT_CAPTURE"),
            "recording a new shortcut must unregister the live hotkey so the field can see the key"
        );
    }

    #[test]
    fn windows_test_harness_embeds_comctl32_v6() {
        let src = include_str!("../build.rs");
        assert!(
            src.contains("Microsoft.Windows.Common-Controls"),
            "cargo test --lib on Windows dies with STATUS_ENTRYPOINT_NOT_FOUND without this"
        );
        assert!(
            src.contains("cargo:rustc-link-arg=/MANIFESTDEPENDENCY"),
            "the lib harness is not an [[test]] target; rustc-link-arg-tests would miss it"
        );
    }

    #[test]
    fn windows_installer_ships_sherpa_shared_dlls_next_to_the_exe() {
        let build = include_str!("../build.rs");
        let config = include_str!("../tauri.conf.json");
        let windows = include_str!("../tauri.windows.conf.json");
        let hooks = include_str!("../nsis-hooks.nsh");
        let runtime_directory = include_str!("../resources/runtime/.gitkeep");
        assert!(
            build.contains("bundle_sherpa_windows_dlls"),
            "shared sherpa-onnx must copy DLLs next to the profile exe when they exist"
        );
        assert!(
            build.contains("sherpa-onnx-prebuilt"),
            "copy from the sherpa extract dir after a release compile"
        );
        assert!(windows.contains("installerHooks"));
        assert!(
            config.contains("\"resources/runtime\": \"runtime\""),
            "the base Tauri config must bundle the staged runtime directory; a platform override can be discarded when it changes resources from a list to a map"
        );
        assert!(
            runtime_directory.contains("cargo check"),
            "the runtime directory must exist before Windows cargo check runs"
        );
        assert!(
            hooks.contains("$INSTDIR\\resources\\runtime\\*.dll")
                && hooks.contains("$INSTDIR\\sherpa-onnx*.dll")
                && hooks.contains("$INSTDIR\\onnxruntime*.dll"),
            "NSIS must copy shared runtime DLLs next to localflow.exe and remove them on uninstall"
        );
        let packager = include_str!("../../scripts/build-release.mjs");
        assert!(
            packager.contains("resources/runtime") && packager.contains("sherpa-onnx-c-api.dll"),
            "NSIS must stage sherpa DLLs after the Windows release compile"
        );
        assert!(
            packager.contains("verifyWindowsSherpaRuntimeInInstaller")
                && packager.contains("resources/runtime")
                && packager.contains("NSIS post-install hook did not place"),
            "Windows packaging must verify both staged and loader-visible DLL locations"
        );
        let ensure = include_str!("../../scripts/ensure-sherpa-windows-libs.mjs");
        assert!(
            ensure.contains("sherpa-onnx-c-api.lib"),
            "Windows CI rust-cache can keep an empty sherpa lib/ and skip extract"
        );
        assert!(
            build.contains("restore_sherpa_windows_prebuilt")
                && build.contains("stage_sherpa_windows_import_libs"),
            "localflow build.rs must unpack import libs even when sherpa-onnx-sys skips download"
        );
    }
}
