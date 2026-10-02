use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::tracking::tracker::ActivityTrackerHandle;
use crate::windows::main_window::show_main_window;
use crate::windows::overlay_window::toggle_overlay;

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../../icons/tray.png");

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?;
    let overlay_item = MenuItem::with_id(app, "show_overlay", "Показать overlay", true, None::<&str>)?;
    let pause_item = MenuItem::with_id(app, "toggle_pause", "Приостановить", true, None::<&str>)?;
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
        .tooltip("DevLog — Tracking")
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "show_overlay" => {
                let _ = toggle_overlay(app);
            }
            "today" => show_main_window(app),
            "quit" => app.exit(0),
            "toggle_pause" => {
                // FR-01.2/FR-01.4: реальная пауза трекера (не заглушка —
                // появилась вместе с Activity Tracker, Итерация 2). Поток
                // опроса сам проверяет этот флаг на каждом poll и пишет
                // PAUSED вместо TRACKING, пока он установлен.
                let tracker = app.state::<ActivityTrackerHandle>();
                let now_paused = tracker.toggle_paused();
                let label = if now_paused { "Продолжить" } else { "Приостановить" };
                let _ = pause_item.set_text(label);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
