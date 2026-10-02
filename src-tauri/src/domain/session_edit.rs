use super::work_session::{SessionEditOperation, WorkSession};
use serde::Serialize;

/// Запись журнала правок сессии (FR-04.2/FR-04.8, раздел 5
/// "session_edits"). Insert-only: правки не удаляют и не перезаписывают
/// первичную историю. `previous_state`/`next_state` — полный снэпшот
/// сессии до/после (а не diff), чтобы undo был простым и не зависел от
/// порядка применения патчей.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEdit {
    pub id: String,
    pub session_id: String,
    pub edited_at_utc: i64,
    pub operation: SessionEditOperation,
    pub previous_state: WorkSession,
    pub next_state: WorkSession,
}

pub struct CreateSessionEditInput {
    pub session_id: String,
    pub operation: SessionEditOperation,
    pub previous_state: WorkSession,
    pub next_state: WorkSession,
}
