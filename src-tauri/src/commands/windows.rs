use tauri::{AppHandle, Emitter};

use crate::windows::main_window::{show_main_window, MAIN_LABEL};

/// Вкладки главного окна, на которые overlay может попросить переключиться.
const NAVIGABLE_TABS: &[&str] = &["dashboard", "timeline", "worklog", "settings"];

/// FR-06.11: "Открыть отчёт"/"Открыть" из overlay. `tab` — на какую вкладку
/// переключить главное окно (`"worklog"` для "Отчёт"); без него — просто
/// показать окно как есть.
#[tauri::command]
pub fn show_main_window_command(app: AppHandle, tab: Option<String>) -> Result<(), String> {
    if let Some(tab) = &tab {
        if !NAVIGABLE_TABS.contains(&tab.as_str()) {
            return Err(format!("unknown tab '{tab}'"));
        }
    }
    show_main_window(&app);
    if let Some(tab) = tab {
        app.emit_to(MAIN_LABEL, "main:navigate", tab).map_err(|e| e.to_string())?;
    }
    Ok(())
}
