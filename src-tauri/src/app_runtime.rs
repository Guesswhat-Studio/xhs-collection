use crate::constants::MAIN_WINDOW_LABEL;
use crate::{ai, commands, xhs};
use tauri::{
    plugin::{Builder as PluginBuilder, TauriPlugin},
    Runtime, Url,
};
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
        .plugin(tauri_plugin_log::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::get_library_overview,
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
            ai::list_tags,
            ai::ai_classify_uncategorized,
            ai::ai_split_category,
            ai::ai_group_tags,
            commands::export_library,
            xhs::load_xhs_saved_session,
            xhs::open_xhs_login_window,
            xhs::read_xhs_login_cookies,
            xhs::test_xhs_session,
            xhs::sync_xhs_favorites,
            xhs::enrich_xhs_note_details,
            xhs::download_media_assets
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
