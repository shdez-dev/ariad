use std::{path::Path, time::Duration};

use async_trait::async_trait;
use sqlx::{
    Row, Sqlite, SqlitePool, Transaction,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
};
use thiserror::Error;

use crate::{
    application::{EventOutcome, InventoryRepository, RepositoryError},
    domain::{
        HandlingUnit, HandlingUnitStatus, Location, LocationKind, MovementEvent, MovementKind,
        Product, Sku,
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
                transaction.commit().await.map_err(map_repository_error)?;
                return Ok(EventOutcome::Duplicate);
            }
            let _ = transaction.rollback().await;
            return Err(RepositoryError::EventIdConflict);
        }

        let result = async {
            for line in &unit.lines {
                let product_active = sqlx::query_scalar::<_, i64>(
                    "SELECT is_active FROM products WHERE sku = ?",
                )
                .bind(line.sku.as_str())
                .fetch_optional(&mut *transaction)
                .await
                .map_err(map_repository_error)?
                .ok_or(RepositoryError::NotFound)?;
                if product_active == 0 {
                    return Err(RepositoryError::InactiveProduct);
                }
                if line.quantity == 0 {
                    return Err(RepositoryError::Storage(
                        "cantidad de línea inválida en unidad de recepción".to_owned(),
                    ));
                }
            }

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
                "INSERT INTO movement_events (event_id, handling_unit_id, kind, origin_location_id, destination_location_id, occurred_at_ms, source, result) VALUES (?, ?, 'received', NULL, NULL, ?, ?, 'accepted')",
            )
            .bind(&event.event_id)
            .bind(&unit.id)
            .bind(event.occurred_at_ms)
            .bind(&event.source)
            .execute(&mut *transaction)
            .await
            .map_err(map_repository_error)?;

            Ok::<(), RepositoryError>(())
        }
        .await;

        if let Err(error) = result {
            let _ = transaction.rollback().await;
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
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(map_repository_error)?;

        if let Some(previous_event) =
            event_payload_for_id(&mut transaction, &event.event_id).await?
        {
            if movement_payload_matches(&previous_event, event) {
                transaction.commit().await.map_err(map_repository_error)?;
                return Ok(EventOutcome::Duplicate);
            }
            let _ = transaction.rollback().await;
            return Err(RepositoryError::EventIdConflict);
        }

        let current = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT status, current_location_id FROM handling_units WHERE id = ?",
        )
        .bind(&event.handling_unit_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_repository_error)?
        .ok_or(RepositoryError::NotFound)?;

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
            transition_target(event, current_status, current.1.as_deref())?;

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
                let _ = transaction.rollback().await;
                return Err(map_repository_error(error));
            }
        };
        if update.rows_affected() != 1 {
            let _ = transaction.rollback().await;
            return Err(RepositoryError::InvalidTransition);
        }

        let insert_result = sqlx::query(
            "INSERT INTO movement_events (event_id, handling_unit_id, kind, origin_location_id, destination_location_id, occurred_at_ms, source, result) VALUES (?, ?, ?, ?, ?, ?, ?, 'accepted')",
        )
        .bind(&event.event_id)
        .bind(&event.handling_unit_id)
        .bind(event.kind.as_str())
        .bind(&event.origin_location_id)
        .bind(&event.destination_location_id)
        .bind(event.occurred_at_ms)
        .bind(&event.source)
        .execute(&mut *transaction)
        .await;

        if let Err(error) = insert_result {
            let _ = transaction.rollback().await;
            return Err(map_repository_error(error));
        }

        transaction
            .commit()
            .await
            .map(|()| EventOutcome::Applied)
            .map_err(map_repository_error)
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
        EventOutcome, GetLocation, GetProduct, InventoryRepository, ListLocations, ListProducts,
        MoveUnit, ReceiveLine, ReceiveUnit, UpdateLocation, UpdateProduct,
    };
    use crate::domain::{MovementKind, Sku};
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
                "receipt-event-1",
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
            "SELECT kind, result, origin_location_id, destination_location_id FROM movement_events WHERE event_id = 'receipt-event-1'",
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
        assert_eq!(migration_count, 1);

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
        assert_eq!(migration_count, 1);

        copied.close().await;
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn sqlite_adapter_accepts_domain_sku_type() {
        assert_eq!(Sku::new("sku-2").unwrap().as_str(), "SKU-2");
    }
}
