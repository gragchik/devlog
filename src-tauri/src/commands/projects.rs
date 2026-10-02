use tauri::State;

use crate::db::app_database::AppDatabase;
use crate::db::repositories::projects;
use crate::domain::project::{CreateProjectInput, Project, UpdateProjectInput};

#[tauri::command]
pub fn list_projects(db: State<AppDatabase>) -> Result<Vec<Project>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned".to_string())?;
    projects::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_project(db: State<AppDatabase>, input: CreateProjectInput) -> Result<Project, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned".to_string())?;
    // Понятная ошибка вместо голого SQL "UNIQUE constraint failed" из
    // CHECK в схеме (раздел 5 ТЗ: `repoPath` уникален).
    if projects::get_by_repo_path(&conn, &input.repo_path).map_err(|e| e.to_string())?.is_some() {
        return Err(format!("Репозиторий уже добавлен: {}", input.repo_path));
    }
    projects::create(&conn, input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_project(db: State<AppDatabase>, id: String, patch: UpdateProjectInput) -> Result<Project, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned".to_string())?;
    projects::update(&conn, &id, patch).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_project(db: State<AppDatabase>, id: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned".to_string())?;
    projects::remove(&conn, &id).map_err(|e| e.to_string())
}
