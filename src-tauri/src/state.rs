use std::sync::Arc;

use crate::{
    config::AppConfig, diagnostic_log::DiagnosticLog, infrastructure::sqlite::SqliteDatabase,
};

pub struct AppState {
    pub config: AppConfig,
    pub database: SqliteDatabase,
    pub diagnostic_log: Arc<DiagnosticLog>,
}
