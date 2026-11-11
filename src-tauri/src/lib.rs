pub mod application;
pub mod config;
pub mod diagnostic_log;
pub mod domain;
pub mod infrastructure;
pub mod ipc;
pub mod state;

use std::sync::Arc;

use application::InventoryRepository;
use config::AppConfig;
use diagnostic_log::DiagnosticLog;
use infrastructure::sqlite::{SqliteDatabase, SqliteInventoryRepository};
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
        ])
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
            let inventory_repository: Arc<dyn InventoryRepository> =
                Arc::new(SqliteInventoryRepository::new(&database));

            app.manage(AppState {
                config,
                inventory_repository,
                diagnostic_log,
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("No se pudo iniciar la ventana de Ariadna");
}
