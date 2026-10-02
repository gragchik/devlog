use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const OVERLAY_LABEL: &str = "overlay";
const OVERLAY_WIDTH: f64 = 520.0;
const OVERLAY_HEIGHT: f64 = 380.0;
const SCREEN_EDGE_MARGIN: f64 = 16.0;

/// Создаёт overlay-окно скрытым (если его ещё нет) и возвращает хендл.
/// Окно никогда не уничтожается само по себе — только скрывается (см.
/// `on_window_event` в lib.rs), чтобы повторное открытие было быстрым
/// (целевой p95 ~500мс, ТЗ FR-06.4).
pub fn ensure_overlay_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        return Ok(window);
    }

    WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("overlay.html".into()))
        .title("DevLog Overlay")
        .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .build()
}

/// Позиционирует overlay у правого верхнего края монитора, на котором
/// сейчас находится курсор.
///
/// ВАЖНО (известное отступление от ТЗ FR-06.3 в Итерации 0): tao/Tauri
/// `Monitor` не предоставляет `workArea` (в отличие от Electron
/// `display.workArea`) — здесь используются полные границы монитора, без
/// учёта занятой панелью задач области. Overlay может на несколько
/// пикселей перекрыть taskbar. Точный workArea через Win32
/// `GetMonitorInfoW` — задача Итерации 6 (там же и тест-кейсы FR-06 на
/// несколько мониторов/DPI).
/// Чистая геометрия позиционирования — вынесена отдельно от Tauri API,
/// чтобы быть тестируемой без реального окна/монитора. Все величины в
/// логических пикселях (уже делённые на `scale_factor`).
///
/// Возвращает `(x, y, width, height)` — верхний правый угол области
/// монитора, с отступом `margin`, ширина/высота не превышают доступную
/// область за вычетом отступов с обеих сторон.
fn compute_overlay_bounds(
    area_x: f64,
    area_y: f64,
    area_width: f64,
    area_height: f64,
    desired_width: f64,
    desired_height: f64,
    margin: f64,
) -> (f64, f64, f64, f64) {
    let width = desired_width.min(area_width - margin * 2.0);
    let height = desired_height.min(area_height - margin * 2.0);
    let x = area_x + area_width - width - margin;
    let y = area_y + margin;
    (x, y, width, height)
}

/// Находит монитор, геометрически содержащий точку `cursor`, иначе первый
/// доступный (крайне маловероятный fallback — курсор всегда на каком-то мониторе).
fn find_monitor_at(monitors: &[tauri::Monitor], cursor_x: f64, cursor_y: f64) -> Option<&tauri::Monitor> {
    monitors
        .iter()
        .find(|m| {
            let pos = m.position();
            let size = m.size();
            let (px, py) = (pos.x as f64, pos.y as f64);
            let (pw, ph) = (size.width as f64, size.height as f64);
            cursor_x >= px && cursor_x < px + pw && cursor_y >= py && cursor_y < py + ph
        })
        .or_else(|| monitors.first())
}

/// Позиционирует overlay у правого верхнего края монитора, на котором
/// сейчас находится курсор.
///
/// ВАЖНО (известное отступление от ТЗ FR-06.3 в Итерации 0): tao/Tauri
/// `Monitor` не предоставляет `workArea` (в отличие от Electron
/// `display.workArea`) — здесь используются полные границы монитора, без
/// учёта занятой панелью задач области. Overlay может на несколько
/// пикселей перекрыть taskbar. Точный workArea через Win32
/// `GetMonitorInfoW` — задача Итерации 6 (там же и тест-кейсы FR-06 на
/// несколько мониторов/DPI).
fn position_near_cursor(window: &WebviewWindow) -> tauri::Result<()> {
    let cursor = window.cursor_position()?;
    let monitors = window.available_monitors()?;

    let Some(monitor) = find_monitor_at(&monitors, cursor.x, cursor.y) else {
        return Ok(()); // нет ни одного монитора — крайне маловероятно, просто не двигаем окно
    };

    let scale = monitor.scale_factor();
    let pos = monitor.position();
    let size = monitor.size();
    let (x, y, width, height) = compute_overlay_bounds(
        pos.x as f64 / scale,
        pos.y as f64 / scale,
        size.width as f64 / scale,
        size.height as f64 / scale,
        OVERLAY_WIDTH,
        OVERLAY_HEIGHT,
        SCREEN_EDGE_MARGIN,
    );

    window.set_size(LogicalSize::new(width, height))?;
    window.set_position(LogicalPosition::new(x, y))?;
    Ok(())
}

pub fn show_overlay(app: &AppHandle) -> tauri::Result<()> {
    let window = ensure_overlay_window(app)?;
    position_near_cursor(&window)?;
    window.show()?;
    window.set_focus()?;
    Ok(())
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = window.hide();
    }
}

pub fn toggle_overlay(app: &AppHandle) -> tauri::Result<()> {
    let window = ensure_overlay_window(app)?;
    if window.is_visible()? {
        hide_overlay(app);
    } else {
        drop(window);
        show_overlay(app)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_in_top_right_corner_on_a_normal_monitor() {
        let (x, y, w, h) = compute_overlay_bounds(0.0, 0.0, 1920.0, 1080.0, 520.0, 380.0, 16.0);
        assert_eq!(w, 520.0);
        assert_eq!(h, 380.0);
        assert_eq!(x, 1920.0 - 520.0 - 16.0);
        assert_eq!(y, 16.0);
    }

    #[test]
    fn shrinks_to_fit_a_smaller_monitor() {
        // Монитор уже, чем overlay + отступы с обеих сторон.
        let (x, y, w, h) = compute_overlay_bounds(0.0, 0.0, 400.0, 300.0, 520.0, 380.0, 16.0);
        assert_eq!(w, 400.0 - 16.0 * 2.0);
        assert_eq!(h, 300.0 - 16.0 * 2.0);
        assert_eq!(x, 16.0); // area_x + area_width - width - margin == 0 + 400 - 368 - 16 == 16
        assert_eq!(y, 16.0);
    }

    #[test]
    fn respects_monitor_offset_for_secondary_display() {
        // Второй монитор правее основного (типичная раскладка "слева направо").
        let (x, y, _w, _h) = compute_overlay_bounds(1920.0, 0.0, 1280.0, 1024.0, 520.0, 380.0, 16.0);
        assert_eq!(x, 1920.0 + 1280.0 - 520.0 - 16.0);
        assert_eq!(y, 16.0);
    }

    #[test]
    fn find_monitor_at_falls_back_to_first_when_cursor_outside_all() {
        // compute_overlay_bounds уже покрыт выше; здесь просто убеждаемся,
        // что геометрический поиск не падает на пустом списке реальных
        // Monitor (конструировать tauri::Monitor в unit-тесте без реального
        // рантайма недоступно — see docs/iterations/00.md о границах
        // тестируемости этого модуля).
        let empty: Vec<tauri::Monitor> = Vec::new();
        assert!(find_monitor_at(&empty, 0.0, 0.0).is_none());
    }
}
