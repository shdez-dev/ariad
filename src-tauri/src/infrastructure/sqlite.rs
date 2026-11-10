use std::{path::Path, time::Duration};

use async_trait::async_trait;
use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use thiserror::Error;

use crate::{
    application::{InventoryRepository, RepositoryError},
    domain::{HandlingUnitStatus, MovementEvent, Product},
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
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
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

    async fn record_movement(&self, event: &MovementEvent) -> Result<(), RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(map_repository_error)?;

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

        transaction.commit().await.map_err(map_repository_error)
    }
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
    if error
        .as_database_error()
        .is_some_and(|database_error| database_error.is_unique_violation())
    {
        RepositoryError::Conflict
    } else if error
        .as_database_error()
        .is_some_and(|database_error| database_error.is_foreign_key_violation())
    {
        RepositoryError::NotFound
    } else {
        RepositoryError::Storage(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MovementKind, Sku};

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
    async fn duplicate_event_rolls_back_the_unit_update() {
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

        let duplicate_id = movement(
            "event-1",
            "unit-1",
            MovementKind::Moved,
            Some("position-1"),
            Some("position-2"),
        );
        assert_eq!(
            repository.record_movement(&duplicate_id).await,
            Err(RepositoryError::Conflict)
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
