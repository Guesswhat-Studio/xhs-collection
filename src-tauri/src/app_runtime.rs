use crate::constants::MAIN_WINDOW_LABEL;
use crate::{ai, app_logs, commands, xhs};
use log::LevelFilter;
use tauri::{
    plugin::{Builder as PluginBuilder, TauriPlugin},
    Manager, Runtime, Url, WindowEvent,
};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
pub(crate) fn main_navigation_guard<R: Runtime>() -> TauriPlugin<R> {
    PluginBuilder::new("main-navigation-guard")
        .on_navigation(|webview, url| {
            if webview.label() != MAIN_WINDOW_LABEL {
                return true;
            }

            let allowed = is_main_window_url(url);
            if !allowed {
                log::warn!("main_window_external_navigation_blocked url={url}");
            }
            allowed
        })
        .build()
}

fn is_main_window_url(url: &Url) -> bool {
    match url.scheme() {
        "about" | "data" | "tauri" | "asset" => true,
        "http" | "https" => matches!(
            url.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("tauri.localhost")
        ),
        _ => false,
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(main_navigation_guard())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(configure_log_plugin())
        .on_window_event(|window, event| {
            if window.label() == MAIN_WINDOW_LABEL
                && matches!(
                    event,
                    WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
                )
            {
                app_logs::copy_current_log_to_latest(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_library_overview,
            app_logs::get_log_file_info,
            app_logs::clear_log_files,
            commands::list_local_profiles,
            commands::switch_local_profile,
            commands::delete_library_database,
            commands::clear_media_files,
            commands::reset_library_data,
            commands::list_notes,
            commands::update_note_status,
            commands::update_note_metadata,
            commands::batch_update_note_metadata,
            ai::load_ai_settings,
            ai::save_ai_settings,
            ai::test_ai_settings,
            ai::load_ai_prompt_settings,
            ai::save_ai_prompt_settings,
            ai::reset_ai_prompt_settings,
            ai::list_tags,
            ai::clear_ai_tag_groups,
            ai::cancel_ai_task,
            ai::ai_classify_uncategorized,
            ai::ai_split_category,
            ai::ai_group_tags,
            ai::ai_suggest_tag_merges,
            ai::apply_tag_governance,
            commands::export_library,
            commands::create_library_backup,
            xhs::load_xhs_saved_session,
            xhs::open_xhs_login_window,
            xhs::read_xhs_login_cookies,
            xhs::test_xhs_session,
            xhs::cancel_xhs_sync,
            xhs::sync_xhs_favorites,
            xhs::sync_xhs_files,
            xhs::sync_xhs_albums,
            xhs::enrich_xhs_note_details,
            xhs::download_media_assets
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn configure_log_plugin<R: Runtime>() -> TauriPlugin<R> {
    let builder = tauri_plugin_log::Builder::new()
        .clear_targets()
        .target(Target::new(TargetKind::LogDir {
            file_name: Some(app_logs::current_log_stem().to_string()),
        }))
        .rotation_strategy(RotationStrategy::KeepAll)
        .timezone_strategy(TimezoneStrategy::UseLocal)
        .max_file_size(20 * 1024 * 1024)
        .level(LevelFilter::Info)
        .level_for("xhs_collection_lib", LevelFilter::Debug)
        .level_for("reqwest", LevelFilter::Warn)
        .level_for("tao", LevelFilter::Warn)
        .level_for("tauri", LevelFilter::Warn);

    #[cfg(debug_assertions)]
    let builder = builder.target(Target::new(TargetKind::Stdout));

    builder.build()
}
