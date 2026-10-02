use tauri::AppHandle;

use crate::windows::main_window::show_main_window;

/// FR-06.11: "Открыть отчёт"/"Открыть" из overlay — полноценного Worklog
/// Review ещё нет (Итерация 7/8), пока оба действия просто показывают
/// главное окно. Когда появится отдельная страница отчёта, этой команде
/// можно будет передавать, на какую вкладку переключиться.
#[tauri::command]
pub fn show_main_window_command(app: AppHandle) {
    show_main_window(&app);
}
