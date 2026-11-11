use std::{path::Path, time::Duration};

use async_trait::async_trait;
use sqlx::{
    Row, Sqlite, SqlitePool, Transaction,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
};
use thiserror::Error;

use crate::{
    application::{
        AlertEntry, EventOutcome, InventoryRepository, MovementHistoryEntry, MovementRecordOutcome,
        MovementRecordResult, RepositoryError,
    },
    domain::{
        Alert, AlertIntent, AlertRecordTarget, AlertStatus, HandlingUnit, HandlingUnitLine,
        HandlingUnitStatus, Location, LocationKind, MovementCorrection, MovementEvent,
        MovementKind, Product, Sku,
    },
};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct SqliteDatabase {
    pool: SqlitePool,
}

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("No se pudo abrir SQLite: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("No se pudieron aplicar las migraciones: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("No se pudo crear la carpeta local de datos: {0}")]
    Io(#[from] std::io::Error),
}

impl PersistenceError {
    pub fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::Sqlx(_) => "database.open_failed",
            Self::Migration(_) => "database.migration_failed",
            Self::Io(_) => "filesystem.initialization_failed",
        }
    }
}

impl SqliteDatabase {
    pub async fn open(path: &Path) -> Result<Self, PersistenceError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        Self::connect_with_options(options, 5).await
    }

    async fn connect_with_options(
        options: SqliteConnectOptions,
        max_connections: u32,
    ) -> Result<Self, PersistenceError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    #[cfg(test)]
    async fn in_memory() -> Result<Self, PersistenceError> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true);
        Self::connect_with_options(options, 1).await
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

#[derive(Clone)]
pub struct SqliteInventoryRepository {
    pool: SqlitePool,
}

impl SqliteInventoryRepository {
    pub fn new(database: &SqliteDatabase) -> Self {
        Self {
            pool: database.pool().clone(),
        }
    }

    async fn query_handling_units(
        &self,
        id_or_label: Option<&str>,
        include_dispatched: bool,
    ) -> Result<Vec<HandlingUnit>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT unit.id, unit.label, unit.status, unit.current_location_id, line.product_sku, line.lot_code, line.quantity FROM handling_units AS unit LEFT JOIN handling_unit_lines AS line ON line.handling_unit_id = unit.id WHERE (? IS NULL OR unit.id = ? OR unit.label = ? COLLATE NOCASE) AND (? = 1 OR unit.status <> 'dispatched') ORDER BY CASE WHEN unit.id = ? THEN 0 ELSE 1 END, unit.label COLLATE NOCASE, unit.id, line.product_sku COLLATE NOCASE, line.lot_code",
        )
        .bind(id_or_label)
        .bind(id_or_label)
        .bind(id_or_label)
        .bind(include_dispatched)
        .bind(id_or_label)
        .fetch_all(&self.pool)
        .await
        .map_err(map_repository_error)?;

        let mut units: Vec<HandlingUnit> = Vec::new();
        for row in rows {
            let id: String = row
                .try_get("id")
                .map_err(|error| RepositoryError::Storage(error.to_string()))?;
            if units.last().map(|unit| unit.id.as_str()) != Some(id.as_str()) {
                let status = match row
                    .try_get::<String, _>("status")
                    .map_err(|error| RepositoryError::Storage(error.to_string()))?
                    .as_str()
                {
                    "received" => HandlingUnitStatus::Received,
                    "stored" => HandlingUnitStatus::Stored,
                    "dispatched" => HandlingUnitStatus::Dispatched,
                    other => {
                        return Err(RepositoryError::Storage(format!(
                            "estado de unidad desconocido: {other}"
                        )));
                    }
                };
                units.push(HandlingUnit {
                    id,
                    label: row
                        .try_get("label")
                        .map_err(|error| RepositoryError::Storage(error.to_string()))?,
                    status,
                    current_location_id: row
                        .try_get("current_location_id")
                        .map_err(|error| RepositoryError::Storage(error.to_string()))?,
                    lines: Vec::new(),
                });
            }

            if let Some(sku) = row
                .try_get::<Option<String>, _>("product_sku")
                .map_err(|error| RepositoryError::Storage(error.to_string()))?
            {
                let quantity = row
                    .try_get::<i64, _>("quantity")
                    .map_err(|error| RepositoryError::Storage(error.to_string()))?;
                let quantity = u32::try_from(quantity).map_err(|_| {
                    RepositoryError::Storage("cantidad de línea fuera de rango".to_owned())
                })?;
                let lot_code: String = row
                    .try_get("lot_code")
                    .map_err(|error| RepositoryError::Storage(error.to_string()))?;
                let line = HandlingUnitLine {
                    sku: Sku::new(sku)
                        .map_err(|error| RepositoryError::Storage(error.to_string()))?,
                    quantity,
                    lot_code: (!lot_code.is_empty()).then_some(lot_code),
                };
                units
                    .last_mut()
                    .expect("la unidad se creó para la fila")
                    .lines
                    .push(line);
            }
        }

        if units.iter().any(|unit| unit.lines.is_empty()) {
            return Err(RepositoryError::Storage(
                "unidad persistida sin líneas de producto".to_owned(),
            ));
        }
        Ok(units)
    }

    async fn query_alert_entries(
        &self,
        alert_id: Option<&str>,
    ) -> Result<Vec<AlertEntry>, RepositoryError> {
        let rows = sqlx::query_as::<_, AlertRow>(
            "SELECT alert.id AS alert_id, alert.movement_event_id, alert.movement_attempt_id, alert.kind AS alert_kind, alert.status AS alert_status, alert.created_at_ms, COALESCE(event.event_id, attempt.event_id) AS event_id, COALESCE(event.handling_unit_id, attempt.handling_unit_id) AS handling_unit_id, unit.label AS handling_unit_label, COALESCE(event.kind, attempt.kind) AS event_kind, COALESCE(event.result, attempt.result) AS event_result, attempt.rejection_reason AS event_rejection_reason, COALESCE(event.source, attempt.source) AS source, COALESCE(event.occurred_at_ms, attempt.occurred_at_ms) AS occurred_at_ms, origin.code AS origin_location_code, destination.code AS destination_location_code FROM alerts AS alert LEFT JOIN movement_events AS event ON event.event_id = alert.movement_event_id LEFT JOIN movement_attempts AS attempt ON attempt.attempt_id = alert.movement_attempt_id LEFT JOIN handling_units AS unit ON unit.id = COALESCE(event.handling_unit_id, attempt.handling_unit_id) LEFT JOIN locations AS origin ON origin.id = COALESCE(event.origin_location_id, attempt.origin_location_id) LEFT JOIN locations AS destination ON destination.id = COALESCE(event.destination_location_id, attempt.destination_location_id) WHERE (? IS NULL OR alert.id = ?) ORDER BY CASE alert.status WHEN 'open' THEN 0 WHEN 'reviewed' THEN 1 ELSE 2 END, alert.created_at_ms DESC, alert.id",
        )
        .bind(alert_id)
        .bind(alert_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_repository_error)?;

        rows.into_iter().map(AlertEntry::try_from).collect()
    }

    async fn record_movement_transactional(
        &self,
        event: &MovementEvent,
        alert_intent: Option<&AlertIntent>,
    ) -> Result<MovementRecordOutcome, RepositoryError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(map_repository_error)?;

        if let Some(previous_event) =
            event_payload_for_id(&mut transaction, &event.event_id).await?
        {
            if movement_payload_matches(&previous_event, event) {
                let outcome = record_attempt_and_alert(
                    &mut transaction,
                    event,
                    "duplicate",
                    None,
                    alert_intent,
                )
                .await?;
                transaction.commit().await.map_err(map_repository_error)?;
                return Ok(outcome);
            }
            let outcome = record_attempt_and_alert(
                &mut transaction,
                event,
                "rejected",
                Some("event_id_conflict"),
                alert_intent,
            )
            .await?;
            transaction.commit().await.map_err(map_repository_error)?;
            return Ok(outcome);
        }

        if movement_attempt_exists(&mut transaction, &event.event_id).await? {
            let outcome = record_attempt_and_alert(
                &mut transaction,
                event,
                "rejected",
                Some("event_id_previously_rejected"),
                alert_intent,
            )
            .await?;
            transaction.commit().await.map_err(map_repository_error)?;
            return Ok(outcome);
        }

        let current = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT status, current_location_id FROM handling_units WHERE id = ?",
        )
        .bind(&event.handling_unit_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_repository_error)?;
        let Some(current) = current else {
            let outcome = record_attempt_and_alert(
                &mut transaction,
                event,
                "rejected",
                Some("handling_unit_not_found"),
                alert_intent,
            )
            .await?;
            transaction.commit().await.map_err(map_repository_error)?;
            return Ok(outcome);
        };

        let current_status = match current.0.as_str() {
            "received" => HandlingUnitStatus::Received,
            "stored" => HandlingUnitStatus::Stored,
            "dispatched" => HandlingUnitStatus::Dispatched,
            _ => {
                return Err(RepositoryError::Storage(
                    "estado de unidad desconocido".into(),
                ));
            }
        };
        let (new_status, new_location_id) =
            match transition_target(event, current_status, current.1.as_deref()) {
                Ok(target) => target,
                Err(error) => {
                    let outcome = record_attempt_and_alert(
                        &mut transaction,
                        event,
                        "rejected",
                        Some("invalid_transition"),
                        alert_intent,
                    )
                    .await?;
                    transaction.commit().await.map_err(map_repository_error)?;
                    return Ok(outcome_with_error(outcome, error));
                }
            };

        let update_result = sqlx::query(
            "UPDATE handling_units SET status = ?, current_location_id = ?, updated_at_ms = ? WHERE id = ? AND status = ? AND current_location_id IS ?",
        )
        .bind(new_status.as_str())
        .bind(new_location_id)
        .bind(event.occurred_at_ms)
        .bind(&event.handling_unit_id)
        .bind(current.0)
        .bind(current.1)
        .execute(&mut *transaction)
        .await;

        let update = match update_result {
            Ok(update) => update,
            Err(error) => {
                let repository_error = map_repository_error(error);
                if matches!(
                    &repository_error,
                    RepositoryError::PositionOccupied
                        | RepositoryError::InvalidDestination
                        | RepositoryError::NotFound
                ) {
                    let outcome = record_attempt_and_alert(
                        &mut transaction,
                        event,
                        "rejected",
                        Some(event_attempt_reason(&repository_error)),
                        alert_intent,
                    )
                    .await?;
                    transaction.commit().await.map_err(map_repository_error)?;
                    return Ok(outcome_with_error(outcome, repository_error));
                }
                let _ = transaction.rollback().await;
                return Err(repository_error);
            }
        };

        if update.rows_affected() != 1 {
            let error = RepositoryError::InvalidTransition;
            let outcome = record_attempt_and_alert(
                &mut transaction,
                event,
                "rejected",
                Some("invalid_transition"),
                alert_intent,
            )
            .await?;
            transaction.commit().await.map_err(map_repository_error)?;
            return Ok(outcome_with_error(outcome, error));
        }

        let insert_result = sqlx::query(
            "INSERT INTO movement_events (event_id, handling_unit_id, kind, origin_location_id, destination_location_id, occurred_at_ms, source, result, created_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, 'accepted', ?)",
        )
        .bind(&event.event_id)
        .bind(&event.handling_unit_id)
        .bind(event.kind.as_str())
        .bind(&event.origin_location_id)
        .bind(&event.destination_location_id)
        .bind(event.occurred_at_ms)
        .bind(&event.source)
        .bind(now_ms())
        .execute(&mut *transaction)
        .await;

        if let Err(error) = insert_result {
            let _ = transaction.rollback().await;
            return Err(map_repository_error(error));
        }

        let alert_id = match alert_intent {
            Some(intent) => Some(
                insert_alert_for_target(
                    &mut transaction,
                    intent,
                    AlertRecordTarget::MovementEvent {
                        event_id: event.event_id.clone(),
                    },
                )
                .await?,
            ),
            None => None,
        };
        transaction.commit().await.map_err(map_repository_error)?;
        Ok(MovementRecordOutcome {
            result: MovementRecordResult::Applied,
            record_id: event.event_id.clone(),
            alert_id,
        })
    }
}

#[async_trait]
impl InventoryRepository for SqliteInventoryRepository {
    async fn insert_product(&self, product: &Product) -> Result<(), RepositoryError> {
        let now_ms = now_ms();
        sqlx::query(
            "INSERT INTO products (sku, description, unit_of_measure, is_active, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(product.sku.as_str())
        .bind(&product.description)
        .bind(&product.unit_of_measure)
        .bind(i64::from(product.active))
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(map_repository_error)?;
        Ok(())
    }

    async fn find_product(&self, sku: &Sku) -> Result<Option<Product>, RepositoryError> {
        let row = sqlx::query(
            "SELECT sku, description, unit_of_measure, is_active FROM products WHERE sku = ?",
        )
        .bind(sku.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_repository_error)?;
        row.map(product_from_row).transpose()
    }

    async fn list_products(&self, include_inactive: bool) -> Result<Vec<Product>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT sku, description, unit_of_measure, is_active FROM products WHERE (? = 1 OR is_active = 1) ORDER BY sku COLLATE NOCASE",
        )
        .bind(include_inactive)
        .fetch_all(&self.pool)
        .await
        .map_err(map_repository_error)?;
        rows.into_iter().map(product_from_row).collect()
    }

    async fn update_product(&self, product: &Product) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE products SET description = ?, unit_of_measure = ?, updated_at_ms = ? WHERE sku = ?",
        )
        .bind(&product.description)
        .bind(&product.unit_of_measure)
        .bind(now_ms())
        .bind(product.sku.as_str())
        .execute(&self.pool)
        .await
        .map_err(map_repository_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn deactivate_product(&self, sku: &Sku) -> Result<(), RepositoryError> {
        let result =
            sqlx::query("UPDATE products SET is_active = 0, updated_at_ms = ? WHERE sku = ?")
                .bind(now_ms())
                .bind(sku.as_str())
                .execute(&self.pool)
                .await
                .map_err(map_repository_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn insert_location(&self, location: &Location) -> Result<(), RepositoryError> {
        let now = now_ms();
        sqlx::query(
            "INSERT INTO locations (id, code, name, kind, parent_id, is_active, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&location.id)
        .bind(&location.code)
        .bind(&location.name)
        .bind(location.kind.as_str())
        .bind(&location.parent_id)
        .bind(location.active)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(map_repository_error)?;
        Ok(())
    }

    async fn find_location(&self, id: &str) -> Result<Option<Location>, RepositoryError> {
        let row = sqlx::query(
            "SELECT id, code, name, kind, parent_id, is_active FROM locations WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_repository_error)?;
        row.map(location_from_row).transpose()
    }

    async fn list_locations(
        &self,
        include_inactive: bool,
    ) -> Result<Vec<Location>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, code, name, kind, parent_id, is_active FROM locations WHERE (? = 1 OR is_active = 1) ORDER BY CASE kind WHEN 'zone' THEN 0 WHEN 'aisle' THEN 1 WHEN 'rack' THEN 2 ELSE 3 END, parent_id, code COLLATE NOCASE",
        )
        .bind(include_inactive)
        .fetch_all(&self.pool)
        .await
        .map_err(map_repository_error)?;
        rows.into_iter().map(location_from_row).collect()
    }

    async fn update_location(&self, location: &Location) -> Result<(), RepositoryError> {
        let result =
            sqlx::query("UPDATE locations SET code = ?, name = ?, updated_at_ms = ? WHERE id = ?")
                .bind(&location.code)
                .bind(&location.name)
                .bind(now_ms())
                .bind(&location.id)
                .execute(&self.pool)
                .await
                .map_err(map_repository_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn deactivate_location(&self, id: &str) -> Result<(), RepositoryError> {
        let result =
            sqlx::query("UPDATE locations SET is_active = 0, updated_at_ms = ? WHERE id = ?")
                .bind(now_ms())
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(map_repository_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn find_handling_unit(
        &self,
        id_or_label: &str,
    ) -> Result<Option<HandlingUnit>, RepositoryError> {
        Ok(self
            .query_handling_units(Some(id_or_label), true)
            .await?
            .into_iter()
            .next())
    }

    async fn list_handling_units(
        &self,
        include_dispatched: bool,
    ) -> Result<Vec<HandlingUnit>, RepositoryError> {
        self.query_handling_units(None, include_dispatched).await
    }

    async fn receive_unit(
        &self,
        unit: &HandlingUnit,
        event: &MovementEvent,
    ) -> Result<EventOutcome, RepositoryError> {
        if unit.status != HandlingUnitStatus::Received
            || unit.current_location_id.is_some()
            || unit.lines.is_empty()
            || event.kind != MovementKind::Received
            || event.handling_unit_id != unit.id
            || event.origin_location_id.is_some()
            || event.destination_location_id.is_some()
        {
            return Err(RepositoryError::InvalidTransition);
        }

        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(map_repository_error)?;

        if let Some(previous_event) =
            event_payload_for_id(&mut transaction, &event.event_id).await?
        {
            let previous_label =
                sqlx::query_scalar::<_, String>("SELECT label FROM handling_units WHERE id = ?")
                    .bind(&unit.id)
                    .fetch_optional(&mut *transaction)
                    .await
                    .map_err(map_repository_error)?;
            let previous_lines = sqlx::query_as::<_, (String, String, i64)>(
                "SELECT product_sku, lot_code, quantity FROM handling_unit_lines WHERE handling_unit_id = ? ORDER BY product_sku, lot_code",
            )
            .bind(&unit.id)
            .fetch_all(&mut *transaction)
            .await
            .map_err(map_repository_error)?;
            let mut expected_lines: Vec<(String, String, i64)> = unit
                .lines
                .iter()
                .map(|line| {
                    (
                        line.sku.as_str().to_owned(),
                        line.lot_code.clone().unwrap_or_default(),
                        i64::from(line.quantity),
                    )
                })
                .collect();
            expected_lines.sort();
            let payload_matches = movement_payload_matches(&previous_event, event)
                && previous_label.as_deref() == Some(unit.label.as_str())
                && previous_lines == expected_lines;
            if payload_matches {
                record_event_attempt(&mut transaction, event, "duplicate", None).await?;
                transaction.commit().await.map_err(map_repository_error)?;
                return Ok(EventOutcome::Duplicate);
            }
            record_event_attempt(
                &mut transaction,
                event,
                "rejected",
                Some("event_id_conflict"),
            )
            .await?;
            transaction.commit().await.map_err(map_repository_error)?;
            return Err(RepositoryError::EventIdConflict);
        }

        if movement_attempt_exists(&mut transaction, &event.event_id).await? {
            record_event_attempt(
                &mut transaction,
                event,
                "rejected",
                Some("event_id_previously_rejected"),
            )
            .await?;
            transaction.commit().await.map_err(map_repository_error)?;
            return Err(RepositoryError::EventIdConflict);
        }

        for line in &unit.lines {
            let product_active =
                sqlx::query_scalar::<_, i64>("SELECT is_active FROM products WHERE sku = ?")
                    .bind(line.sku.as_str())
                    .fetch_optional(&mut *transaction)
                    .await
                    .map_err(map_repository_error)?;
            match product_active {
                None => {
                    record_event_attempt(
                        &mut transaction,
                        event,
                        "rejected",
                        Some("product_not_found"),
                    )
                    .await?;
                    transaction.commit().await.map_err(map_repository_error)?;
                    return Err(RepositoryError::NotFound);
                }
                Some(0) => {
                    record_event_attempt(
                        &mut transaction,
                        event,
                        "rejected",
                        Some("product_inactive"),
                    )
                    .await?;
                    transaction.commit().await.map_err(map_repository_error)?;
                    return Err(RepositoryError::InactiveProduct);
                }
                Some(_) => {}
            }
        }

        let result = async {
            sqlx::query(
                "INSERT INTO handling_units (id, label, status, current_location_id, created_at_ms, updated_at_ms) VALUES (?, ?, 'received', NULL, ?, ?)",
            )
            .bind(&unit.id)
            .bind(&unit.label)
            .bind(event.occurred_at_ms)
            .bind(event.occurred_at_ms)
            .execute(&mut *transaction)
            .await
            .map_err(map_repository_error)?;

            for line in &unit.lines {
                sqlx::query(
                    "INSERT INTO handling_unit_lines (handling_unit_id, product_sku, lot_code, quantity) VALUES (?, ?, ?, ?)",
                )
                .bind(&unit.id)
                .bind(line.sku.as_str())
                .bind(line.lot_code.as_deref().unwrap_or(""))
                .bind(i64::from(line.quantity))
                .execute(&mut *transaction)
                .await
                .map_err(map_repository_error)?;
            }

            sqlx::query(
                "INSERT INTO movement_events (event_id, handling_unit_id, kind, origin_location_id, destination_location_id, occurred_at_ms, source, result, created_at_ms) VALUES (?, ?, 'received', NULL, NULL, ?, ?, 'accepted', ?)",
            )
            .bind(&event.event_id)
            .bind(&unit.id)
            .bind(event.occurred_at_ms)
            .bind(&event.source)
            .bind(now_ms())
            .execute(&mut *transaction)
            .await
            .map_err(map_repository_error)?;

            Ok::<(), RepositoryError>(())
        }
        .await;

        if let Err(error) = result {
            let _ = transaction.rollback().await;
            if matches!(
                &error,
                RepositoryError::Conflict | RepositoryError::EventIdConflict
            ) {
                let mut attempt_transaction = self
                    .pool
                    .begin_with("BEGIN IMMEDIATE")
                    .await
                    .map_err(map_repository_error)?;
                record_event_attempt(
                    &mut attempt_transaction,
                    event,
                    "rejected",
                    Some(event_attempt_reason(&error)),
                )
                .await?;
                attempt_transaction
                    .commit()
                    .await
                    .map_err(map_repository_error)?;
            }
            return Err(error);
        }
        transaction
            .commit()
            .await
            .map(|()| EventOutcome::Applied)
            .map_err(map_repository_error)
    }

    async fn record_movement(
        &self,
        event: &MovementEvent,
    ) -> Result<EventOutcome, RepositoryError> {
        let outcome = self.record_movement_transactional(event, None).await?;
        match outcome.result {
            MovementRecordResult::Applied => Ok(EventOutcome::Applied),
            MovementRecordResult::Duplicate => Ok(EventOutcome::Duplicate),
            MovementRecordResult::Rejected(error) => Err(error),
        }
    }

    async fn record_movement_with_alert(
        &self,
        event: &MovementEvent,
        intent: &AlertIntent,
    ) -> Result<MovementRecordOutcome, RepositoryError> {
        self.record_movement_transactional(event, Some(intent))
            .await
    }

    async fn record_correction(
        &self,
        correction: &MovementCorrection,
    ) -> Result<EventOutcome, RepositoryError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(map_repository_error)?;

        let existing = sqlx::query_as::<_, (String, String, String, i64, String)>(
            "SELECT handling_unit_id, corrects_event_id, reason, occurred_at_ms, source FROM movement_corrections WHERE correction_id = ?",
        )
        .bind(&correction.correction_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_repository_error)?;
        if let Some(existing) = existing {
            let matches = existing.0 == correction.handling_unit_id
                && existing.1 == correction.corrects_event_id
                && existing.2 == correction.reason
                && existing.3 == correction.occurred_at_ms
                && existing.4 == correction.source;
            if matches {
                transaction.commit().await.map_err(map_repository_error)?;
                return Ok(EventOutcome::Duplicate);
            }
            let _ = transaction.rollback().await;
            return Err(RepositoryError::EventIdConflict);
        }

        let unit_exists: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM handling_units WHERE id = ?)")
                .bind(&correction.handling_unit_id)
                .fetch_one(&mut *transaction)
                .await
                .map_err(map_repository_error)?;
        if unit_exists == 0 {
            let _ = transaction.rollback().await;
            return Err(RepositoryError::NotFound);
        }
        let (original_unit, original_occurred_at_ms) = sqlx::query_as::<_, (String, i64)>(
            "SELECT handling_unit_id, occurred_at_ms FROM movement_events WHERE event_id = ?",
        )
        .bind(&correction.corrects_event_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_repository_error)?
        .ok_or(RepositoryError::NotFound)?;
        if original_unit != correction.handling_unit_id {
            let _ = transaction.rollback().await;
            return Err(RepositoryError::CorrectionReferenceMismatch);
        }
        if correction.occurred_at_ms < original_occurred_at_ms {
            let _ = transaction.rollback().await;
            return Err(RepositoryError::CorrectionBeforeOriginal);
        }

        sqlx::query(
            "INSERT INTO movement_corrections (correction_id, handling_unit_id, corrects_event_id, reason, occurred_at_ms, source, created_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&correction.correction_id)
        .bind(&correction.handling_unit_id)
        .bind(&correction.corrects_event_id)
        .bind(&correction.reason)
        .bind(correction.occurred_at_ms)
        .bind(&correction.source)
        .bind(now_ms())
        .execute(&mut *transaction)
        .await
        .map_err(map_repository_error)?;

        transaction
            .commit()
            .await
            .map(|()| EventOutcome::Applied)
            .map_err(map_repository_error)
    }

    async fn list_movement_history(
        &self,
        handling_unit_id: &str,
    ) -> Result<Vec<MovementHistoryEntry>, RepositoryError> {
        let history_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM handling_units WHERE id = ?) OR EXISTS(SELECT 1 FROM movement_attempts WHERE handling_unit_id = ?) OR EXISTS(SELECT 1 FROM movement_corrections WHERE handling_unit_id = ?)",
        )
        .bind(handling_unit_id)
        .bind(handling_unit_id)
        .bind(handling_unit_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_repository_error)?;
        if history_exists == 0 {
            return Err(RepositoryError::NotFound);
        }

        let rows = sqlx::query_as::<_, MovementHistoryRow>(
            "SELECT event_id AS record_id, event_id, handling_unit_id, kind, origin_location_id, destination_location_id, occurred_at_ms, created_at_ms AS recorded_at_ms, source, result, rejection_reason, NULL AS corrects_event_id, NULL AS correction_reason, 0 AS record_order FROM movement_events WHERE handling_unit_id = ? UNION ALL SELECT 'attempt:' || attempt_id AS record_id, event_id, handling_unit_id, kind, origin_location_id, destination_location_id, occurred_at_ms, created_at_ms AS recorded_at_ms, source, result, rejection_reason, NULL AS corrects_event_id, NULL AS correction_reason, 1 AS record_order FROM movement_attempts WHERE handling_unit_id = ? UNION ALL SELECT correction_id AS record_id, correction_id AS event_id, handling_unit_id, 'correction' AS kind, NULL AS origin_location_id, NULL AS destination_location_id, occurred_at_ms, created_at_ms AS recorded_at_ms, source, 'accepted' AS result, NULL AS rejection_reason, corrects_event_id, reason AS correction_reason, 2 AS record_order FROM movement_corrections WHERE handling_unit_id = ? ORDER BY occurred_at_ms ASC, recorded_at_ms ASC, record_order ASC, record_id ASC",
        )
        .bind(handling_unit_id)
        .bind(handling_unit_id)
        .bind(handling_unit_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_repository_error)?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_alerts(&self) -> Result<Vec<AlertEntry>, RepositoryError> {
        self.query_alert_entries(None).await
    }

    async fn mark_alert_reviewed(&self, alert_id: &str) -> Result<AlertEntry, RepositoryError> {
        let updated =
            sqlx::query("UPDATE alerts SET status = 'reviewed' WHERE id = ? AND status = 'open'")
                .bind(alert_id)
                .execute(&self.pool)
                .await
                .map_err(map_repository_error)?;

        let entry = self
            .query_alert_entries(Some(alert_id))
            .await?
            .into_iter()
            .next()
            .ok_or(RepositoryError::NotFound)?;

        if updated.rows_affected() == 0 {
            match entry.alert.status {
                AlertStatus::Reviewed => return Ok(entry),
                AlertStatus::Resolved => return Err(RepositoryError::InvalidAlertTransition),
                AlertStatus::Open => {
                    return Err(RepositoryError::Storage(
                        "la alerta no pudo actualizarse".to_owned(),
                    ));
                }
            }
        }

        Ok(entry)
    }
}

#[derive(sqlx::FromRow)]
struct AlertRow {
    alert_id: String,
    movement_event_id: Option<String>,
    movement_attempt_id: Option<i64>,
    alert_kind: String,
    alert_status: String,
    created_at_ms: i64,
    event_id: String,
    handling_unit_id: String,
    handling_unit_label: Option<String>,
    event_kind: String,
    event_result: String,
    event_rejection_reason: Option<String>,
    source: String,
    occurred_at_ms: i64,
    origin_location_code: Option<String>,
    destination_location_code: Option<String>,
}

impl TryFrom<AlertRow> for AlertEntry {
    type Error = RepositoryError;

    fn try_from(row: AlertRow) -> Result<Self, Self::Error> {
        let status = match row.alert_status.as_str() {
            "open" => AlertStatus::Open,
            "reviewed" => AlertStatus::Reviewed,
            "resolved" => AlertStatus::Resolved,
            other => {
                return Err(RepositoryError::Storage(format!(
                    "estado de alerta desconocido: {other}"
                )));
            }
        };
        let target = match (row.movement_event_id, row.movement_attempt_id) {
            (Some(event_id), None) => AlertRecordTarget::MovementEvent { event_id },
            (None, Some(attempt_id)) => AlertRecordTarget::MovementAttempt {
                attempt_id,
                event_id: row.event_id,
            },
            _ => {
                return Err(RepositoryError::Storage(
                    "referencia de alerta inválida".to_owned(),
                ));
            }
        };
        let alert = Alert::from_persisted(
            row.alert_id,
            target,
            row.alert_kind,
            row.created_at_ms,
            status,
        )
        .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        Ok(Self {
            alert,
            handling_unit_id: row.handling_unit_id,
            handling_unit_label: row.handling_unit_label,
            event_kind: row.event_kind,
            event_result: row.event_result,
            event_rejection_reason: row.event_rejection_reason,
            source: row.source,
            occurred_at_ms: row.occurred_at_ms,
            origin_location_code: row.origin_location_code,
            destination_location_code: row.destination_location_code,
        })
    }
}

#[derive(sqlx::FromRow)]
struct MovementHistoryRow {
    record_id: String,
    event_id: String,
    handling_unit_id: String,
    kind: String,
    origin_location_id: Option<String>,
    destination_location_id: Option<String>,
    occurred_at_ms: i64,
    recorded_at_ms: i64,
    source: String,
    result: String,
    rejection_reason: Option<String>,
    corrects_event_id: Option<String>,
    correction_reason: Option<String>,
}

impl From<MovementHistoryRow> for MovementHistoryEntry {
    fn from(row: MovementHistoryRow) -> Self {
        Self {
            record_id: row.record_id,
            event_id: row.event_id,
            handling_unit_id: row.handling_unit_id,
            kind: row.kind,
            origin_location_id: row.origin_location_id,
            destination_location_id: row.destination_location_id,
            occurred_at_ms: row.occurred_at_ms,
            recorded_at_ms: row.recorded_at_ms,
            source: row.source,
            result: row.result,
            rejection_reason: row.rejection_reason,
            corrects_event_id: row.corrects_event_id,
            correction_reason: row.correction_reason,
        }
    }
}

type StoredEventPayload = (String, String, Option<String>, Option<String>, i64, String);

async fn event_payload_for_id(
    transaction: &mut Transaction<'_, Sqlite>,
    event_id: &str,
) -> Result<Option<StoredEventPayload>, RepositoryError> {
    sqlx::query_as::<_, StoredEventPayload>(
        "SELECT handling_unit_id, kind, origin_location_id, destination_location_id, occurred_at_ms, source FROM movement_events WHERE event_id = ?",
    )
    .bind(event_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(map_repository_error)
}

fn movement_payload_matches(previous: &StoredEventPayload, event: &MovementEvent) -> bool {
    previous.0 == event.handling_unit_id
        && previous.1 == event.kind.as_str()
        && previous.2 == event.origin_location_id
        && previous.3 == event.destination_location_id
        && previous.4 == event.occurred_at_ms
        && previous.5 == event.source
}

async fn movement_attempt_exists(
    transaction: &mut Transaction<'_, Sqlite>,
    event_id: &str,
) -> Result<bool, RepositoryError> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM movement_attempts WHERE event_id = ? AND result = 'rejected')",
    )
    .bind(event_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_repository_error)?;
    Ok(exists != 0)
}

async fn record_event_attempt(
    transaction: &mut Transaction<'_, Sqlite>,
    event: &MovementEvent,
    result: &str,
    rejection_reason: Option<&str>,
) -> Result<i64, RepositoryError> {
    let recorded = sqlx::query(
        "INSERT INTO movement_attempts (event_id, handling_unit_id, kind, origin_location_id, destination_location_id, occurred_at_ms, source, result, rejection_reason, created_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&event.event_id)
    .bind(&event.handling_unit_id)
    .bind(event.kind.as_str())
    .bind(&event.origin_location_id)
    .bind(&event.destination_location_id)
    .bind(event.occurred_at_ms)
    .bind(&event.source)
    .bind(result)
    .bind(rejection_reason)
    .bind(now_ms())
    .execute(&mut **transaction)
    .await
    .map_err(map_repository_error)?;
    Ok(recorded.last_insert_rowid())
}

async fn record_attempt_and_alert(
    transaction: &mut Transaction<'_, Sqlite>,
    event: &MovementEvent,
    attempt_result: &str,
    rejection_reason: Option<&str>,
    alert_intent: Option<&AlertIntent>,
) -> Result<MovementRecordOutcome, RepositoryError> {
    let attempt_id =
        record_event_attempt(transaction, event, attempt_result, rejection_reason).await?;
    let alert_id = match alert_intent {
        Some(intent) => Some(
            insert_alert_for_target(
                transaction,
                intent,
                AlertRecordTarget::MovementAttempt {
                    attempt_id,
                    event_id: event.event_id.clone(),
                },
            )
            .await?,
        ),
        None => None,
    };
    let result = if attempt_result == "duplicate" {
        MovementRecordResult::Duplicate
    } else {
        MovementRecordResult::Rejected(repository_error_for_attempt(rejection_reason))
    };
    Ok(MovementRecordOutcome {
        result,
        record_id: format!("attempt:{attempt_id}"),
        alert_id,
    })
}

fn outcome_with_error(
    mut outcome: MovementRecordOutcome,
    error: RepositoryError,
) -> MovementRecordOutcome {
    outcome.result = MovementRecordResult::Rejected(error);
    outcome
}

fn repository_error_for_attempt(reason: Option<&str>) -> RepositoryError {
    match reason {
        Some("event_id_conflict" | "event_id_previously_rejected") => {
            RepositoryError::EventIdConflict
        }
        Some("handling_unit_not_found") | Some("location_not_found_or_invalid") => {
            RepositoryError::NotFound
        }
        Some("invalid_destination") => RepositoryError::InvalidDestination,
        Some("position_occupied") => RepositoryError::PositionOccupied,
        Some("invalid_transition") => RepositoryError::InvalidTransition,
        Some("product_not_found") => RepositoryError::NotFound,
        Some("product_inactive") => RepositoryError::InactiveProduct,
        _ => RepositoryError::InvalidTransition,
    }
}

async fn insert_alert_for_target(
    transaction: &mut Transaction<'_, Sqlite>,
    intent: &AlertIntent,
    target: AlertRecordTarget,
) -> Result<String, RepositoryError> {
    let alert = Alert::from_intent(intent, target)
        .map_err(|error| RepositoryError::Storage(error.to_string()))?;
    sqlx::query(
        "INSERT INTO alerts (id, movement_event_id, movement_attempt_id, kind, created_at_ms) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&alert.id)
    .bind(alert.target.movement_event_id())
    .bind(alert.target.movement_attempt_id())
    .bind(&alert.kind)
    .bind(alert.created_at_ms)
    .execute(&mut **transaction)
    .await
    .map_err(map_repository_error)?;
    Ok(alert.id)
}

fn event_attempt_reason(error: &RepositoryError) -> &'static str {
    match error {
        RepositoryError::PositionOccupied => "position_occupied",
        RepositoryError::InvalidDestination => "invalid_destination",
        RepositoryError::NotFound => "location_not_found_or_invalid",
        _ => "movement_rejected",
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn product_from_row(row: SqliteRow) -> Result<Product, RepositoryError> {
    let sku = Sku::new(
        row.try_get::<String, _>("sku")
            .map_err(|error| RepositoryError::Storage(error.to_string()))?,
    )
    .map_err(|error| RepositoryError::Storage(error.to_string()))?;
    Ok(Product {
        sku,
        description: row
            .try_get("description")
            .map_err(|error| RepositoryError::Storage(error.to_string()))?,
        unit_of_measure: row
            .try_get("unit_of_measure")
            .map_err(|error| RepositoryError::Storage(error.to_string()))?,
        active: row
            .try_get::<i64, _>("is_active")
            .map_err(|error| RepositoryError::Storage(error.to_string()))?
            != 0,
    })
}

fn location_from_row(row: SqliteRow) -> Result<Location, RepositoryError> {
    let kind = match row
        .try_get::<String, _>("kind")
        .map_err(|error| RepositoryError::Storage(error.to_string()))?
        .as_str()
    {
        "zone" => LocationKind::Zone,
        "aisle" => LocationKind::Aisle,
        "rack" => LocationKind::Rack,
        "position" => LocationKind::Position,
        other => {
            return Err(RepositoryError::Storage(format!(
                "tipo de ubicación desconocido: {other}"
            )));
        }
    };
    Ok(Location {
        id: row
            .try_get("id")
            .map_err(|error| RepositoryError::Storage(error.to_string()))?,
        code: row
            .try_get("code")
            .map_err(|error| RepositoryError::Storage(error.to_string()))?,
        name: row
            .try_get("name")
            .map_err(|error| RepositoryError::Storage(error.to_string()))?,
        kind,
        parent_id: row
            .try_get("parent_id")
            .map_err(|error| RepositoryError::Storage(error.to_string()))?,
        active: row
            .try_get::<i64, _>("is_active")
            .map_err(|error| RepositoryError::Storage(error.to_string()))?
            != 0,
    })
}

fn transition_target(
    event: &MovementEvent,
    current_status: HandlingUnitStatus,
    current_location_id: Option<&str>,
) -> Result<(HandlingUnitStatus, Option<String>), RepositoryError> {
    use crate::domain::MovementKind;

    match event.kind {
        MovementKind::Stored
            if current_status == HandlingUnitStatus::Received
                && current_location_id.is_none()
                && event.origin_location_id.is_none()
                && event.destination_location_id.is_some() =>
        {
            Ok((
                HandlingUnitStatus::Stored,
                event.destination_location_id.clone(),
            ))
        }
        MovementKind::Moved
            if current_status == HandlingUnitStatus::Stored
                && event.origin_location_id.as_deref() == current_location_id
                && event.destination_location_id.is_some()
                && event.destination_location_id.as_deref() != current_location_id =>
        {
            Ok((
                HandlingUnitStatus::Stored,
                event.destination_location_id.clone(),
            ))
        }
        MovementKind::Dispatched
            if current_status == HandlingUnitStatus::Stored
                && event.origin_location_id.as_deref() == current_location_id
                && event.destination_location_id.is_none() =>
        {
            Ok((HandlingUnitStatus::Dispatched, None))
        }
        _ => Err(RepositoryError::InvalidTransition),
    }
}

fn map_repository_error(error: sqlx::Error) -> RepositoryError {
    let Some(database_error) = error.as_database_error() else {
        return RepositoryError::Storage(error.to_string());
    };
    let message = database_error.message();
    if message.contains("movement_event_id_conflict")
        || message.contains("movement_attempt_id_conflict")
        || message.contains("correction_id_conflict")
    {
        return RepositoryError::EventIdConflict;
    }
    if database_error.is_unique_violation() {
        if message.contains("movement_events.event_id") {
            RepositoryError::EventIdConflict
        } else if message.contains("handling_units.current_location_id") {
            RepositoryError::PositionOccupied
        } else {
            RepositoryError::Conflict
        }
    } else if database_error.is_foreign_key_violation() {
        RepositoryError::NotFound
    } else if message.contains("invalid_location_hierarchy") {
        RepositoryError::InvalidLocationHierarchy
    } else if message.contains("location_parent_is_immutable") {
        RepositoryError::LocationHierarchyImmutable
    } else if message.contains("location_occupied")
        || message.contains("location_has_active_children")
    {
        RepositoryError::InUse
    } else if message.contains("handling_unit_location_must_be_active_position") {
        RepositoryError::InvalidDestination
    } else {
        RepositoryError::Storage(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{
        CreateLocation, CreateProduct, DeactivateLocation, DeactivateProduct, DispatchUnit,
        EventOutcome, GetHandlingUnit, GetLocation, GetMovementHistory, GetProduct,
        InventoryRepository, InventorySearchKind, ListAlerts, ListHandlingUnits, ListLocations,
        ListProducts, MarkAlertReviewed, MoveUnit, ReceiveLine, ReceiveUnit, RecordCorrection,
        RunSimulation, SearchInventory, SimulationScenario, StoreUnit, UpdateLocation,
        UpdateProduct,
    };
    use crate::domain::{AlertIntent, AlertStatus, MovementKind, Sku};
    use std::sync::Arc;

    async fn insert_location_tree(pool: &SqlitePool) {
        sqlx::query(
            "INSERT INTO locations (id, code, name, kind) VALUES ('zone-1', 'Z1', 'Zona', 'zone')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO locations (id, code, name, kind, parent_id) VALUES ('aisle-1', 'A1', 'Pasillo', 'aisle', 'zone-1')")
            .execute(pool).await.unwrap();
        sqlx::query("INSERT INTO locations (id, code, name, kind, parent_id) VALUES ('rack-1', 'R1', 'Rack', 'rack', 'aisle-1')")
            .execute(pool).await.unwrap();
        for (id, code) in [("position-1", "P1"), ("position-2", "P2")] {
            sqlx::query("INSERT INTO locations (id, code, name, kind, parent_id) VALUES (?, ?, 'Posición', 'position', 'rack-1')")
                .bind(id).bind(code).execute(pool).await.unwrap();
        }
    }

    #[tokio::test]
    async fn handling_unit_queries_find_by_id_or_label_and_filter_dispatched_units() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        insert_location_tree(database.pool()).await;
        let repository = Arc::new(SqliteInventoryRepository::new(&database));
        repository
            .insert_product(&Product::new("SKU-1", "Caja", "unidad").unwrap())
            .await
            .unwrap();
        let unit = HandlingUnit::new(
            "unit-1",
            "PALLET-1",
            vec![HandlingUnitLine::new("SKU-1", 3, Some("LOT-1")).unwrap()],
        )
        .unwrap();
        let receipt = MovementEvent::new(
            "event-received",
            "unit-1",
            MovementKind::Received,
            None::<&str>,
            None::<&str>,
            1_800_000_000_000,
            "test",
        )
        .unwrap();
        repository.receive_unit(&unit, &receipt).await.unwrap();
        let store = MovementEvent::new(
            "event-stored",
            "unit-1",
            MovementKind::Stored,
            None::<&str>,
            Some("position-1"),
            1_800_000_000_001,
            "test",
        )
        .unwrap();
        repository.record_movement(&store).await.unwrap();

        let by_id = GetHandlingUnit::new(repository.clone())
            .execute(" unit-1 ")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(by_id.status, HandlingUnitStatus::Stored);
        assert_eq!(by_id.current_location_id.as_deref(), Some("position-1"));
        assert_eq!(by_id.lines[0].quantity, 3);
        assert_eq!(by_id.lines[0].lot_code.as_deref(), Some("LOT-1"));

        let by_label = GetHandlingUnit::new(repository.clone())
            .execute("pallet-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(by_label.id, "unit-1");

        let overlapping_unit = HandlingUnit::new(
            "unit-2",
            "unit-1",
            vec![HandlingUnitLine::new("SKU-1", 1, None::<&str>).unwrap()],
        )
        .unwrap();
        let overlapping_receipt = MovementEvent::new(
            "event-received-2",
            "unit-2",
            MovementKind::Received,
            None::<&str>,
            None::<&str>,
            1_800_000_000_003,
            "test",
        )
        .unwrap();
        repository
            .receive_unit(&overlapping_unit, &overlapping_receipt)
            .await
            .unwrap();
        let exact_id_match = GetHandlingUnit::new(repository.clone())
            .execute("unit-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(exact_id_match.id, "unit-1");

        let dispatch = MovementEvent::new(
            "event-dispatched",
            "unit-1",
            MovementKind::Dispatched,
            Some("position-1"),
            None::<&str>,
            1_800_000_000_002,
            "test",
        )
        .unwrap();
        repository.record_movement(&dispatch).await.unwrap();
        let active_units = ListHandlingUnits::new(repository.clone())
            .execute(false)
            .await
            .unwrap();
        assert_eq!(active_units.len(), 1);
        assert_eq!(active_units[0].id, "unit-2");
        let all = ListHandlingUnits::new(repository)
            .execute(true)
            .await
            .unwrap();
        assert_eq!(all.len(), 2);
        let dispatched_unit = all.iter().find(|unit| unit.id == "unit-1").unwrap();
        assert_eq!(dispatched_unit.status, HandlingUnitStatus::Dispatched);
        assert_eq!(dispatched_unit.current_location_id, None);
        database.close().await;
    }

    fn movement(
        id: &str,
        unit: &str,
        kind: MovementKind,
        origin: Option<&str>,
        destination: Option<&str>,
    ) -> MovementEvent {
        MovementEvent::new(
            id,
            unit,
            kind,
            origin,
            destination,
            1_800_000_000_000,
            "simulator",
        )
        .unwrap()
    }

    fn receipt_line(sku: &str, quantity: u32, lot_code: Option<&str>) -> ReceiveLine {
        ReceiveLine {
            sku: sku.to_owned(),
            quantity,
            lot_code: lot_code.map(str::to_owned),
        }
    }

    #[tokio::test]
    async fn product_catalog_use_cases_create_query_update_and_deactivate() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));

        let created = CreateProduct::new(repository.clone())
            .execute(" sku-100 ", " Caja", " unidad ")
            .await
            .unwrap();
        assert_eq!(created.sku.as_str(), "SKU-100");
        assert_eq!(
            GetProduct::new(repository.clone())
                .execute("sku-100")
                .await
                .unwrap(),
            Some(created.clone())
        );
        assert!(matches!(
            CreateProduct::new(repository.clone())
                .execute("SKU-100", "Duplicado", "unidad")
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::Conflict
            ))
        ));

        let updated = UpdateProduct::new(repository.clone())
            .execute("SKU-100", "Caja reforzada", "caja")
            .await
            .unwrap();
        assert_eq!(updated.description, "Caja reforzada");
        let listed_active = ListProducts::new(repository.clone())
            .execute(false)
            .await
            .unwrap();
        assert_eq!(listed_active.len(), 1);
        assert_eq!(listed_active[0], updated);

        let inactive = DeactivateProduct::new(repository.clone())
            .execute("SKU-100")
            .await
            .unwrap();
        assert!(!inactive.active);
        assert!(
            ListProducts::new(repository.clone())
                .execute(false)
                .await
                .unwrap()
                .is_empty()
        );
        let listed_all = ListProducts::new(repository.clone())
            .execute(true)
            .await
            .unwrap();
        assert_eq!(listed_all.len(), 1);
        assert_eq!(listed_all[0], inactive);
        assert_eq!(
            GetProduct::new(repository)
                .execute("SKU-100")
                .await
                .unwrap(),
            Some(inactive)
        );
        database.close().await;
    }

    #[tokio::test]
    async fn location_use_cases_validate_hierarchy_and_persist_updates() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        let create = CreateLocation::new(repository.clone());
        let zone = create
            .execute("z1", "Z1", "Zona", LocationKind::Zone, None)
            .await
            .unwrap();
        let aisle = create
            .execute("a1", "A1", "Pasillo", LocationKind::Aisle, Some("z1"))
            .await
            .unwrap();
        let rack = create
            .execute("r1", "R1", "Rack", LocationKind::Rack, Some("a1"))
            .await
            .unwrap();
        let rack_2 = create
            .execute("r2", "R2", "Rack 2", LocationKind::Rack, Some("a1"))
            .await
            .unwrap();
        create
            .execute("p1", "P1", "Posición", LocationKind::Position, Some("r1"))
            .await
            .unwrap();

        assert!(matches!(
            create
                .execute("bad", "BAD", "Pasillo sin padre", LocationKind::Aisle, None)
                .await,
            Err(crate::application::ApplicationError::Domain(
                crate::domain::DomainError::InvalidLocationHierarchy
            ))
        ));
        assert!(matches!(
            create
                .execute(
                    "bad-rack",
                    "BR",
                    "Rack bajo zona",
                    LocationKind::Rack,
                    Some("z1")
                )
                .await,
            Err(crate::application::ApplicationError::Domain(
                crate::domain::DomainError::InvalidLocationHierarchy
            ))
        ));
        assert!(matches!(
            create
                .execute(
                    "missing-parent",
                    "MP",
                    "Posición",
                    LocationKind::Position,
                    Some("no-such-rack")
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::NotFound
            ))
        ));
        assert!(matches!(
            create
                .execute(
                    "duplicate",
                    "Z1",
                    "Código duplicado",
                    LocationKind::Zone,
                    None
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::Conflict
            ))
        ));

        let renamed = UpdateLocation::new(repository.clone())
            .execute("p1", "P1-A", "Posición actualizada")
            .await
            .unwrap();
        assert_eq!(renamed.parent_id.as_deref(), Some("r1"));
        assert_eq!(
            GetLocation::new(repository.clone())
                .execute("p1")
                .await
                .unwrap(),
            Some(renamed.clone())
        );
        sqlx::query("INSERT INTO handling_units (id, label, status) VALUES ('unit-history', 'PALLET-HISTORY', 'dispatched')")
            .execute(database.pool()).await.unwrap();
        sqlx::query("INSERT INTO movement_events (event_id, handling_unit_id, kind, origin_location_id, occurred_at_ms, source) VALUES ('event-history', 'unit-history', 'dispatched', 'p1', 1800000000000, 'test')")
            .execute(database.pool()).await.unwrap();
        let reparent_error = sqlx::query("UPDATE locations SET parent_id = ? WHERE id = 'p1'")
            .bind(&rack_2.id)
            .execute(database.pool())
            .await
            .unwrap_err();
        assert_eq!(
            map_repository_error(reparent_error),
            RepositoryError::LocationHierarchyImmutable
        );
        let parent_id: String =
            sqlx::query_scalar("SELECT parent_id FROM locations WHERE id = 'p1'")
                .fetch_one(database.pool())
                .await
                .unwrap();
        assert_eq!(parent_id, "r1");
        let historical_event_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_events WHERE event_id = 'event-history' AND origin_location_id = 'p1'",
        ).fetch_one(database.pool()).await.unwrap();
        assert_eq!(historical_event_count, 1);
        let active_locations = ListLocations::new(repository.clone())
            .execute(false)
            .await
            .unwrap();
        assert_eq!(active_locations.len(), 5);
        assert_eq!(active_locations[0], zone);
        assert_eq!(active_locations[1], aisle);
        assert!(active_locations.contains(&rack));
        assert!(active_locations.contains(&renamed));

        let inactive_zone = create
            .execute("z2", "Z2", "Zona temporal", LocationKind::Zone, None)
            .await
            .unwrap();
        create
            .execute(
                "a2",
                "A2",
                "Pasillo temporal",
                LocationKind::Aisle,
                Some("z2"),
            )
            .await
            .unwrap();
        DeactivateLocation::new(repository.clone())
            .execute("a2")
            .await
            .unwrap();
        DeactivateLocation::new(repository.clone())
            .execute(&inactive_zone.id)
            .await
            .unwrap();
        let edited_archived_child = UpdateLocation::new(repository.clone())
            .execute("a2", "A2-OLD", "Pasillo archivado")
            .await
            .unwrap();
        assert!(!edited_archived_child.active);
        assert_eq!(edited_archived_child.parent_id.as_deref(), Some("z2"));
        assert!(matches!(
            create
                .execute(
                    "a3",
                    "A3",
                    "Bajo zona inactiva",
                    LocationKind::Aisle,
                    Some("z2")
                )
                .await,
            Err(crate::application::ApplicationError::Domain(
                crate::domain::DomainError::InactiveLocationParent
            ))
        ));
        database.close().await;
    }

    #[tokio::test]
    async fn deactivation_preserves_referenced_rows_and_blocks_active_location_dependencies() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        insert_location_tree(pool).await;
        let product = CreateProduct::new(repository.clone())
            .execute("SKU-REF", "Producto referenciado", "unidad")
            .await
            .unwrap();
        sqlx::query("INSERT INTO handling_units (id, label, status) VALUES ('unit-ref', 'PALLET-REF', 'received')")
            .execute(pool).await.unwrap();
        sqlx::query("INSERT INTO handling_unit_lines (handling_unit_id, product_sku, lot_code, quantity) VALUES ('unit-ref', ?, '', 1)")
            .bind(product.sku.as_str()).execute(pool).await.unwrap();

        let deactivated = DeactivateProduct::new(repository.clone())
            .execute(product.sku.as_str())
            .await
            .unwrap();
        assert!(!deactivated.active);
        let referenced_product_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM handling_unit_lines WHERE product_sku = ?")
                .bind(product.sku.as_str())
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(referenced_product_count, 1);

        assert!(matches!(
            DeactivateLocation::new(repository.clone())
                .execute("rack-1")
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::InUse
            ))
        ));
        sqlx::query("INSERT INTO handling_units (id, label, status) VALUES ('unit-out', 'PALLET-OUT', 'dispatched')")
            .execute(pool).await.unwrap();
        sqlx::query("INSERT INTO movement_events (event_id, handling_unit_id, kind, origin_location_id, occurred_at_ms, source) VALUES ('event-ref', 'unit-out', 'dispatched', 'position-1', 1800000000000, 'test')")
            .execute(pool).await.unwrap();
        let deactivated_position = DeactivateLocation::new(repository.clone())
            .execute("position-1")
            .await
            .unwrap();
        assert!(!deactivated_position.active);
        let historic_location_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_events WHERE origin_location_id = 'position-1'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(historic_location_count, 1);
        assert!(
            GetLocation::new(repository.clone())
                .execute("position-1")
                .await
                .unwrap()
                .is_some_and(|location| !location.active)
        );
        database.close().await;
    }

    #[tokio::test]
    async fn receipt_creates_unit_lines_and_event_atomically() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        CreateProduct::new(repository.clone())
            .execute("SKU-RECEIPT", "Producto recibido", "unidad")
            .await
            .unwrap();

        let received = ReceiveUnit::new(repository.clone())
            .execute(
                "unit-received",
                "PALLET-RECEIVED",
                vec![
                    receipt_line("SKU-RECEIPT", 12, Some("LOT-1")),
                    receipt_line("SKU-RECEIPT-2", 2, None),
                ],
                "receipt-event-1",
                1_800_000_000_000,
                "manual",
            )
            .await;
        assert!(matches!(
            received,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::NotFound
            ))
        ));
        let missing_sku_unit_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM handling_units WHERE id = 'unit-received'")
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(missing_sku_unit_count, 0);

        CreateProduct::new(repository.clone())
            .execute("SKU-RECEIPT-2", "Segundo producto", "unidad")
            .await
            .unwrap();
        let outcome = ReceiveUnit::new(repository.clone())
            .execute(
                "unit-received",
                "PALLET-RECEIVED",
                vec![
                    receipt_line("SKU-RECEIPT", 12, Some("LOT-1")),
                    receipt_line("SKU-RECEIPT-2", 2, None),
                ],
                "receipt-event-2",
                1_800_000_000_001,
                "manual",
            )
            .await
            .unwrap();
        assert_eq!(outcome, EventOutcome::Applied);

        let persisted_unit: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-received'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(persisted_unit, ("received".to_owned(), None));
        let lines: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM handling_unit_lines WHERE handling_unit_id = 'unit-received'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(lines, 2);
        let event: (String, String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT kind, result, origin_location_id, destination_location_id FROM movement_events WHERE event_id = 'receipt-event-2'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(
            event,
            ("received".to_owned(), "accepted".to_owned(), None, None)
        );
        database.close().await;
    }

    #[tokio::test]
    async fn failed_receipt_event_insert_rolls_back_unit_and_lines() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        CreateProduct::new(repository.clone())
            .execute("SKU-RECEIPT-ROLLBACK", "Producto", "unidad")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TRIGGER fail_receipt_event BEFORE INSERT ON movement_events WHEN NEW.event_id = 'receipt-forced-failure' BEGIN SELECT RAISE(ABORT, 'forced_failure'); END",
        )
        .execute(pool)
        .await
        .unwrap();

        let result = ReceiveUnit::new(repository)
            .execute(
                "unit-receipt-rollback",
                "PALLET-RECEIPT-ROLLBACK",
                vec![receipt_line(
                    "SKU-RECEIPT-ROLLBACK",
                    2,
                    Some("LOT-ROLLBACK"),
                )],
                "receipt-forced-failure",
                1_800_000_000_000,
                "test",
            )
            .await;
        assert!(matches!(
            result,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::Storage(_)
            ))
        ));

        let unit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM handling_units WHERE id = 'unit-receipt-rollback'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let line_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM handling_unit_lines WHERE handling_unit_id = 'unit-receipt-rollback'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let event_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_events WHERE event_id = 'receipt-forced-failure'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let attempt_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_attempts WHERE event_id = 'receipt-forced-failure'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(unit_count, 0);
        assert_eq!(line_count, 0);
        assert_eq!(event_count, 0);
        assert_eq!(attempt_count, 0);
        database.close().await;
    }

    #[tokio::test]
    async fn receipt_rejects_invalid_quantity_and_inactive_sku() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        CreateProduct::new(repository.clone())
            .execute("SKU-INACTIVE", "Producto", "unidad")
            .await
            .unwrap();
        DeactivateProduct::new(repository.clone())
            .execute("SKU-INACTIVE")
            .await
            .unwrap();

        assert!(matches!(
            ReceiveUnit::new(repository.clone())
                .execute(
                    "unit-zero",
                    "PALLET-ZERO",
                    vec![receipt_line("SKU-INACTIVE", 0, None)],
                    "event-zero",
                    1_800_000_000_000,
                    "manual"
                )
                .await,
            Err(crate::application::ApplicationError::Domain(
                crate::domain::DomainError::QuantityMustBePositive
            ))
        ));
        assert!(matches!(
            ReceiveUnit::new(repository.clone())
                .execute(
                    "unit-inactive",
                    "PALLET-INACTIVE",
                    vec![receipt_line("SKU-INACTIVE", 1, None)],
                    "event-inactive",
                    1_800_000_000_000,
                    "manual"
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::InactiveProduct
            ))
        ));
        let long_label = "L".repeat(121);
        assert!(matches!(
            ReceiveUnit::new(repository.clone())
                .execute(
                    "unit-long-label",
                    &long_label,
                    vec![receipt_line("SKU-INACTIVE", 1, None)],
                    "event-long-label",
                    1_800_000_000_000,
                    "manual"
                )
                .await,
            Err(crate::application::ApplicationError::Domain(
                crate::domain::DomainError::FieldTooLong("etiqueta de unidad")
            ))
        ));
        assert!(matches!(
            ReceiveUnit::new(repository.clone())
                .execute(
                    "unit-invalid-sku",
                    "PALLET-INVALID-SKU",
                    vec![receipt_line("SKU WITH SPACE", 1, None)],
                    "event-invalid-sku",
                    1_800_000_000_000,
                    "manual"
                )
                .await,
            Err(crate::application::ApplicationError::Domain(
                crate::domain::DomainError::InvalidSku
            ))
        ));
        let units: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM handling_units")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(units, 0);
        database.close().await;
    }

    #[tokio::test]
    async fn duplicate_receipt_event_is_idempotent_and_rejects_conflicting_payloads() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        CreateProduct::new(repository.clone())
            .execute("SKU-RECEIPT", "Producto recibido", "unidad")
            .await
            .unwrap();
        let receipt = ReceiveUnit::new(repository.clone());
        assert_eq!(
            receipt
                .execute(
                    "unit-first",
                    "PALLET-FIRST",
                    vec![receipt_line("SKU-RECEIPT", 1, None)],
                    "receipt-event-1",
                    1_800_000_000_000,
                    "manual",
                )
                .await
                .unwrap(),
            EventOutcome::Applied
        );

        assert_eq!(
            receipt
                .execute(
                    "unit-first",
                    "PALLET-FIRST",
                    vec![receipt_line("SKU-RECEIPT", 1, None)],
                    "receipt-event-1",
                    1_800_000_000_000,
                    "manual",
                )
                .await
                .unwrap(),
            EventOutcome::Duplicate
        );

        assert!(matches!(
            receipt
                .execute(
                    "unit-second",
                    "PALLET-SECOND",
                    vec![receipt_line("SKU-RECEIPT", 3, None)],
                    "receipt-event-1",
                    1_800_000_000_001,
                    "manual"
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::EventIdConflict
            ))
        ));
        let partial_units: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM handling_units WHERE id = 'unit-second'")
                .fetch_one(pool)
                .await
                .unwrap();
        let partial_lines: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM handling_unit_lines WHERE handling_unit_id = 'unit-second'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movement_events")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(partial_units, 0);
        assert_eq!(partial_lines, 0);
        assert_eq!(event_count, 1);
        database.close().await;
    }

    #[tokio::test]
    async fn dispatch_releases_position_and_preserves_receipt_history_and_lines() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        CreateProduct::new(repository.clone())
            .execute("SKU-DISPATCH", "Producto", "unidad")
            .await
            .unwrap();
        ReceiveUnit::new(repository.clone())
            .execute(
                "unit-dispatch",
                "PALLET-DISPATCH",
                vec![receipt_line("SKU-DISPATCH", 5, None)],
                "event-received",
                1_800_000_000_000,
                "manual",
            )
            .await
            .unwrap();
        repository
            .record_movement(&movement(
                "event-stored",
                "unit-dispatch",
                MovementKind::Stored,
                None,
                Some("position-1"),
            ))
            .await
            .unwrap();

        DispatchUnit::new(repository.clone())
            .execute(
                "event-dispatched",
                "unit-dispatch",
                "position-1",
                1_800_000_000_002,
                "manual",
            )
            .await
            .unwrap();
        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-dispatch'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, ("dispatched".to_owned(), None));
        let history: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_events WHERE handling_unit_id = 'unit-dispatch'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(history, 3);
        let line_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM handling_unit_lines WHERE handling_unit_id = 'unit-dispatch'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(line_count, 1);

        ReceiveUnit::new(repository.clone())
            .execute(
                "unit-next",
                "PALLET-NEXT",
                vec![receipt_line("SKU-DISPATCH", 1, None)],
                "event-next-received",
                1_800_000_000_003,
                "manual",
            )
            .await
            .unwrap();
        repository
            .record_movement(&movement(
                "event-next-stored",
                "unit-next",
                MovementKind::Stored,
                None,
                Some("position-1"),
            ))
            .await
            .unwrap();
        let position_units: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM handling_units WHERE current_location_id = 'position-1' AND status = 'stored'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(position_units, 1);
        database.close().await;
    }

    #[tokio::test]
    async fn dispatch_rejects_unit_that_is_not_stored_without_mutating_it() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        CreateProduct::new(repository.clone())
            .execute("SKU-DISPATCH", "Producto", "unidad")
            .await
            .unwrap();
        ReceiveUnit::new(repository.clone())
            .execute(
                "unit-received",
                "PALLET-RECEIVED",
                vec![receipt_line("SKU-DISPATCH", 1, None)],
                "event-received",
                1_800_000_000_000,
                "manual",
            )
            .await
            .unwrap();
        assert!(matches!(
            DispatchUnit::new(repository.clone())
                .execute(
                    "event-invalid-dispatch",
                    "unit-received",
                    "position-1",
                    1_800_000_000_001,
                    "manual"
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::InvalidTransition
            ))
        ));
        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-received'",
        )
        .fetch_one(database.pool())
        .await
        .unwrap();
        assert_eq!(state, ("received".to_owned(), None));
        let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movement_events")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(event_count, 1);
        database.close().await;
    }

    #[tokio::test]
    async fn migrations_create_schema_and_constraints() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();

        let migration_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(migration_count, 3);

        let product = Product::new("sku-1", "Caja", "unidad").unwrap();
        let repository = SqliteInventoryRepository::new(&database);
        repository.insert_product(&product).await.unwrap();
        assert_eq!(
            repository.insert_product(&product).await,
            Err(RepositoryError::Conflict)
        );

        sqlx::query(
            "INSERT INTO locations (id, code, name, kind) VALUES ('zone-1', 'Z1', 'Zona', 'zone')",
        )
        .execute(pool)
        .await
        .unwrap();
        let invalid_parent = sqlx::query(
            "INSERT INTO locations (id, code, name, kind, parent_id) VALUES ('rack-1', 'R1', 'Rack', 'rack', 'zone-1')",
        )
        .execute(pool)
        .await;
        assert!(invalid_parent.is_err());

        sqlx::query("INSERT INTO locations (id, code, name, kind, parent_id) VALUES ('aisle-1', 'A1', 'Pasillo', 'aisle', 'zone-1')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO locations (id, code, name, kind, parent_id) VALUES ('rack-1', 'R1', 'Rack', 'rack', 'aisle-1')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO locations (id, code, name, kind, parent_id) VALUES ('position-1', 'P1', 'Posición', 'position', 'rack-1')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO locations (id, code, name, kind) VALUES ('zone-inactive', 'Z2', 'Zona inactiva', 'zone')")
            .execute(pool).await.unwrap();
        sqlx::query("UPDATE locations SET is_active = 0 WHERE id = 'zone-inactive'")
            .execute(pool)
            .await
            .unwrap();
        let active_child_of_inactive_parent = sqlx::query("INSERT INTO locations (id, code, name, kind, parent_id) VALUES ('aisle-inactive', 'A2', 'Pasillo', 'aisle', 'zone-inactive')")
            .execute(pool).await;
        assert!(active_child_of_inactive_parent.is_err());
        sqlx::query("INSERT INTO handling_units (id, label, status, current_location_id) VALUES ('unit-1', 'PALLET-1', 'stored', 'position-1')")
            .execute(pool)
            .await
            .unwrap();
        let deactivate_occupied_position =
            sqlx::query("UPDATE locations SET is_active = 0 WHERE id = 'position-1'")
                .execute(pool)
                .await;
        assert!(deactivate_occupied_position.is_err());
        let deactivate_rack_with_active_position =
            sqlx::query("UPDATE locations SET is_active = 0 WHERE id = 'rack-1'")
                .execute(pool)
                .await;
        assert!(deactivate_rack_with_active_position.is_err());
        let occupied_position = sqlx::query("INSERT INTO handling_units (id, label, status, current_location_id) VALUES ('unit-2', 'PALLET-2', 'stored', 'position-1')")
            .execute(pool)
            .await;
        assert!(occupied_position.is_err());

        database.close().await;
    }

    #[tokio::test]
    async fn missing_unit_does_not_create_movement_event() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let repository = SqliteInventoryRepository::new(&database);
        let event = movement(
            "event-1",
            "unit-missing",
            MovementKind::Stored,
            None,
            Some("position-1"),
        );

        assert_eq!(
            repository.record_movement(&event).await,
            Err(RepositoryError::NotFound)
        );
        let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movement_events")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(events, 0);
        let history = repository
            .list_movement_history("unit-missing")
            .await
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].result, "rejected");
        assert_eq!(
            history[0].rejection_reason.as_deref(),
            Some("handling_unit_not_found")
        );
        database.close().await;
    }

    #[tokio::test]
    async fn movement_event_and_inventory_state_commit_as_one_valid_transition() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        sqlx::query("INSERT INTO handling_units (id, label, status) VALUES ('unit-1', 'PALLET-1', 'received')")
            .execute(pool).await.unwrap();
        let repository = SqliteInventoryRepository::new(&database);

        repository
            .record_movement(&movement(
                "event-1",
                "unit-1",
                MovementKind::Stored,
                None,
                Some("position-1"),
            ))
            .await
            .unwrap();
        repository
            .record_movement(&movement(
                "event-2",
                "unit-1",
                MovementKind::Moved,
                Some("position-1"),
                Some("position-2"),
            ))
            .await
            .unwrap();
        repository
            .record_movement(&movement(
                "event-3",
                "unit-1",
                MovementKind::Dispatched,
                Some("position-2"),
                None,
            ))
            .await
            .unwrap();

        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-1'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, ("dispatched".to_owned(), None));
        let event_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_events WHERE handling_unit_id = 'unit-1'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(event_count, 3);
        database.close().await;
    }

    #[tokio::test]
    async fn inconsistent_event_is_rejected_without_partial_changes() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        sqlx::query("INSERT INTO handling_units (id, label, status) VALUES ('unit-1', 'PALLET-1', 'received')")
            .execute(pool).await.unwrap();
        let repository = SqliteInventoryRepository::new(&database);
        let inconsistent = movement(
            "event-1",
            "unit-1",
            MovementKind::Stored,
            Some("position-2"),
            Some("position-1"),
        );

        assert_eq!(
            repository.record_movement(&inconsistent).await,
            Err(RepositoryError::InvalidTransition)
        );
        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-1'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, ("received".to_owned(), None));
        let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movement_events")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(event_count, 0);
        database.close().await;
    }

    #[tokio::test]
    async fn duplicate_movement_event_is_idempotent_and_conflicts_do_not_mutate_state() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        sqlx::query("INSERT INTO handling_units (id, label, status) VALUES ('unit-1', 'PALLET-1', 'received')")
            .execute(pool).await.unwrap();
        let repository = SqliteInventoryRepository::new(&database);
        assert_eq!(
            repository
                .record_movement(&movement(
                    "event-1",
                    "unit-1",
                    MovementKind::Stored,
                    None,
                    Some("position-1"),
                ))
                .await
                .unwrap(),
            EventOutcome::Applied
        );

        let retry = movement(
            "event-1",
            "unit-1",
            MovementKind::Stored,
            None,
            Some("position-1"),
        );
        assert_eq!(
            repository.record_movement(&retry).await.unwrap(),
            EventOutcome::Duplicate
        );

        let duplicate_id = movement(
            "event-1",
            "unit-1",
            MovementKind::Moved,
            Some("position-1"),
            Some("position-2"),
        );
        assert_eq!(
            repository.record_movement(&duplicate_id).await,
            Err(RepositoryError::EventIdConflict)
        );
        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-1'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, ("stored".to_owned(), Some("position-1".to_owned())));
        let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movement_events")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(event_count, 1);
        database.close().await;
    }

    #[tokio::test]
    async fn move_unit_records_origin_destination_and_updates_position() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        CreateProduct::new(repository.clone())
            .execute("SKU-MOVE", "Producto", "unidad")
            .await
            .unwrap();
        ReceiveUnit::new(repository.clone())
            .execute(
                "unit-move",
                "PALLET-MOVE",
                vec![receipt_line("SKU-MOVE", 1, None)],
                "event-received",
                1_800_000_000_000,
                "manual",
            )
            .await
            .unwrap();
        repository
            .record_movement(&movement(
                "event-stored",
                "unit-move",
                MovementKind::Stored,
                None,
                Some("position-1"),
            ))
            .await
            .unwrap();

        let move_unit = MoveUnit::new(repository.clone());
        assert_eq!(
            move_unit
                .execute(
                    "event-moved",
                    "unit-move",
                    "position-1",
                    "position-2",
                    1_800_000_000_003,
                    "manual",
                )
                .await
                .unwrap(),
            EventOutcome::Applied
        );

        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-move'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, ("stored".to_owned(), Some("position-2".to_owned())));
        let event: (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT kind, origin_location_id, destination_location_id FROM movement_events WHERE event_id = 'event-moved'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(
            event,
            (
                "moved".to_owned(),
                Some("position-1".to_owned()),
                Some("position-2".to_owned())
            )
        );
        database.close().await;
    }

    #[tokio::test]
    async fn store_unit_assigns_a_received_unit_to_a_position_idempotently() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        CreateProduct::new(repository.clone())
            .execute("SKU-STORE", "Producto", "unidad")
            .await
            .unwrap();
        ReceiveUnit::new(repository.clone())
            .execute(
                "unit-store",
                "PALLET-STORE",
                vec![receipt_line("SKU-STORE", 2, None)],
                "event-store-received",
                1_800_000_000_000,
                "manual",
            )
            .await
            .unwrap();
        ReceiveUnit::new(repository.clone())
            .execute(
                "unit-store-blocked",
                "PALLET-STORE-BLOCKED",
                vec![receipt_line("SKU-STORE", 1, None)],
                "event-store-received-blocked",
                1_800_000_000_002,
                "manual",
            )
            .await
            .unwrap();

        let store_unit = StoreUnit::new(repository.clone());
        let input = (
            "event-unit-stored",
            "unit-store",
            "position-1",
            1_800_000_000_001,
            "manual",
        );
        assert_eq!(
            store_unit
                .execute(input.0, input.1, input.2, input.3, input.4)
                .await
                .unwrap(),
            EventOutcome::Applied
        );
        assert_eq!(
            store_unit
                .execute(input.0, input.1, input.2, input.3, input.4)
                .await
                .unwrap(),
            EventOutcome::Duplicate
        );
        assert!(matches!(
            store_unit
                .execute(
                    "event-store-occupied-position",
                    "unit-store-blocked",
                    "position-1",
                    1_800_000_000_003,
                    "manual",
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::PositionOccupied
            ))
        ));

        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-store'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, ("stored".to_owned(), Some("position-1".to_owned())));
        let event: (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT kind, origin_location_id, destination_location_id FROM movement_events WHERE event_id = 'event-unit-stored'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(
            event,
            ("stored".to_owned(), None, Some("position-1".to_owned()))
        );
        let blocked_state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-store-blocked'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(blocked_state, ("received".to_owned(), None));
        database.close().await;
    }

    #[tokio::test]
    async fn move_unit_rejects_stale_origin_occupied_or_inactive_destinations() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        CreateProduct::new(repository.clone())
            .execute("SKU-MOVE", "Producto", "unidad")
            .await
            .unwrap();
        let receive = ReceiveUnit::new(repository.clone());
        receive
            .execute(
                "unit-move",
                "PALLET-MOVE",
                vec![receipt_line("SKU-MOVE", 1, None)],
                "event-received-1",
                1_800_000_000_000,
                "manual",
            )
            .await
            .unwrap();
        receive
            .execute(
                "unit-occupant",
                "PALLET-OCCUPANT",
                vec![receipt_line("SKU-MOVE", 1, None)],
                "event-received-2",
                1_800_000_000_001,
                "manual",
            )
            .await
            .unwrap();
        repository
            .record_movement(&movement(
                "event-stored-1",
                "unit-move",
                MovementKind::Stored,
                None,
                Some("position-1"),
            ))
            .await
            .unwrap();
        repository
            .record_movement(&movement(
                "event-stored-2",
                "unit-occupant",
                MovementKind::Stored,
                None,
                Some("position-2"),
            ))
            .await
            .unwrap();
        sqlx::query("INSERT INTO locations (id, code, name, kind, parent_id) VALUES ('position-3', 'P3', 'Posición 3', 'position', 'rack-1')")
            .execute(pool).await.unwrap();
        let move_unit = MoveUnit::new(repository.clone());

        assert!(matches!(
            move_unit
                .execute(
                    "event-stale-origin",
                    "unit-move",
                    "position-2",
                    "position-1",
                    1_800_000_000_004,
                    "manual",
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::InvalidTransition
            ))
        ));
        assert!(matches!(
            move_unit
                .execute(
                    "event-occupied-destination",
                    "unit-move",
                    "position-1",
                    "position-2",
                    1_800_000_000_005,
                    "manual",
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::PositionOccupied
            ))
        ));

        sqlx::query("UPDATE locations SET is_active = 0 WHERE id = 'position-3'")
            .execute(pool)
            .await
            .unwrap();
        assert!(matches!(
            move_unit
                .execute(
                    "event-inactive-destination",
                    "unit-move",
                    "position-1",
                    "position-3",
                    1_800_000_000_006,
                    "manual",
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::InvalidDestination
            ))
        ));

        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-move'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, ("stored".to_owned(), Some("position-1".to_owned())));
        let event_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_events WHERE handling_unit_id = 'unit-move'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(event_count, 2);
        database.close().await;
    }

    #[tokio::test]
    async fn failed_movement_event_insert_rolls_back_inventory_update() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        sqlx::query("INSERT INTO handling_units (id, label, status, current_location_id) VALUES ('unit-1', 'PALLET-1', 'stored', 'position-1')")
            .execute(pool).await.unwrap();
        sqlx::query(
            "CREATE TRIGGER fail_movement_insert BEFORE INSERT ON movement_events WHEN NEW.event_id = 'forced-failure' BEGIN SELECT RAISE(ABORT, 'forced_failure'); END",
        )
        .execute(pool)
        .await
        .unwrap();
        let repository = SqliteInventoryRepository::new(&database);

        assert!(matches!(
            repository
                .record_movement(&movement(
                    "forced-failure",
                    "unit-1",
                    MovementKind::Moved,
                    Some("position-1"),
                    Some("position-2"),
                ))
                .await,
            Err(RepositoryError::Storage(_))
        ));
        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-1'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, ("stored".to_owned(), Some("position-1".to_owned())));
        let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movement_events")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(event_count, 0);
        database.close().await;
    }

    #[tokio::test]
    async fn failed_alert_insert_rolls_back_movement_and_inventory_update() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        sqlx::query("INSERT INTO handling_units (id, label, status, current_location_id) VALUES ('unit-alert-rollback', 'PALLET-ALERT-ROLLBACK', 'stored', 'position-1')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TRIGGER fail_alert_insert BEFORE INSERT ON alerts WHEN NEW.id = 'alert-forced-failure-event' BEGIN SELECT RAISE(ABORT, 'forced_failure'); END",
        )
        .execute(pool)
        .await
        .unwrap();
        let repository = SqliteInventoryRepository::new(&database);
        let intent = AlertIntent::new("alert-forced-failure", "test", 1_800_000_000_001).unwrap();

        assert!(matches!(
            repository
                .record_movement_with_alert(
                    &movement(
                        "event-alert-forced-failure",
                        "unit-alert-rollback",
                        MovementKind::Moved,
                        Some("position-1"),
                        Some("position-2"),
                    ),
                    &intent,
                )
                .await,
            Err(RepositoryError::Storage(_))
        ));

        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-alert-rollback'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let event_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_events WHERE event_id = 'event-alert-forced-failure'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let alert_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM alerts WHERE id = 'alert-forced-failure-event'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let attempt_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_attempts WHERE event_id = 'event-alert-forced-failure'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, ("stored".to_owned(), Some("position-1".to_owned())));
        assert_eq!(event_count, 0);
        assert_eq!(alert_count, 0);
        assert_eq!(attempt_count, 0);
        database.close().await;
    }

    #[tokio::test]
    async fn concurrent_retry_of_same_movement_returns_applied_and_duplicate() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-concurrent-retry-{unique}"));
        std::fs::create_dir_all(&directory).unwrap();
        let database = SqliteDatabase::open(&directory.join("ariadna.sqlite3"))
            .await
            .unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        sqlx::query("INSERT INTO locations (id, code, name, kind, parent_id) VALUES ('position-3', 'P3', 'Posición 3', 'position', 'rack-1')")
            .execute(pool).await.unwrap();
        sqlx::query("INSERT INTO handling_units (id, label, status, current_location_id) VALUES ('unit-1', 'PALLET-1', 'stored', 'position-1')")
            .execute(pool).await.unwrap();
        let repository = SqliteInventoryRepository::new(&database);
        let event = movement(
            "event-concurrent",
            "unit-1",
            MovementKind::Moved,
            Some("position-1"),
            Some("position-2"),
        );

        let (first, second) = tokio::join!(
            repository.record_movement(&event),
            repository.record_movement(&event)
        );
        assert!(matches!(
            (first, second),
            (Ok(EventOutcome::Applied), Ok(EventOutcome::Duplicate))
                | (Ok(EventOutcome::Duplicate), Ok(EventOutcome::Applied))
        ));
        let event_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_events WHERE event_id = 'event-concurrent'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(event_count, 1);
        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-1'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, ("stored".to_owned(), Some("position-2".to_owned())));

        let first_payload = movement(
            "event-concurrent-conflict",
            "unit-1",
            MovementKind::Moved,
            Some("position-2"),
            Some("position-1"),
        );
        let second_payload = movement(
            "event-concurrent-conflict",
            "unit-1",
            MovementKind::Moved,
            Some("position-2"),
            Some("position-3"),
        );
        let (first, second) = tokio::join!(
            repository.record_movement(&first_payload),
            repository.record_movement(&second_payload)
        );
        let outcomes = [first, second];
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Ok(EventOutcome::Applied)))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Err(RepositoryError::EventIdConflict)))
                .count(),
            1
        );
        let conflicting_id_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_events WHERE event_id = 'event-concurrent-conflict'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(conflicting_id_count, 1);
        database.close().await;
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn concurrent_retry_of_same_receipt_creates_one_unit_and_returns_duplicate() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-concurrent-receipt-{unique}"));
        std::fs::create_dir_all(&directory).unwrap();
        let database = SqliteDatabase::open(&directory.join("ariadna.sqlite3"))
            .await
            .unwrap();
        let pool = database.pool();
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        CreateProduct::new(repository.clone())
            .execute("SKU-CONCURRENT", "Producto", "unidad")
            .await
            .unwrap();
        let receive = ReceiveUnit::new(repository);

        let (first, second) = tokio::join!(
            receive.execute(
                "unit-concurrent",
                "PALLET-CONCURRENT",
                vec![receipt_line("SKU-CONCURRENT", 2, Some("LOT-1"))],
                "receipt-concurrent",
                1_800_000_000_000,
                "manual",
            ),
            receive.execute(
                "unit-concurrent",
                "PALLET-CONCURRENT",
                vec![receipt_line("SKU-CONCURRENT", 2, Some("LOT-1"))],
                "receipt-concurrent",
                1_800_000_000_000,
                "manual",
            )
        );
        assert!(matches!(
            (first, second),
            (Ok(EventOutcome::Applied), Ok(EventOutcome::Duplicate))
                | (Ok(EventOutcome::Duplicate), Ok(EventOutcome::Applied))
        ));
        let units: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM handling_units WHERE id = 'unit-concurrent'")
                .fetch_one(pool)
                .await
                .unwrap();
        let lines: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM handling_unit_lines WHERE handling_unit_id = 'unit-concurrent'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_events WHERE event_id = 'receipt-concurrent'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!((units, lines, events), (1, 1, 1));
        database.close().await;
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn concurrent_moves_to_same_position_only_allow_one_unit() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-concurrent-position-{unique}"));
        std::fs::create_dir_all(&directory).unwrap();
        let database = SqliteDatabase::open(&directory.join("ariadna.sqlite3"))
            .await
            .unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        sqlx::query("INSERT INTO locations (id, code, name, kind, parent_id) VALUES ('position-3', 'P3', 'Posición 3', 'position', 'rack-1')")
            .execute(pool).await.unwrap();
        sqlx::query("INSERT INTO handling_units (id, label, status, current_location_id) VALUES ('unit-1', 'PALLET-1', 'stored', 'position-1'), ('unit-2', 'PALLET-2', 'stored', 'position-2')")
            .execute(pool).await.unwrap();
        let repository = SqliteInventoryRepository::new(&database);
        let first_event = movement(
            "event-first-unit",
            "unit-1",
            MovementKind::Moved,
            Some("position-1"),
            Some("position-3"),
        );
        let second_event = movement(
            "event-second-unit",
            "unit-2",
            MovementKind::Moved,
            Some("position-2"),
            Some("position-3"),
        );

        let (first, second) = tokio::join!(
            repository.record_movement(&first_event),
            repository.record_movement(&second_event)
        );
        let outcomes = [first, second];
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Ok(EventOutcome::Applied)))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Err(RepositoryError::PositionOccupied)))
                .count(),
            1
        );
        let occupant_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM handling_units WHERE current_location_id = 'position-3' AND status = 'stored'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(occupant_count, 1);
        let event_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_events WHERE destination_location_id = 'position-3'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(event_count, 1);
        database.close().await;
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn movement_history_reconstructs_timeline_and_records_rejections_and_duplicates() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        CreateProduct::new(repository.clone())
            .execute("SKU-HISTORY", "Producto", "unidad")
            .await
            .unwrap();
        ReceiveUnit::new(repository.clone())
            .execute(
                "unit-history",
                "PALLET-HISTORY",
                vec![receipt_line("SKU-HISTORY", 1, None)],
                "event-received",
                1_000,
                "manual",
            )
            .await
            .unwrap();
        let stored = MovementEvent::new(
            "event-stored",
            "unit-history",
            MovementKind::Stored,
            None::<&str>,
            Some("position-1"),
            2_000,
            "manual",
        )
        .unwrap();
        repository.record_movement(&stored).await.unwrap();

        let move_unit = MoveUnit::new(repository.clone());
        assert!(matches!(
            move_unit
                .execute(
                    "event-stale-origin",
                    "unit-history",
                    "position-2",
                    "position-1",
                    2_500,
                    "manual",
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::InvalidTransition
            ))
        ));
        assert!(matches!(
            move_unit
                .execute(
                    "event-stale-origin",
                    "unit-history",
                    "position-1",
                    "position-2",
                    2_600,
                    "manual",
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::EventIdConflict
            ))
        ));
        let move_result = move_unit
            .execute(
                "event-moved",
                "unit-history",
                "position-1",
                "position-2",
                3_000,
                "manual",
            )
            .await
            .unwrap();
        assert_eq!(move_result, EventOutcome::Applied);
        assert_eq!(
            move_unit
                .execute(
                    "event-moved",
                    "unit-history",
                    "position-1",
                    "position-2",
                    3_000,
                    "manual",
                )
                .await
                .unwrap(),
            EventOutcome::Duplicate
        );
        DispatchUnit::new(repository.clone())
            .execute(
                "event-dispatched",
                "unit-history",
                "position-2",
                4_000,
                "manual",
            )
            .await
            .unwrap();

        let history = GetMovementHistory::new(repository)
            .execute("unit-history")
            .await
            .unwrap();
        assert_eq!(history.len(), 7);
        assert!(
            history
                .windows(2)
                .all(|pair| pair[0].occurred_at_ms <= pair[1].occurred_at_ms)
        );
        let received = history
            .iter()
            .find(|entry| entry.record_id == "event-received")
            .unwrap();
        assert_eq!(
            (received.kind.as_str(), received.result.as_str()),
            ("received", "accepted")
        );
        assert_eq!(received.source, "manual");
        let stored = history
            .iter()
            .find(|entry| entry.record_id == "event-stored")
            .unwrap();
        assert_eq!(
            stored.destination_location_id.as_deref(),
            Some("position-1")
        );
        let moved_index = history
            .iter()
            .position(|entry| entry.record_id == "event-moved")
            .unwrap();
        let moved = &history[moved_index];
        assert_eq!(moved.origin_location_id.as_deref(), Some("position-1"));
        assert_eq!(moved.destination_location_id.as_deref(), Some("position-2"));
        assert_eq!(moved.result, "accepted");
        let duplicate_index = history
            .iter()
            .position(|entry| entry.event_id == "event-moved" && entry.result == "duplicate")
            .unwrap();
        assert!(moved_index < duplicate_index);
        let rejection = history
            .iter()
            .find(|entry| entry.event_id == "event-stale-origin")
            .unwrap();
        assert_eq!(rejection.result, "rejected");
        assert_eq!(
            rejection.rejection_reason.as_deref(),
            Some("invalid_transition")
        );
        let rejected_id_reuse = history
            .iter()
            .find(|entry| {
                entry.event_id == "event-stale-origin"
                    && entry.rejection_reason.as_deref() == Some("event_id_previously_rejected")
            })
            .unwrap();
        assert_eq!(rejected_id_reuse.result, "rejected");
        let dispatched = history
            .iter()
            .find(|entry| entry.record_id == "event-dispatched")
            .unwrap();
        assert_eq!(dispatched.origin_location_id.as_deref(), Some("position-2"));
        database.close().await;
    }

    #[tokio::test]
    async fn corrections_are_linked_idempotent_and_do_not_rewrite_or_move_inventory() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let pool = database.pool();
        insert_location_tree(pool).await;
        sqlx::query("INSERT INTO handling_units (id, label, status) VALUES ('unit-1', 'PALLET-1', 'received'), ('unit-2', 'PALLET-2', 'received')")
            .execute(pool).await.unwrap();
        let repository: Arc<dyn InventoryRepository> =
            Arc::new(SqliteInventoryRepository::new(&database));
        let stored = MovementEvent::new(
            "event-stored",
            "unit-1",
            MovementKind::Stored,
            None::<&str>,
            Some("position-1"),
            1_000,
            "manual",
        )
        .unwrap();
        repository.record_movement(&stored).await.unwrap();
        let correction = RecordCorrection::new(repository.clone());
        assert!(matches!(
            correction
                .execute(
                    "correction-before-original",
                    "unit-1",
                    "event-stored",
                    "Fecha inválida para una corrección.",
                    999,
                    "manual-audit",
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::CorrectionBeforeOriginal
            ))
        ));
        assert_eq!(
            correction
                .execute(
                    "correction-1",
                    "unit-1",
                    "event-stored",
                    "El conteo físico confirmó la posición registrada.",
                    2_000,
                    "manual-audit",
                )
                .await
                .unwrap(),
            EventOutcome::Applied
        );
        assert_eq!(
            correction
                .execute(
                    "correction-1",
                    "unit-1",
                    "event-stored",
                    "El conteo físico confirmó la posición registrada.",
                    2_000,
                    "manual-audit",
                )
                .await
                .unwrap(),
            EventOutcome::Duplicate
        );
        assert!(matches!(
            correction
                .execute(
                    "correction-1",
                    "unit-1",
                    "event-stored",
                    "contenido diferente",
                    2_000,
                    "manual-audit",
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::EventIdConflict
            ))
        ));
        assert!(matches!(
            correction
                .execute(
                    "correction-wrong-unit",
                    "unit-2",
                    "event-stored",
                    "Referencia cruzada inválida.",
                    2_001,
                    "manual-audit",
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::CorrectionReferenceMismatch
            ))
        ));
        assert!(matches!(
            correction
                .execute(
                    "correction-missing-event",
                    "unit-1",
                    "event-missing",
                    "El evento no existe.",
                    2_002,
                    "manual-audit",
                )
                .await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::NotFound
            ))
        ));

        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, current_location_id FROM handling_units WHERE id = 'unit-1'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, ("stored".to_owned(), Some("position-1".to_owned())));
        let original: (String, String, Option<String>) = sqlx::query_as(
            "SELECT kind, source, destination_location_id FROM movement_events WHERE event_id = 'event-stored'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(
            original,
            (
                "stored".to_owned(),
                "manual".to_owned(),
                Some("position-1".to_owned())
            )
        );
        let history = GetMovementHistory::new(repository)
            .execute("unit-1")
            .await
            .unwrap();
        assert_eq!(history.len(), 2);
        let correction_entry = history
            .iter()
            .find(|entry| entry.record_id == "correction-1")
            .unwrap();
        assert_eq!(correction_entry.kind, "correction");
        assert_eq!(
            correction_entry.corrects_event_id.as_deref(),
            Some("event-stored")
        );
        assert_eq!(
            correction_entry.correction_reason.as_deref(),
            Some("El conteo físico confirmó la posición registrada.")
        );
        assert_eq!(correction_entry.result, "accepted");
        let correction_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM movement_corrections WHERE correction_id = 'correction-1'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(correction_count, 1);
        database.close().await;
    }

    #[tokio::test]
    async fn movement_history_rejects_updates_and_deletes() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        insert_location_tree(database.pool()).await;
        sqlx::query("INSERT INTO handling_units (id, label, status) VALUES ('unit-1', 'PALLET-1', 'received')")
            .execute(database.pool()).await.unwrap();
        let repository = SqliteInventoryRepository::new(&database);
        repository
            .record_movement(&movement(
                "event-1",
                "unit-1",
                MovementKind::Stored,
                None,
                Some("position-1"),
            ))
            .await
            .unwrap();
        assert_eq!(
            repository
                .record_movement(&movement(
                    "event-rejected",
                    "unit-1",
                    MovementKind::Moved,
                    Some("position-2"),
                    Some("position-1"),
                ))
                .await,
            Err(RepositoryError::InvalidTransition)
        );
        sqlx::query("INSERT INTO movement_corrections (correction_id, handling_unit_id, corrects_event_id, reason, occurred_at_ms, source) VALUES ('correction-1', 'unit-1', 'event-1', 'Corrección de prueba', 1800000000001, 'test')")
            .execute(database.pool()).await.unwrap();
        let error = sqlx::query("INSERT INTO movement_corrections (correction_id, handling_unit_id, corrects_event_id, reason, occurred_at_ms, source) VALUES ('correction-before-original', 'unit-1', 'event-1', 'Fecha previa no permitida', 1799999999999, 'test')")
            .execute(database.pool())
            .await
            .unwrap_err();
        let message = error
            .as_database_error()
            .map(|database_error| database_error.message())
            .unwrap_or_default();
        assert!(
            message.contains("correction_precedes_original"),
            "{message}"
        );
        assert!(
            sqlx::query("INSERT INTO movement_attempts (event_id, handling_unit_id, kind, occurred_at_ms, source, result, rejection_reason) VALUES ('correction-1', 'unit-1', 'moved', 1800000000002, 'test', 'rejected', 'id_collision')")
                .execute(database.pool())
                .await
                .is_err()
        );

        assert!(
            sqlx::query("UPDATE movement_events SET source = 'edited' WHERE event_id = 'event-1'")
                .execute(database.pool())
                .await
                .is_err()
        );
        assert!(
            sqlx::query("DELETE FROM movement_events WHERE event_id = 'event-1'")
                .execute(database.pool())
                .await
                .is_err()
        );
        assert!(
            sqlx::query(
                "UPDATE movement_attempts SET source = 'edited' WHERE event_id = 'event-rejected'"
            )
            .execute(database.pool())
            .await
            .is_err()
        );
        assert!(
            sqlx::query("DELETE FROM movement_attempts WHERE event_id = 'event-rejected'")
                .execute(database.pool())
                .await
                .is_err()
        );
        assert!(
            sqlx::query("UPDATE movement_corrections SET reason = 'edited' WHERE correction_id = 'correction-1'")
                .execute(database.pool())
                .await
                .is_err()
        );
        assert!(
            sqlx::query("DELETE FROM movement_corrections WHERE correction_id = 'correction-1'")
                .execute(database.pool())
                .await
                .is_err()
        );
        database.close().await;
    }

    #[tokio::test]
    async fn migrations_run_on_a_clean_database_and_a_file_copy() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-sqlite-{unique}"));
        std::fs::create_dir_all(&directory).unwrap();
        let original_path = directory.join("original.sqlite3");
        let copy_path = directory.join("copy.sqlite3");

        let original = SqliteDatabase::open(&original_path).await.unwrap();
        SqliteInventoryRepository::new(&original)
            .insert_product(&Product::new("SKU-COPY", "Caja", "unidad").unwrap())
            .await
            .unwrap();
        let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(original.pool())
            .await
            .unwrap();
        original.close().await;
        std::fs::copy(&original_path, &copy_path).unwrap();

        let copied = SqliteDatabase::open(&copy_path).await.unwrap();
        let product_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE sku = 'SKU-COPY'")
                .fetch_one(copied.pool())
                .await
                .unwrap();
        let migration_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(copied.pool())
                .await
                .unwrap();
        assert_eq!(product_count, 1);
        assert_eq!(migration_count, 3);

        copied.close().await;
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn inventory_and_history_persist_after_reopening_the_same_database_file() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-reopen-{unique}"));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("inventory.sqlite3");

        {
            let database = SqliteDatabase::open(&path).await.unwrap();
            let repository = SqliteInventoryRepository::new(&database);
            repository
                .insert_product(&Product::new("SKU-REOPEN", "Caja persistida", "unidad").unwrap())
                .await
                .unwrap();
            insert_location_tree(database.pool()).await;
            let unit = HandlingUnit::new(
                "unit-reopen",
                "PALLET-REOPEN",
                vec![HandlingUnitLine::new("SKU-REOPEN", 4, Some("LOT-REOPEN")).unwrap()],
            )
            .unwrap();
            repository
                .receive_unit(
                    &unit,
                    &movement(
                        "event-reopen-received",
                        "unit-reopen",
                        MovementKind::Received,
                        None,
                        None,
                    ),
                )
                .await
                .unwrap();
            let store_event = movement(
                "event-reopen-stored",
                "unit-reopen",
                MovementKind::Stored,
                None,
                Some("position-1"),
            );
            let alert_intent =
                AlertIntent::new("alert-reopen", "persisted_test", 1_800_000_000_001).unwrap();
            let movement_result = repository
                .record_movement_with_alert(&store_event, &alert_intent)
                .await
                .unwrap();
            assert_eq!(
                movement_result.alert_id.as_deref(),
                Some("alert-reopen-event")
            );
            database.close().await;
        }

        {
            let database = SqliteDatabase::open(&path).await.unwrap();
            let repository: Arc<dyn InventoryRepository> =
                Arc::new(SqliteInventoryRepository::new(&database));
            let product = GetProduct::new(repository.clone())
                .execute("SKU-REOPEN")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(product.description, "Caja persistida");

            let unit = GetHandlingUnit::new(repository.clone())
                .execute("PALLET-REOPEN")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(unit.status, HandlingUnitStatus::Stored);
            assert_eq!(unit.current_location_id.as_deref(), Some("position-1"));
            assert_eq!(unit.lines[0].quantity, 4);
            assert_eq!(unit.lines[0].lot_code.as_deref(), Some("LOT-REOPEN"));

            let history = GetMovementHistory::new(repository.clone())
                .execute("unit-reopen")
                .await
                .unwrap();
            assert_eq!(history.len(), 2);
            assert_eq!(history[0].event_id, "event-reopen-received");
            assert_eq!(history[1].event_id, "event-reopen-stored");

            let alerts = ListAlerts::new(repository).execute().await.unwrap();
            assert_eq!(alerts.len(), 1);
            assert_eq!(alerts[0].alert.id, "alert-reopen-event");
            assert_eq!(
                alerts[0].alert.target.movement_event_id(),
                Some("event-reopen-stored")
            );
            database.close().await;
        }

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn inventory_search_matches_products_units_and_locations_case_insensitively() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        insert_location_tree(database.pool()).await;
        let repository = Arc::new(SqliteInventoryRepository::new(&database));
        repository
            .insert_product(&Product::new("SKU-SEARCH", "Caja azul", "unidad").unwrap())
            .await
            .unwrap();
        let unit = HandlingUnit::new(
            "unit-search",
            "PALLET-SEARCH",
            vec![HandlingUnitLine::new("SKU-SEARCH", 2, None::<&str>).unwrap()],
        )
        .unwrap();
        repository
            .receive_unit(
                &unit,
                &movement(
                    "event-search-received",
                    "unit-search",
                    MovementKind::Received,
                    None,
                    None,
                ),
            )
            .await
            .unwrap();
        repository
            .record_movement(&movement(
                "event-search-stored",
                "unit-search",
                MovementKind::Stored,
                None,
                Some("position-1"),
            ))
            .await
            .unwrap();

        let search = SearchInventory::new(repository);
        let by_sku = search.execute(" sku-search ").await.unwrap();
        assert!(by_sku.iter().any(|result| {
            result.kind == InventorySearchKind::Product && result.id == "SKU-SEARCH"
        }));
        assert!(by_sku.iter().any(|result| {
            result.kind == InventorySearchKind::HandlingUnit && result.id == "unit-search"
        }));

        let by_label = search.execute("pallet-sea").await.unwrap();
        assert_eq!(by_label.len(), 1);
        assert_eq!(by_label[0].kind, InventorySearchKind::HandlingUnit);
        assert_eq!(by_label[0].status, "stored");

        let by_location = search.execute("p1").await.unwrap();
        assert!(by_location.iter().any(|result| {
            result.kind == InventorySearchKind::Location && result.id == "position-1"
        }));
        assert!(by_location.iter().any(|result| {
            result.kind == InventorySearchKind::HandlingUnit && result.id == "unit-search"
        }));
        assert!(search.execute("   ").await.unwrap().is_empty());
        database.close().await;
    }

    #[tokio::test]
    async fn alerts_list_their_movement_and_review_is_persisted_and_idempotent() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let repository = Arc::new(SqliteInventoryRepository::new(&database));
        repository
            .insert_product(&Product::new("SKU-ALERT", "Caja", "unidad").unwrap())
            .await
            .unwrap();
        let unit = HandlingUnit::new(
            "unit-alert",
            "PALLET-ALERT",
            vec![HandlingUnitLine::new("SKU-ALERT", 1, None::<&str>).unwrap()],
        )
        .unwrap();
        repository
            .receive_unit(
                &unit,
                &movement(
                    "event-alert",
                    "unit-alert",
                    MovementKind::Received,
                    None,
                    None,
                ),
            )
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO alerts (id, movement_event_id, kind, created_at_ms) VALUES ('alert-1', 'event-alert', 'revisión manual', 1800000000000)",
        )
        .execute(database.pool())
        .await
        .unwrap();

        let listed = ListAlerts::new(repository.clone()).execute().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].alert.status, AlertStatus::Open);
        assert_eq!(listed[0].handling_unit_id, "unit-alert");
        assert_eq!(
            listed[0].handling_unit_label.as_deref(),
            Some("PALLET-ALERT")
        );
        assert_eq!(
            listed[0].alert.target.movement_event_id(),
            Some("event-alert")
        );

        let review = MarkAlertReviewed::new(repository.clone());
        let reviewed = review.execute(" alert-1 ").await.unwrap();
        assert_eq!(reviewed.alert.status, AlertStatus::Reviewed);
        let repeated = review.execute("alert-1").await.unwrap();
        assert_eq!(repeated.alert.status, AlertStatus::Reviewed);
        assert_eq!(
            ListAlerts::new(repository.clone()).execute().await.unwrap()[0]
                .alert
                .status,
            AlertStatus::Reviewed
        );
        assert!(matches!(
            review.execute("missing-alert").await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::NotFound
            ))
        ));
        sqlx::query(
            "INSERT INTO alerts (id, movement_event_id, kind, status, created_at_ms) VALUES ('alert-resolved', 'event-alert', 'already_resolved', 'resolved', 1800000000001)",
        )
        .execute(database.pool())
        .await
        .unwrap();
        assert!(matches!(
            review.execute("alert-resolved").await,
            Err(crate::application::ApplicationError::Repository(
                RepositoryError::InvalidAlertTransition
            ))
        ));
        database.close().await;
    }

    #[tokio::test]
    async fn simulator_scenarios_use_cases_and_alert_exact_attempt_records() {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let repository = Arc::new(SqliteInventoryRepository::new(&database));
        let simulator = RunSimulation::new(repository.clone());

        let normal = simulator
            .execute("seed-normal", SimulationScenario::Normal)
            .await
            .unwrap();
        assert_eq!(normal.steps.len(), 4);
        assert!(normal.steps.iter().all(|step| step.result == "applied"));
        assert!(normal.steps.iter().all(|step| step.alert_id.is_none()));
        assert!(
            ListAlerts::new(repository.clone())
                .execute()
                .await
                .unwrap()
                .is_empty()
        );

        let duplicate = simulator
            .execute("seed-duplicate", SimulationScenario::Duplicate)
            .await
            .unwrap();
        assert_eq!(duplicate.steps.len(), 4);
        assert_eq!(duplicate.steps[2].result, "applied");
        assert_eq!(duplicate.steps[3].result, "duplicate");
        assert_ne!(duplicate.steps[2].record_id, duplicate.steps[3].record_id);

        let unknown = simulator
            .execute("seed-unknown", SimulationScenario::UnknownLabel)
            .await
            .unwrap();
        let invalid = simulator
            .execute("seed-invalid", SimulationScenario::InvalidLocation)
            .await
            .unwrap();
        let occupied = simulator
            .execute("seed-occupied", SimulationScenario::OccupiedDestination)
            .await
            .unwrap();

        let alerts = ListAlerts::new(repository.clone()).execute().await.unwrap();
        assert_eq!(alerts.len(), 4);
        let cases = [
            (&duplicate.steps[3], "duplicate", None, true),
            (
                &unknown.steps[0],
                "rejected",
                Some("handling_unit_not_found"),
                false,
            ),
            (
                &invalid.steps[1],
                "rejected",
                Some("invalid_destination"),
                false,
            ),
            (
                &occupied.steps[3],
                "rejected",
                Some("position_occupied"),
                false,
            ),
        ];

        for (step, expected_result, expected_reason, is_duplicate) in cases {
            assert_eq!(step.result, expected_result);
            let record_id = step.record_id.as_deref().expect("intento con recordId");
            let attempt_id = record_id
                .strip_prefix("attempt:")
                .expect("recordId debe distinguir el intento")
                .parse::<i64>()
                .unwrap();
            let alert_id = step.alert_id.as_deref().expect("alerta persistida");
            let attempt = sqlx::query_as::<_, (String, String, String, Option<String>, String)>(
                "SELECT event_id, handling_unit_id, result, rejection_reason, source FROM movement_attempts WHERE attempt_id = ?",
            )
            .bind(attempt_id)
            .fetch_one(database.pool())
            .await
            .unwrap();
            assert_eq!(attempt.0, step.event_id.as_deref().unwrap());
            assert_eq!(attempt.1, step.handling_unit_id.as_deref().unwrap());
            assert_eq!(attempt.2, expected_result);
            assert_eq!(attempt.3.as_deref(), expected_reason);
            assert_eq!(attempt.4, "simulator");

            let alert = alerts
                .iter()
                .find(|alert| alert.alert.id == alert_id)
                .expect("alerta listada");
            assert_eq!(alert.alert.target.record_id(), record_id);
            assert_eq!(
                alert.alert.target.event_id(),
                step.event_id.as_deref().unwrap()
            );
            assert_eq!(alert.event_result, expected_result);
            assert_eq!(alert.event_rejection_reason.as_deref(), expected_reason);
            assert_eq!(alert.source, "simulator");
            assert_eq!(alert.handling_unit_id, attempt.1);
            assert_eq!(
                alert.handling_unit_label.is_none(),
                alert.handling_unit_id.contains("unknown-label")
            );
            assert!(matches!(
                alert.alert.target,
                AlertRecordTarget::MovementAttempt { attempt_id: linked_id, .. } if linked_id == attempt_id
            ));
            if is_duplicate {
                assert_eq!(expected_reason, None);
                let accepted_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM movement_events WHERE event_id = ?")
                        .bind(step.event_id.as_deref().unwrap())
                        .fetch_one(database.pool())
                        .await
                        .unwrap();
                assert_eq!(accepted_count, 1);
            }

            let history = repository.list_movement_history(&attempt.1).await.unwrap();
            assert!(history.iter().any(|entry| entry.record_id == record_id));
        }

        assert!(
            unknown.steps[0]
                .handling_unit_id
                .as_deref()
                .unwrap()
                .contains("unknown-label")
        );
        let unknown_alert = alerts
            .iter()
            .find(|alert| alert.alert.id == unknown.steps[0].alert_id.as_deref().unwrap())
            .unwrap();
        assert_eq!(unknown_alert.handling_unit_label, None);

        let normal_unit = normal.steps[0].handling_unit_id.as_deref().unwrap();
        let normal_state: (String, Option<String>) =
            sqlx::query_as("SELECT status, current_location_id FROM handling_units WHERE id = ?")
                .bind(normal_unit)
                .fetch_one(database.pool())
                .await
                .unwrap();
        assert_eq!(normal_state, ("dispatched".to_owned(), None));

        let invalid_unit = invalid.steps[0].handling_unit_id.as_deref().unwrap();
        let invalid_state: (String, Option<String>) =
            sqlx::query_as("SELECT status, current_location_id FROM handling_units WHERE id = ?")
                .bind(invalid_unit)
                .fetch_one(database.pool())
                .await
                .unwrap();
        assert_eq!(invalid_state, ("received".to_owned(), None));

        let occupied_probe = occupied.steps[2].handling_unit_id.as_deref().unwrap();
        let occupied_state: (String, Option<String>) =
            sqlx::query_as("SELECT status, current_location_id FROM handling_units WHERE id = ?")
                .bind(occupied_probe)
                .fetch_one(database.pool())
                .await
                .unwrap();
        assert_eq!(occupied_state, ("received".to_owned(), None));

        let duplicate_move = duplicate.steps[2].event_id.as_deref().unwrap();
        let duplicate_events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM movement_events WHERE event_id = ?")
                .bind(duplicate_move)
                .fetch_one(database.pool())
                .await
                .unwrap();
        assert_eq!(duplicate_events, 1);

        let reviewed_id = duplicate.steps[3].alert_id.as_deref().unwrap();
        let reviewer = MarkAlertReviewed::new(repository.clone());
        assert_eq!(
            reviewer.execute(reviewed_id).await.unwrap().alert.status,
            AlertStatus::Reviewed
        );
        assert_eq!(
            reviewer.execute(reviewed_id).await.unwrap().alert.status,
            AlertStatus::Reviewed
        );

        let rerun = simulator
            .execute("seed-duplicate", SimulationScenario::Duplicate)
            .await
            .unwrap();
        assert_eq!(rerun.steps[2].event_id, duplicate.steps[2].event_id);
        assert_eq!(rerun.steps[3].result, "duplicate");
        let unchanged: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM movement_events WHERE event_id = ?")
                .bind(duplicate_move)
                .fetch_one(database.pool())
                .await
                .unwrap();
        assert_eq!(unchanged, 1);
        database.close().await;
    }

    #[tokio::test]
    async fn simulator_anomaly_reruns_create_distinct_exact_attempt_alerts_without_moving_units() {
        type EventSnapshotRow = (String, String, String, Option<String>, Option<String>, i64);

        let database = SqliteDatabase::in_memory().await.unwrap();
        let repository = Arc::new(SqliteInventoryRepository::new(&database));
        let simulator = RunSimulation::new(repository.clone());
        let cases = [
            ("rerun-duplicate", SimulationScenario::Duplicate, 3),
            ("rerun-unknown", SimulationScenario::UnknownLabel, 0),
            ("rerun-invalid", SimulationScenario::InvalidLocation, 1),
            ("rerun-occupied", SimulationScenario::OccupiedDestination, 3),
        ];
        let mut first_runs = Vec::new();
        for (seed, scenario, anomaly_index) in cases {
            let response = simulator.execute(seed, scenario).await.unwrap();
            let step = &response.steps[anomaly_index];
            assert!(step.record_id.as_deref().unwrap().starts_with("attempt:"));
            assert!(step.alert_id.is_some());
            first_runs.push((seed, scenario, anomaly_index, response));
        }

        let events_before: Vec<EventSnapshotRow> =
            sqlx::query_as("SELECT event_id, handling_unit_id, kind, origin_location_id, destination_location_id, occurred_at_ms FROM movement_events ORDER BY event_id")
                .fetch_all(database.pool())
                .await
                .unwrap();
        let unit_states_before: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, status, current_location_id FROM handling_units ORDER BY id",
        )
        .fetch_all(database.pool())
        .await
        .unwrap();
        let attempts_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movement_attempts")
            .fetch_one(database.pool())
            .await
            .unwrap();

        for (seed, scenario, anomaly_index, first_response) in first_runs {
            let first_step = &first_response.steps[anomaly_index];
            let rerun = simulator.execute(seed, scenario).await.unwrap();
            let step = &rerun.steps[anomaly_index];
            assert_eq!(step.event_id, first_step.event_id, "{}", scenario.as_str());
            assert_eq!(step.handling_unit_id, first_step.handling_unit_id);
            assert!(matches!(step.result.as_str(), "duplicate" | "rejected"));
            assert_ne!(step.record_id, first_step.record_id);
            assert_ne!(step.alert_id, first_step.alert_id);

            let record_id = step.record_id.as_deref().unwrap();
            let attempt_id = record_id
                .strip_prefix("attempt:")
                .unwrap()
                .parse::<i64>()
                .unwrap();
            let alert_id = step.alert_id.as_deref().unwrap();
            let alert = ListAlerts::new(repository.clone())
                .execute()
                .await
                .unwrap()
                .into_iter()
                .find(|alert| alert.alert.id == alert_id)
                .expect("la alerta del rerun debe existir");
            assert_eq!(alert.alert.target.record_id(), record_id);
            assert_eq!(alert.alert.target.movement_attempt_id(), Some(attempt_id));
            assert_eq!(
                alert.alert.target.event_id(),
                step.event_id.as_deref().unwrap()
            );
            assert_eq!(alert.source, "simulator");
            assert_eq!(alert.event_result, step.result);
        }

        let events_after: Vec<EventSnapshotRow> =
            sqlx::query_as("SELECT event_id, handling_unit_id, kind, origin_location_id, destination_location_id, occurred_at_ms FROM movement_events ORDER BY event_id")
                .fetch_all(database.pool())
                .await
                .unwrap();
        let unit_states_after: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, status, current_location_id FROM handling_units ORDER BY id",
        )
        .fetch_all(database.pool())
        .await
        .unwrap();
        let attempts_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movement_attempts")
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(events_after, events_before);
        assert_eq!(unit_states_after, unit_states_before);
        assert_eq!(attempts_after, attempts_before + 11);
        database.close().await;
    }

    #[tokio::test]
    async fn simulator_rejects_manual_collisions_with_reserved_sku_and_location_metadata() {
        for (seed, description, unit_of_measure) in [
            ("reserved-sku-description", "Producto manual", "unidad"),
            ("reserved-sku-unit", "Producto sintético de Ariadna", "caja"),
        ] {
            let database = SqliteDatabase::in_memory().await.unwrap();
            let repository = Arc::new(SqliteInventoryRepository::new(&database));
            let hash = test_seed_hash(seed);
            let sku = format!("SIM.{hash:016X}");
            repository
                .insert_product(&Product::new(&sku, description, unit_of_measure).unwrap())
                .await
                .unwrap();

            let result = RunSimulation::new(repository.clone())
                .execute(seed, SimulationScenario::Normal)
                .await;
            assert!(matches!(
                result,
                Err(crate::application::ApplicationError::Repository(
                    RepositoryError::Conflict
                ))
            ));
            let movement_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM movement_events WHERE source = 'simulator'",
            )
            .fetch_one(database.pool())
            .await
            .unwrap();
            assert_eq!(movement_count, 0);
            let location_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM locations")
                .fetch_one(database.pool())
                .await
                .unwrap();
            assert_eq!(location_count, 0);
            database.close().await;
        }

        for (seed, use_expected_code, name) in [
            ("reserved-location-code", false, "Zona sintética"),
            ("reserved-location-name", true, "Zona manual"),
        ] {
            let database = SqliteDatabase::in_memory().await.unwrap();
            let repository = Arc::new(SqliteInventoryRepository::new(&database));
            let hash = test_seed_hash(seed);
            let prefix = format!("sim-{hash:016x}");
            let expected_code = format!("SZ{hash:016X}");
            let manual_code = if use_expected_code {
                expected_code
            } else {
                "MANUAL-CODE".to_owned()
            };
            let zone = Location::new(
                format!("{prefix}-zone"),
                manual_code,
                name,
                LocationKind::Zone,
                None,
            )
            .unwrap();
            repository.insert_location(&zone).await.unwrap();

            let result = RunSimulation::new(repository.clone())
                .execute(seed, SimulationScenario::Normal)
                .await;
            assert!(matches!(
                result,
                Err(crate::application::ApplicationError::Repository(
                    RepositoryError::Conflict
                ))
            ));
            let movement_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM movement_events WHERE source = 'simulator'",
            )
            .fetch_one(database.pool())
            .await
            .unwrap();
            assert_eq!(movement_count, 0);
            database.close().await;
        }
    }

    fn test_seed_hash(seed: &str) -> u64 {
        seed.bytes().fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
    }

    #[tokio::test]
    async fn simulator_seed_reproduces_event_attempt_and_alert_payloads() {
        for scenario in [
            SimulationScenario::Normal,
            SimulationScenario::Duplicate,
            SimulationScenario::UnknownLabel,
            SimulationScenario::InvalidLocation,
            SimulationScenario::OccupiedDestination,
        ] {
            let left = simulation_snapshot("repeatable-seed", scenario).await;
            let right = simulation_snapshot("repeatable-seed", scenario).await;
            assert_eq!(left, right, "scenario {}", scenario.as_str());
        }
    }

    async fn simulation_snapshot(
        seed: &str,
        scenario: SimulationScenario,
    ) -> (
        crate::application::SimulationResponse,
        Vec<String>,
        Vec<String>,
        Vec<String>,
    ) {
        let database = SqliteDatabase::in_memory().await.unwrap();
        let repository = Arc::new(SqliteInventoryRepository::new(&database));
        let response = RunSimulation::new(repository.clone())
            .execute(seed, scenario)
            .await
            .unwrap();
        let event_rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, i64, String)>(
            "SELECT event_id, handling_unit_id, kind, origin_location_id, destination_location_id, occurred_at_ms, source FROM movement_events WHERE source = 'simulator' ORDER BY occurred_at_ms, event_id",
        )
        .fetch_all(database.pool())
        .await
        .unwrap();
        let attempt_rows = sqlx::query_as::<_, (i64, String, String, String, Option<String>, i64, String)>(
            "SELECT attempt_id, event_id, handling_unit_id, result, rejection_reason, occurred_at_ms, source FROM movement_attempts WHERE source = 'simulator' ORDER BY attempt_id",
        )
        .fetch_all(database.pool())
        .await
        .unwrap();
        let alert_rows = sqlx::query_as::<_, (String, Option<String>, Option<i64>, String, i64)>(
            "SELECT id, movement_event_id, movement_attempt_id, kind, created_at_ms FROM alerts ORDER BY id",
        )
        .fetch_all(database.pool())
        .await
        .unwrap();
        let snapshot = (
            response,
            event_rows.iter().map(|row| format!("{row:?}")).collect(),
            attempt_rows.iter().map(|row| format!("{row:?}")).collect(),
            alert_rows.iter().map(|row| format!("{row:?}")).collect(),
        );
        database.close().await;
        snapshot
    }

    #[tokio::test]
    async fn migration_0003_preserves_event_alerts_and_enforces_target_xor_and_foreign_keys() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-migration-0003-{unique}"));
        std::fs::create_dir_all(&directory).unwrap();
        let migrations_path = directory.join("migrations");
        std::fs::create_dir_all(&migrations_path).unwrap();
        let source_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        for name in ["0001_initial_inventory.sql", "0002_movement_history.sql"] {
            std::fs::copy(source_path.join(name), migrations_path.join(name)).unwrap();
        }

        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate::Migrator::new(migrations_path.as_path())
            .await
            .unwrap()
            .run(&pool)
            .await
            .unwrap();

        let database = SqliteDatabase { pool: pool.clone() };
        insert_location_tree(database.pool()).await;
        let repository = SqliteInventoryRepository::new(&database);
        repository
            .insert_product(&Product::new("SKU-MIGRATION", "Caja", "unidad").unwrap())
            .await
            .unwrap();
        let unit = HandlingUnit::new(
            "unit-migration",
            "PALLET-MIGRATION",
            vec![HandlingUnitLine::new("SKU-MIGRATION", 1, None::<&str>).unwrap()],
        )
        .unwrap();
        repository
            .receive_unit(
                &unit,
                &movement(
                    "event-migration",
                    "unit-migration",
                    MovementKind::Received,
                    None,
                    None,
                ),
            )
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO alerts (id, movement_event_id, kind, created_at_ms) VALUES ('legacy-alert', 'event-migration', 'legacy', 1800000000000)",
        )
        .execute(database.pool())
        .await
        .unwrap();

        std::fs::copy(
            source_path.join("0003_alert_record_target.sql"),
            migrations_path.join("0003_alert_record_target.sql"),
        )
        .unwrap();
        sqlx::migrate::Migrator::new(migrations_path.as_path())
            .await
            .unwrap()
            .run(database.pool())
            .await
            .unwrap();

        let legacy_alert = repository.list_alerts().await.unwrap().remove(0);
        assert_eq!(legacy_alert.alert.id, "legacy-alert");
        assert_eq!(
            legacy_alert.alert.target.movement_event_id(),
            Some("event-migration")
        );
        let migration_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(database.pool())
                .await
                .unwrap();
        assert_eq!(migration_count, 3);

        sqlx::query("INSERT INTO movement_attempts (event_id, handling_unit_id, kind, occurred_at_ms, source, result, rejection_reason) VALUES ('attempt-migration', 'unknown-label', 'stored', 1800000000001, 'simulator', 'rejected', 'handling_unit_not_found')")
            .execute(database.pool())
            .await
            .unwrap();
        let attempt_id: i64 = sqlx::query_scalar(
            "SELECT attempt_id FROM movement_attempts WHERE event_id = 'attempt-migration'",
        )
        .fetch_one(database.pool())
        .await
        .unwrap();

        let no_target = sqlx::query(
            "INSERT INTO alerts (id, kind, created_at_ms) VALUES ('no-target', 'invalid', 1800000000002)",
        )
        .execute(database.pool())
        .await;
        assert!(no_target.is_err());
        let both_targets = sqlx::query(
            "INSERT INTO alerts (id, movement_event_id, movement_attempt_id, kind, created_at_ms) VALUES ('both-targets', 'event-migration', ?, 'invalid', 1800000000002)",
        )
        .bind(attempt_id)
        .execute(database.pool())
        .await;
        assert!(both_targets.is_err());
        let bad_event_fk = sqlx::query(
            "INSERT INTO alerts (id, movement_event_id, kind, created_at_ms) VALUES ('bad-event-fk', 'missing-event', 'invalid', 1800000000002)",
        )
        .execute(database.pool())
        .await;
        assert!(bad_event_fk.is_err());
        let bad_attempt_fk = sqlx::query(
            "INSERT INTO alerts (id, movement_attempt_id, kind, created_at_ms) VALUES ('bad-attempt-fk', ?, 'invalid', 1800000000002)",
        )
        .bind(attempt_id + 10)
        .execute(database.pool())
        .await;
        assert!(bad_attempt_fk.is_err());
        let fk_violations = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(database.pool())
            .await
            .unwrap();
        assert!(fk_violations.is_empty());

        database.close().await;
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn sqlite_adapter_accepts_domain_sku_type() {
        assert_eq!(Sku::new("sku-2").unwrap().as_str(), "SKU-2");
    }
}
