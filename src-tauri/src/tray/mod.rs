use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::windows::main_window::show_main_window;
use crate::windows::overlay_window::toggle_overlay;

/// Заглушка на Итерацию 0: реальной паузы трекера ещё нет (появится в
/// Итерации 2). Переключатель здесь только проверяет механику меню трея и
/// не должен восприниматься как настоящий Pause/Resume (FR-01.2/FR-01.4).
pub struct TrackerPauseStub(pub Mutex<bool>);

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../../icons/tray.png");

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?;
    let overlay_item = MenuItem::with_id(app, "show_overlay", "Показать overlay", true, None::<&str>)?;
    let pause_item = MenuItem::with_id(app, "toggle_pause", "Приостановить (заглушка)", true, None::<&str>)?;
    let today_item = MenuItem::with_id(app, "today", "Сегодня", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&open_item, &overlay_item, &pause_item, &today_item, &quit_item],
    )?;

    let icon = Image::from_bytes(TRAY_ICON_BYTES)?;

    TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .tooltip("DevLog — Tracking status: Unknown (итерация 0)")
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "show_overlay" => {
                let _ = toggle_overlay(app);
            }
            "today" => show_main_window(app),
            "quit" => app.exit(0),
            "toggle_pause" => {
                let state = app.state::<TrackerPauseStub>();
                let mut paused = state.0.lock().expect("tray pause mutex poisoned");
                *paused = !*paused;
                let label = if *paused { "Продолжить (заглушка)" } else { "Приостановить (заглушка)" };
                let _ = pause_item.set_text(label);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
