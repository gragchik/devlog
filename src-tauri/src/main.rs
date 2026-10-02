// Тонкая точка входа — вся сборка Tauri Builder в lib.rs (стандартная
// конвенция Tauri 2, упрощает будущую поддержку мобильных таргетов).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    devlog_lib::run();
}
