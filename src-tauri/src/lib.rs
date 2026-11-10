pub mod application;
pub mod config;
pub mod diagnostic_log;
pub mod domain;
pub mod infrastructure;
pub mod state;

use std::sync::Arc;

use config::AppConfig;
use diagnostic_log::DiagnosticLog;
use infrastructure::sqlite::SqliteDatabase;
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config = AppConfig::from_data_directory(app.path().app_data_dir()?)?;
            let diagnostic_log = Arc::new(DiagnosticLog::open(config.log_path())?);

            let database = match tauri::async_runtime::block_on(SqliteDatabase::open(
                config.database_path(),
            )) {
                Ok(database) => database,
                Err(error) => {
                    let _ = diagnostic_log.record("startup.database", &error);
                    return Err(Box::new(error));
                }
            };

            app.manage(AppState {
                config,
                database,
                diagnostic_log,
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("No se pudo iniciar la ventana de Ariadna");
}
