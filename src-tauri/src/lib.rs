mod commands;
mod db;
mod domain;
mod integrations;
mod platform;
mod shortcuts;
mod tracking;
mod tray;
mod windows;
mod worklog;

use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;
use tray::create_tray;
use windows::main_window::{show_main_window, MAIN_LABEL};
use windows::overlay_window::{ensure_overlay_window, toggle_overlay, OVERLAY_LABEL};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Должен быть первым плагином (FR-01.3: только один экземпляр приложения).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        let _ = toggle_overlay(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .invoke_handler(tauri::generate_handler![
            commands::app_info::get_app_info,
            commands::sessions::get_sessions_for_day,
            commands::sessions::update_session,
            commands::sessions::split_session,
            commands::sessions::merge_sessions,
            commands::sessions::exclude_session,
            commands::sessions::restore_session,
            commands::sessions::undo_session_edit,
            commands::sessions::create_manual_session,
            commands::projects::list_projects,
            commands::projects::add_project,
            commands::projects::update_project,
            commands::projects::remove_project,
            commands::settings::get_whitelist,
            commands::settings::set_whitelist,
            commands::settings::get_tracker_thresholds,
            commands::settings::set_tracker_thresholds,
            commands::settings::get_autostart_enabled,
            commands::settings::set_autostart_enabled,
            commands::tracking::get_tracking_paused,
            commands::tracking::set_tracking_paused,
            commands::tracking::get_pinned_issue,
            commands::tracking::set_pinned_issue,
            commands::shortcuts::get_overlay_shortcut,
            commands::shortcuts::set_overlay_shortcut,
            commands::windows::show_main_window_command,
            commands::jira::jira_get_connection_status,
            commands::jira::jira_save_connection,
            commands::jira::jira_clear_connection,
            commands::jira::jira_test_connection,
            commands::jira::jira_fetch_issue,
            commands::jira::worklog_get_day,
            commands::jira::worklog_generate_drafts,
            commands::jira::worklog_update_draft,
            commands::jira::worklog_delete_draft,
            commands::jira::jira_submit_drafts,
            commands::jira::jira_reconcile_submission,
            commands::jira::jira_list_unknown_submissions,
            commands::jira::jira_resolve_submission_manually,
        ])
        .setup(|app| {
            let handle = app.handle();

            // Открываем БД до создания окон — все команды, которые
            // понадобятся позже, должны видеть уже готовую, промигрированную
            // БД (ТЗ, раздел 5: "Все изменения из overlay и основного окна
            // выполняются через один сервис").
            let database = db::app_database::init(handle)?;
            app.manage(database);

            // Activity Tracker (Итерация 2): фоновый поток опроса, не
            // зависит от жизненного цикла окон (ТЗ, раздел 7). Хендл кладём
            // в managed state — трей дёргает pause/resume через него.
            let tracker_handle = tracking::tracker::start(handle);
            app.manage(tracker_handle);

            // Overlay создаём скрытым сразу при старте — повторное открытие
            // не пересоздаёт окно (ТЗ FR-06.4: p95 ~500мс на открытие).
            ensure_overlay_window(handle)?;
            create_tray(handle)?;

            // FR-06.2: хоткей — из settings (пользователь мог переназначить
            // в прошлой сессии), иначе дефолт.
            let accelerator = {
                let db = app.state::<db::app_database::AppDatabase>();
                let conn = db.0.lock().expect("AppDatabase mutex poisoned");
                shortcuts::configured_shortcut(&conn)
            };
            if let Err(err) = shortcuts::register_overlay_shortcut(handle, &accelerator) {
                // FR-06.2: конфликт с другим приложением — предупреждаем, не падаем.
                eprintln!("[shortcuts] Не удалось зарегистрировать {accelerator}: {err}. Overlay можно открыть через трей.");
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // FR-01.1 / FR-06.4: ни главное окно, ни overlay не завершаются по
            // крестику — только скрываются, процесс остаётся в трее.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == MAIN_LABEL || window.label() == OVERLAY_LABEL {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
