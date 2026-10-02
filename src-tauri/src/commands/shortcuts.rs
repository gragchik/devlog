use tauri::{AppHandle, State};

use crate::db::app_database::AppDatabase;
use crate::db::repositories::settings;
use crate::shortcuts::{configured_shortcut, register_overlay_shortcut, SHORTCUT_SETTINGS_KEY};

#[tauri::command]
pub fn get_overlay_shortcut(db: State<AppDatabase>) -> Result<String, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned".to_string())?;
    Ok(configured_shortcut(&conn))
}

/// FR-06.2: "разрешать назначить другую комбинацию" — живая
/// перерегистрация, без перезапуска приложения. Если новая комбинация
/// занята другим приложением, настройка **не сохраняется** (возвращаем
/// ошибку, UI должен её показать) — лучше явная ошибка, чем молча
/// сохранённый нерабочий хоткей.
#[tauri::command]
pub fn set_overlay_shortcut(app: AppHandle, db: State<AppDatabase>, accelerator: String) -> Result<(), String> {
    register_overlay_shortcut(&app, &accelerator)?;
    let conn = db.0.lock().map_err(|_| "database lock poisoned".to_string())?;
    settings::set(&conn, SHORTCUT_SETTINGS_KEY, &accelerator).map_err(|e| e.to_string())
}
