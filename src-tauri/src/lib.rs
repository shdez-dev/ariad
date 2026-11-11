pub mod application;
pub mod config;
pub mod diagnostic_log;
pub mod domain;
pub mod infrastructure;
pub mod ipc;
pub mod state;

use std::{sync::Arc, time::Instant};

use application::InventoryRepository;
use config::AppConfig;
use diagnostic_log::{DiagnosticLog, LogEvent, LogLevel};
use infrastructure::sqlite::{SqliteDatabase, SqliteInventoryRepository};
use state::AppState;
use tauri::Manager;

fn log_startup_event(
    diagnostic_log: &DiagnosticLog,
    operation_id: &str,
    duration_ms: u128,
    error_code: Option<&str>,
) -> std::io::Result<()> {
    let (level, command, result) = match error_code {
        Some(_) => (LogLevel::Error, "app.startup.database", "error"),
        None => (LogLevel::Info, "app.startup", "success"),
    };
    diagnostic_log.record_event(LogEvent {
        level,
        operation_id,
        command,
        result,
        duration_ms: Some(duration_ms),
        error_code,
        detail: None,
    })
}

fn log_shutdown_event(diagnostic_log: &DiagnosticLog) -> std::io::Result<()> {
    let operation_id = DiagnosticLog::next_operation_id();
    diagnostic_log.record_event(LogEvent {
        level: LogLevel::Info,
        operation_id: &operation_id,
        command: "app.shutdown",
        result: "success",
        duration_ms: None,
        error_code: None,
        detail: None,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ipc::create_product,
            ipc::get_product,
            ipc::list_products,
            ipc::search_inventory,
            ipc::update_product,
            ipc::deactivate_product,
            ipc::create_location,
            ipc::get_location,
            ipc::list_locations,
            ipc::update_location,
            ipc::deactivate_location,
            ipc::get_handling_unit,
            ipc::list_handling_units,
            ipc::receive_unit,
            ipc::store_unit,
            ipc::move_unit,
            ipc::dispatch_unit,
            ipc::record_movement_correction,
            ipc::get_movement_history,
            ipc::list_alerts,
            ipc::mark_alert_reviewed,
            ipc::run_simulation,
        ])
        .setup(|app| {
            let config = AppConfig::from_data_directory(app.path().app_data_dir()?)?;
            let diagnostic_log = Arc::new(DiagnosticLog::open(config.log_path())?);
            let startup_operation_id = DiagnosticLog::next_operation_id();
            let startup_started_at = Instant::now();

            let database = match tauri::async_runtime::block_on(SqliteDatabase::open(
                config.database_path(),
            )) {
                Ok(database) => database,
                Err(error) => {
                    let _ = log_startup_event(
                        &diagnostic_log,
                        &startup_operation_id,
                        startup_started_at.elapsed().as_millis(),
                        Some(error.diagnostic_code()),
                    );
                    return Err(Box::new(error));
                }
            };
            let inventory_repository: Arc<dyn InventoryRepository> =
                Arc::new(SqliteInventoryRepository::new(&database));

            app.manage(AppState {
                config,
                inventory_repository,
                diagnostic_log,
            });
            let state = app.state::<AppState>();
            let _ = log_startup_event(
                &state.diagnostic_log,
                &startup_operation_id,
                startup_started_at.elapsed().as_millis(),
                None,
            );
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("No se pudo configurar la ventana de Ariadna");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event
            && let Some(state) = app_handle.try_state::<AppState>()
        {
            let _ = log_shutdown_event(&state.diagnostic_log);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{log_shutdown_event, log_startup_event};
    use crate::diagnostic_log::DiagnosticLog;
    use serde_json::Value;
    use std::{fs, time::SystemTime};

    #[test]
    fn records_startup_failure_success_and_shutdown_without_details() {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-lifecycle-{unique}"));
        let path = directory.join("logs").join("ariadna.log");
        let log = DiagnosticLog::open(&path).unwrap();

        log_startup_event(
            &log,
            "op-start-failure",
            11,
            Some("database.migration_failed"),
        )
        .unwrap();
        log_startup_event(&log, "op-start-success", 14, None).unwrap();
        log_shutdown_event(&log).unwrap();

        let entries: Vec<Value> = fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0]["command"], "app.startup.database");
        assert_eq!(entries[0]["result"], "error");
        assert_eq!(entries[0]["errorCode"], "database.migration_failed");
        assert_eq!(entries[1]["command"], "app.startup");
        assert_eq!(entries[1]["result"], "success");
        assert_eq!(entries[2]["command"], "app.shutdown");
        assert_eq!(entries[2]["result"], "success");
        assert!(entries.iter().all(|entry| entry["detail"].is_null()));

        drop(log);
        fs::remove_dir_all(directory).unwrap();
    }
}
