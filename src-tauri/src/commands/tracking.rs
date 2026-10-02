use tauri::State;

use crate::tracking::tracker::ActivityTrackerHandle;

#[tauri::command]
pub fn get_tracking_paused(tracker: State<ActivityTrackerHandle>) -> bool {
    tracker.is_paused()
}

#[tauri::command]
pub fn set_tracking_paused(tracker: State<ActivityTrackerHandle>, paused: bool) -> bool {
    tracker.set_paused(paused);
    tracker.is_paused()
}
