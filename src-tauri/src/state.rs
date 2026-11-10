use std::sync::Arc;

use crate::{application::InventoryRepository, config::AppConfig, diagnostic_log::DiagnosticLog};

pub struct AppState {
    pub config: AppConfig,
    pub inventory_repository: Arc<dyn InventoryRepository>,
    pub diagnostic_log: Arc<DiagnosticLog>,
}
