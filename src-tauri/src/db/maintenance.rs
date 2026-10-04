//! Обслуживание БД (Итерация 9): проверки целостности, резервные копии,
//! экспорт и удаление данных. Без Tauri — пути приходят снаружи.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{Local, TimeZone};
use rusqlite::{types::ValueRef, Connection};
use serde::Serialize;
use serde_json::{json, Map, Value};

use super::ids::now_utc_seconds;
use super::migrations;
use super::repositories::work_sessions;
use crate::domain::work_session::WorkSessionReviewStatus;

/// Сессия длиннее этого — почти наверняка "гигантский интервал" после
/// сбоя (E2E-сценарий 9), а не реальная непрерывная работа.
const SUSPICIOUS_SESSION_SECONDS: i64 = 12 * 3600;
const KEEP_MIGRATION_BACKUPS: usize = 3;

/// Таблицы с историей работы (не настройки) в порядке, безопасном для
/// удаления при включённых внешних ключах.
const HISTORY_TABLES: &[&str] = &["session_edits", "jira_submissions", "worklog_drafts", "work_sessions", "activity_events", "issues_cache"];
const EXPORT_TABLES: &[&str] = &["projects", "settings", "activity_events", "work_sessions", "session_edits", "worklog_drafts", "jira_submissions", "issues_cache"];

// ---------- Целостность ----------

/// `PRAGMA quick_check` — повреждение файла SQLite (диск, обрыв питания).
pub fn sqlite_quick_check(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn.prepare("PRAGMA quick_check").map_err(|e| e.to_string())?;
    let rows: Vec<String> = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())?;
    if rows == ["ok"] {
        Ok(())
    } else {
        Err(rows.join("; "))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsistencyIssue {
    /// Машинный код для UI/тестов.
    pub kind: String,
    pub message: String,
}

fn issue(kind: &str, message: String) -> ConsistencyIssue {
    ConsistencyIssue { kind: kind.into(), message }
}

fn local_time(ts: i64) -> String {
    Local.timestamp_opt(ts, 0).single().map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| ts.to_string())
}

/// Доменные инварианты, которые SQLite сам не проверяет: двойной учёт
/// времени (FR-04.6), подозрительно длинные/"распухшие" сессии (сценарий
/// 9), черновики со ссылками на исчезнувшие сессии.
pub fn consistency_report(conn: &Connection) -> Result<Vec<ConsistencyIssue>, String> {
    let mut issues = Vec::new();
    let sessions = work_sessions::list_by_range(conn, i64::MIN, i64::MAX, false).map_err(|e| e.to_string())?;
    let counted: Vec<_> = sessions.iter().filter(|s| s.review_status != WorkSessionReviewStatus::Excluded).collect();

    // `list_by_range` отсортирован по началу — достаточно сравнить с
    // самым поздним концом среди предыдущих.
    let mut max_end: Option<(i64, &str)> = None;
    let mut overlaps = 0;
    for s in &counted {
        let end = s.ended_at_utc.unwrap_or(s.started_at_utc);
        if let Some((prev_end, prev_id)) = max_end {
            if s.started_at_utc < prev_end {
                overlaps += 1;
                if overlaps <= 5 {
                    issues.push(issue("overlap", format!("сессии пересекаются ({}): {} и {} — время может считаться дважды", local_time(s.started_at_utc), prev_id, s.id)));
                }
            }
        }
        if max_end.is_none_or(|(e, _)| end > e) {
            max_end = Some((end, &s.id));
        }

        if let Some(ended) = s.ended_at_utc {
            let duration = ended - s.started_at_utc;
            if duration > SUSPICIOUS_SESSION_SECONDS {
                issues.push(issue("too-long", format!("сессия {} длится {} ч — проверьте, не захвачен ли сон/ночь", local_time(s.started_at_utc), duration / 3600)));
            }
            if s.active_seconds > duration {
                issues.push(issue("active-exceeds-duration", format!("сессия {}: активное время больше её длительности", local_time(s.started_at_utc))));
            }
        } else if s.started_at_utc < now_utc_seconds() - 86_400 {
            issues.push(issue("open-session", format!("сессия {} не закрыта больше суток", local_time(s.started_at_utc))));
        }
    }
    if overlaps > 5 {
        issues.push(issue("overlap", format!("…и ещё {} пересечений", overlaps - 5)));
    }

    let existing: HashSet<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
    let mut stmt = conn.prepare("SELECT localDay, issueKey, selectedSessionIdsJson FROM worklog_drafts WHERE status = 'draft'").map_err(|e| e.to_string())?;
    let drafts = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).map_err(|e| e.to_string())?;
    for row in drafts {
        let (day, key, json) = row.map_err(|e| e.to_string())?;
        let ids: Vec<String> = serde_json::from_str(&json).unwrap_or_default();
        let missing = ids.iter().filter(|id| !existing.contains(id.as_str())).count();
        if missing > 0 {
            issues.push(issue("draft-missing-sessions", format!("черновик {key} за {day}: {missing} сессий больше не существует — нажмите «Пересчитать из сессий»")));
        }
    }

    let unknown: i64 = conn.query_row("SELECT COUNT(*) FROM jira_submissions WHERE state IN ('unknown', 'pending')", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if unknown > 0 {
        issues.push(issue("unknown-submissions", format!("{unknown} отправок в Jira с неизвестным результатом — проверьте во вкладке Worklog")));
    }
    Ok(issues)
}

// ---------- Резервные копии ----------

/// Атомарная копия всей БД в один файл (`VACUUM INTO`) — консистентна даже
/// при работающем трекере, включает содержимое WAL.
pub fn backup_to(conn: &Connection, target: &Path) -> Result<(), String> {
    if target.exists() {
        std::fs::remove_file(target).map_err(|e| format!("не удалось заменить {}: {e}", target.display()))?;
    }
    conn.execute("VACUUM INTO ?1", [target.to_string_lossy()]).map(|_| ()).map_err(|e| e.to_string())
}

fn applied_schema_version(conn: &Connection) -> Option<i64> {
    conn.query_row("SELECT MAX(version) FROM _migrations", [], |r| r.get::<_, Option<i64>>(0)).ok().flatten()
}

pub fn latest_schema_version() -> i64 {
    migrations::all().iter().map(|m| m.version).max().unwrap_or(0)
}

/// Перед применением новых миграций к уже существующей БД снимаем копию —
/// обновление приложения не должно рисковать историей (Итерации 9/10).
/// Возвращает путь копии, если она была сделана.
pub fn backup_before_migrations(db_path: &Path, backups_dir: &Path) -> Result<Option<PathBuf>, String> {
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    let Some(current) = applied_schema_version(&conn) else { return Ok(None) };
    if current >= latest_schema_version() {
        return Ok(None);
    }
    std::fs::create_dir_all(backups_dir).map_err(|e| e.to_string())?;
    let target = backups_dir.join(format!("devlog-schema{current}-{}.sqlite3", Local::now().format("%Y%m%d-%H%M%S")));
    backup_to(&conn, &target)?;
    prune_backups(backups_dir, KEEP_MIGRATION_BACKUPS);
    Ok(Some(target))
}

fn prune_backups(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<PathBuf> =
        entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("devlog-schema") && n.ends_with(".sqlite3"))).collect();
    // Имя содержит дату-время в сортируемом формате.
    files.sort();
    let excess = files.len().saturating_sub(keep);
    for old in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(old);
    }
}

/// Файл не прошёл `quick_check` — сохраняем его байт-в-байт рядом (вместе
/// с WAL), прежде чем приложение начнёт в него писать: шанс восстановить
/// данные вручную важнее экономии места.
pub fn preserve_corrupt_copy(db_path: &Path) -> Result<PathBuf, String> {
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let target = db_path.with_file_name(format!("devlog.corrupt-{stamp}.sqlite3"));
    std::fs::copy(db_path, &target).map_err(|e| e.to_string())?;
    let wal = db_path.with_extension("sqlite3-wal");
    if wal.exists() {
        let _ = std::fs::copy(&wal, target.with_extension("sqlite3-wal"));
    }
    Ok(target)
}

// ---------- Экспорт ----------

fn value_ref_to_json(v: ValueRef) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
        ValueRef::Blob(b) => json!(format!("<blob {} bytes>", b.len())),
    }
}

fn table_to_json(conn: &Connection, table: &str) -> Result<Vec<Value>, String> {
    // Имя таблицы — только из констант выше, не из ввода.
    let mut stmt = conn.prepare(&format!("SELECT * FROM {table}")).map_err(|e| e.to_string())?;
    let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
    let rows = stmt
        .query_map([], |row| {
            let mut obj = Map::new();
            for (i, name) in columns.iter().enumerate() {
                obj.insert(name.clone(), value_ref_to_json(row.get_ref(i)?));
            }
            Ok(Value::Object(obj))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())
}

/// Полный экспорт по явному запросу (раздел 5 ТЗ). Секретов в БД нет по
/// построению (токен — в Credential Manager, ADR-0008), так что выгружаются
/// все таблицы как есть.
pub fn export_json(conn: &Connection) -> Result<String, String> {
    let mut tables = Map::new();
    for table in EXPORT_TABLES {
        tables.insert(table.to_string(), Value::Array(table_to_json(conn, table)?));
    }
    let doc = json!({
        "format": "devlog-export",
        "formatVersion": 1,
        "exportedAt": Local::now().to_rfc3339(),
        "schemaVersion": applied_schema_version(conn),
        "tables": tables,
    });
    serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())
}

fn csv_field(value: &str) -> String {
    // Формулы в ячейках (`=`, `+`, `-`, `@`) — защита от CSV-injection при
    // открытии в Excel: описание сессии пишет пользователь.
    let guarded = if value.starts_with(['=', '+', '-', '@']) { format!("'{value}") } else { value.to_string() };
    if guarded.contains([',', '"', '\n', '\r', ';']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

/// Сессии в CSV (для таблиц/ручного отчёта). Время — локальное.
pub fn export_sessions_csv(conn: &Connection) -> Result<String, String> {
    let sessions = work_sessions::list_by_range(conn, i64::MIN, i64::MAX, false).map_err(|e| e.to_string())?;
    let mut out = String::from("date,start,end,issueKey,seconds,status,source,manuallyEdited,description\r\n");
    for s in sessions {
        let start = Local.timestamp_opt(s.started_at_utc, 0).single();
        let end = s.ended_at_utc.and_then(|e| Local.timestamp_opt(e, 0).single());
        let fields = [
            start.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
            start.map(|d| d.format("%H:%M:%S").to_string()).unwrap_or_default(),
            end.map(|d| d.format("%H:%M:%S").to_string()).unwrap_or_default(),
            s.issue_key.unwrap_or_default(),
            s.manual_seconds_override.unwrap_or(s.active_seconds).to_string(),
            s.review_status.as_db_str().to_string(),
            s.source.as_db_str().to_string(),
            s.is_manually_edited.to_string(),
            s.description.unwrap_or_default(),
        ];
        out.push_str(&fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    Ok(out)
}

// ---------- Удаление ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteScope {
    /// История, начавшаяся раньше момента (UTC-секунды).
    HistoryBefore(i64),
    /// Вся история работы; настройки и проекты остаются.
    AllHistory,
    /// Всё, включая настройки и список репозиториев.
    Everything,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSummary {
    pub activity_events: usize,
    pub work_sessions: usize,
    pub worklog_drafts: usize,
}

/// FR-02.7: удаление истории. Одна транзакция, затем `VACUUM` и обрезка
/// WAL: без них удалённые строки физически оставались бы в файле.
pub fn delete_data(conn: &mut Connection, scope: DeleteScope) -> Result<DeleteSummary, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let run = |sql: &str, params: &[&dyn rusqlite::ToSql]| tx.execute(sql, params).map_err(|e| e.to_string());
    let summary = match scope {
        DeleteScope::HistoryBefore(before) => {
            run("DELETE FROM session_edits WHERE sessionId IN (SELECT id FROM work_sessions WHERE startedAtUtc < ?1)", &[&before])?;
            run(
                "DELETE FROM jira_submissions WHERE localDraftId IN (SELECT id FROM worklog_drafts WHERE startedAtUtc < ?1)",
                &[&before],
            )?;
            DeleteSummary {
                worklog_drafts: run("DELETE FROM worklog_drafts WHERE startedAtUtc < ?1", &[&before])?,
                work_sessions: run("DELETE FROM work_sessions WHERE startedAtUtc < ?1", &[&before])?,
                activity_events: run("DELETE FROM activity_events WHERE timestampUtc < ?1", &[&before])?,
            }
        }
        DeleteScope::AllHistory | DeleteScope::Everything => {
            let count = |table: &str| tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).map(|c| c as usize).map_err(|e| e.to_string());
            let summary = DeleteSummary { activity_events: count("activity_events")?, work_sessions: count("work_sessions")?, worklog_drafts: count("worklog_drafts")? };
            for table in HISTORY_TABLES {
                run(&format!("DELETE FROM {table}"), &[])?;
            }
            if scope == DeleteScope::Everything {
                run("DELETE FROM settings", &[])?;
                run("DELETE FROM projects", &[])?;
            }
            summary
        }
    };
    tx.commit().map_err(|e| e.to_string())?;
    conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);").map_err(|e| e.to_string())?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::database::create_database;
    use crate::db::repositories::{activity_events, settings, worklog_drafts};
    use crate::db::test_support::{build_sample_day_fixture, temp_database};
    use crate::domain::work_session::{CreateWorkSessionInput, WorkSessionSource};
    use crate::domain::worklog_draft::CreateWorklogDraftInput;

    const DAY: i64 = 1_700_000_000;

    fn session(start: i64, end: i64, active: i64, key: Option<&str>) -> CreateWorkSessionInput {
        CreateWorkSessionInput {
            started_at_utc: start,
            ended_at_utc: Some(end),
            timezone_id: "UTC".into(),
            active_seconds: active,
            source: WorkSessionSource::Manual,
            project_id: None,
            issue_key: key.map(str::to_string),
            manual_seconds_override: None,
            description: None,
            confidence: None,
            review_status: None,
            is_manually_edited: None,
        }
    }

    fn seed(conn: &mut Connection) {
        let fixture = build_sample_day_fixture(DAY, "UTC");
        activity_events::insert_many(conn, &fixture.events).unwrap();
        for s in &fixture.sessions {
            work_sessions::create(conn, s).unwrap();
        }
    }

    #[test]
    fn healthy_database_passes_checks() {
        let (mut conn, _dir) = temp_database();
        seed(&mut conn);
        sqlite_quick_check(&conn).unwrap();
        assert_eq!(consistency_report(&conn).unwrap(), vec![]);
    }

    #[test]
    fn consistency_report_finds_overlap_giant_interval_and_dangling_draft() {
        let (conn, _dir) = temp_database();
        work_sessions::create(&conn, &session(DAY, DAY + 3600, 3600, Some("A-1"))).unwrap();
        work_sessions::create(&conn, &session(DAY + 1800, DAY + 5400, 3600, Some("B-1"))).unwrap();
        work_sessions::create(&conn, &session(DAY + 86_400, DAY + 86_400 + 15 * 3600, 600, None)).unwrap();
        worklog_drafts::create(
            &conn,
            &CreateWorklogDraftInput { local_day: "d".into(), issue_key: "A-1".into(), time_spent_seconds: 60, started_at_utc: DAY, comment: None, selected_session_ids: vec!["gone".into()], status: None },
        )
        .unwrap();

        let kinds: Vec<String> = consistency_report(&conn).unwrap().into_iter().map(|i| i.kind).collect();
        assert!(kinds.contains(&"overlap".to_string()), "{kinds:?}");
        assert!(kinds.contains(&"too-long".to_string()), "{kinds:?}");
        assert!(kinds.contains(&"draft-missing-sessions".to_string()), "{kinds:?}");
    }

    #[test]
    fn backup_is_a_complete_openable_copy() {
        let (mut conn, dir) = temp_database();
        seed(&mut conn);
        let target = dir.path().join("backup.sqlite3");
        backup_to(&conn, &target).unwrap();

        let copy = Connection::open(&target).unwrap();
        let sessions: i64 = copy.query_row("SELECT COUNT(*) FROM work_sessions", [], |r| r.get(0)).unwrap();
        assert_eq!(sessions, 2);
        sqlite_quick_check(&copy).unwrap();
    }

    #[test]
    fn pre_migration_backup_only_when_schema_is_behind() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("devlog.sqlite3");
        let backups = dir.path().join("backups");

        assert_eq!(backup_before_migrations(&db_path, &backups).unwrap(), None, "нет файла — нечего копировать");
        drop(create_database(&db_path).unwrap());
        assert_eq!(backup_before_migrations(&db_path, &backups).unwrap(), None, "схема актуальна");

        // Имитируем БД старой версии приложения.
        Connection::open(&db_path).unwrap().execute("DELETE FROM _migrations WHERE version = ?1", [latest_schema_version()]).unwrap();
        let made = backup_before_migrations(&db_path, &backups).unwrap().expect("должна появиться копия");
        assert!(made.exists());
    }

    #[test]
    fn prune_keeps_only_newest_backups() {
        let dir = tempfile::TempDir::new().unwrap();
        for stamp in ["20260101", "20260102", "20260103", "20260104"] {
            std::fs::write(dir.path().join(format!("devlog-schema1-{stamp}.sqlite3")), b"x").unwrap();
        }
        std::fs::write(dir.path().join("unrelated.txt"), b"x").unwrap();
        prune_backups(dir.path(), 3);
        assert!(!dir.path().join("devlog-schema1-20260101.sqlite3").exists());
        assert!(dir.path().join("devlog-schema1-20260104.sqlite3").exists());
        assert!(dir.path().join("unrelated.txt").exists());
    }

    #[test]
    fn json_export_contains_all_tables_and_rows() {
        let (mut conn, _dir) = temp_database();
        seed(&mut conn);
        let doc: Value = serde_json::from_str(&export_json(&conn).unwrap()).unwrap();
        assert_eq!(doc["format"], "devlog-export");
        assert_eq!(doc["tables"]["work_sessions"].as_array().unwrap().len(), 2);
        assert_eq!(doc["tables"]["work_sessions"][0]["issueKey"], "OB-448");
        for table in EXPORT_TABLES {
            assert!(doc["tables"][table].is_array(), "{table}");
        }
    }

    #[test]
    fn csv_escapes_and_guards_formulas() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");

        let (mut conn, _dir) = temp_database();
        seed(&mut conn);
        let csv = export_sessions_csv(&conn).unwrap();
        assert_eq!(csv.lines().count(), 3);
        assert!(csv.contains("OB-448,5400,detected"), "{csv}");
    }

    #[test]
    fn delete_history_before_keeps_newer_data() {
        let (mut conn, _dir) = temp_database();
        seed(&mut conn);
        // Граница между двумя сессиями фикстуры (10:30 и 11:00).
        let summary = delete_data(&mut conn, DeleteScope::HistoryBefore(DAY + 10 * 3600 + 2700)).unwrap();
        assert_eq!(summary.work_sessions, 1);
        assert!(summary.activity_events > 0);
        let left = work_sessions::list_by_range(&conn, i64::MIN, i64::MAX, true).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].issue_key.as_deref(), Some("OB-419"));
    }

    #[test]
    fn delete_all_history_keeps_settings_everything_removes_them() {
        let (mut conn, _dir) = temp_database();
        seed(&mut conn);
        settings::set(&conn, "k", "v").unwrap();

        delete_data(&mut conn, DeleteScope::AllHistory).unwrap();
        assert!(work_sessions::list_by_range(&conn, i64::MIN, i64::MAX, true).unwrap().is_empty());
        assert_eq!(settings::get(&conn, "k").unwrap().as_deref(), Some("v"));

        delete_data(&mut conn, DeleteScope::Everything).unwrap();
        assert_eq!(settings::get(&conn, "k").unwrap(), None);
    }

    #[test]
    fn deleted_text_is_physically_gone_from_the_file() {
        // Приватность: после удаления описание сессии не должно оставаться
        // в свободных страницах файла БД.
        let (mut conn, dir) = temp_database();
        let mut s = session(DAY, DAY + 60, 60, None);
        s.description = Some("SECRET-MARKER-7f3a".into());
        work_sessions::create(&conn, &s).unwrap();
        delete_data(&mut conn, DeleteScope::AllHistory).unwrap();
        drop(conn);

        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            assert!(!bytes.windows(18).any(|w| w == b"SECRET-MARKER-7f3a"));
        }
    }
}
