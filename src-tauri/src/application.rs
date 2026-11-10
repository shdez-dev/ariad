use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{
    DomainError, HandlingUnit, HandlingUnitLine, Location, LocationKind, MovementEvent,
    MovementKind, Product, Sku,
};

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RepositoryError {
    #[error("el registro ya existe")]
    Conflict,
    #[error("el registro relacionado no existe")]
    NotFound,
    #[error("el producto no existe o está inactivo")]
    InactiveProduct,
    #[error("la transición solicitada no es válida para el estado actual")]
    InvalidTransition,
    #[error("la jerarquía de ubicación no es válida")]
    InvalidLocationHierarchy,
    #[error("la relación jerárquica es inmutable para conservar el historial")]
    LocationHierarchyImmutable,
    #[error("la ubicación tiene existencias o dependencias activas")]
    InUse,
    #[error("error de persistencia: {0}")]
    Storage(String),
}

#[async_trait]
pub trait InventoryRepository: Send + Sync {
    async fn insert_product(&self, product: &Product) -> Result<(), RepositoryError>;

    async fn find_product(&self, sku: &Sku) -> Result<Option<Product>, RepositoryError>;

    async fn list_products(&self, include_inactive: bool) -> Result<Vec<Product>, RepositoryError>;

    async fn update_product(&self, product: &Product) -> Result<(), RepositoryError>;

    async fn deactivate_product(&self, sku: &Sku) -> Result<(), RepositoryError>;

    async fn insert_location(&self, location: &Location) -> Result<(), RepositoryError>;

    async fn find_location(&self, id: &str) -> Result<Option<Location>, RepositoryError>;

    async fn list_locations(
        &self,
        include_inactive: bool,
    ) -> Result<Vec<Location>, RepositoryError>;

    async fn update_location(&self, location: &Location) -> Result<(), RepositoryError>;

    async fn deactivate_location(&self, id: &str) -> Result<(), RepositoryError>;

    async fn receive_unit(
        &self,
        unit: &HandlingUnit,
        event: &MovementEvent,
    ) -> Result<(), RepositoryError>;

    async fn record_movement(&self, event: &MovementEvent) -> Result<(), RepositoryError>;
}

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl ApplicationError {
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::Domain(DomainError::InvalidSku) => {
                "El SKU solo admite letras, números, guion, guion bajo o punto."
            }
            Self::Domain(DomainError::InvalidLocationHierarchy) => {
                "El nivel y la ubicación padre no forman una jerarquía válida."
            }
            Self::Domain(DomainError::InactiveLocationParent) => {
                "No se puede crear una ubicación bajo un padre inactivo."
            }
            Self::Domain(DomainError::EmptyField(field)) => match *field {
                "SKU" => "El SKU es obligatorio.",
                "código de ubicación" => "El código de ubicación es obligatorio.",
                "nombre de ubicación" => "El nombre de ubicación es obligatorio.",
                _ => "Completa los campos obligatorios.",
            },
            Self::Domain(DomainError::FieldTooLong("etiqueta de unidad")) => {
                "La etiqueta de la unidad no puede superar 120 caracteres."
            }
            Self::Domain(DomainError::FieldTooLong(_)) => {
                "Uno de los campos ingresados supera la longitud permitida."
            }
            Self::Domain(_) => "Revisa los datos ingresados y el estado actual del inventario.",
            Self::Repository(RepositoryError::Conflict) => {
                "El SKU, código o identificador ya existe; no se guardaron cambios."
            }
            Self::Repository(RepositoryError::NotFound) => {
                "No se encontró el producto, ubicación o referencia indicada; no se guardaron cambios."
            }
            Self::Repository(RepositoryError::InactiveProduct) => {
                "El SKU no existe o está inactivo; no se recibió la unidad."
            }
            Self::Repository(RepositoryError::InvalidTransition) => {
                "El movimiento no es válido para el estado actual; no se guardaron cambios."
            }
            Self::Repository(RepositoryError::InvalidLocationHierarchy) => {
                "El nivel y la ubicación padre no forman una jerarquía válida."
            }
            Self::Repository(RepositoryError::LocationHierarchyImmutable) => {
                "La relación jerárquica se conserva para proteger el historial; actualiza el código o el nombre."
            }
            Self::Repository(RepositoryError::InUse) => {
                "No se puede desactivar la ubicación mientras tenga dependencias activas u ocupación."
            }
            Self::Repository(RepositoryError::Storage(_)) => {
                "No se pudo completar la operación. No se confirmaron cambios."
            }
        }
    }
}

pub struct CreateProduct {
    repository: Arc<dyn InventoryRepository>,
}

impl CreateProduct {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        sku: &str,
        description: &str,
        unit_of_measure: &str,
    ) -> Result<Product, ApplicationError> {
        let product = Product::new(sku, description, unit_of_measure)?;
        self.repository.insert_product(&product).await?;
        Ok(product)
    }
}

pub struct GetProduct {
    repository: Arc<dyn InventoryRepository>,
}

impl GetProduct {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, sku: &str) -> Result<Option<Product>, ApplicationError> {
        let sku = Sku::new(sku)?;
        Ok(self.repository.find_product(&sku).await?)
    }
}

pub struct ListProducts {
    repository: Arc<dyn InventoryRepository>,
}

impl ListProducts {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, include_inactive: bool) -> Result<Vec<Product>, ApplicationError> {
        Ok(self.repository.list_products(include_inactive).await?)
    }
}

pub struct UpdateProduct {
    repository: Arc<dyn InventoryRepository>,
}

impl UpdateProduct {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        sku: &str,
        description: &str,
        unit_of_measure: &str,
    ) -> Result<Product, ApplicationError> {
        let sku = Sku::new(sku)?;
        let existing = self
            .repository
            .find_product(&sku)
            .await?
            .ok_or(RepositoryError::NotFound)?;
        let mut updated = Product::new(sku.as_str(), description, unit_of_measure)?;
        updated.active = existing.active;
        self.repository.update_product(&updated).await?;
        Ok(updated)
    }
}

pub struct DeactivateProduct {
    repository: Arc<dyn InventoryRepository>,
}

impl DeactivateProduct {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, sku: &str) -> Result<Product, ApplicationError> {
        let sku = Sku::new(sku)?;
        let mut product = self
            .repository
            .find_product(&sku)
            .await?
            .ok_or(RepositoryError::NotFound)?;
        self.repository.deactivate_product(&sku).await?;
        product.deactivate();
        Ok(product)
    }
}

pub struct CreateLocation {
    repository: Arc<dyn InventoryRepository>,
}

impl CreateLocation {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        id: &str,
        code: &str,
        name: &str,
        kind: LocationKind,
        parent_id: Option<&str>,
    ) -> Result<Location, ApplicationError> {
        let parent = match parent_id {
            Some(parent_id) => self
                .repository
                .find_location(parent_id)
                .await?
                .ok_or(RepositoryError::NotFound)?,
            None => return self.create_root(id, code, name, kind).await,
        };
        let location = Location::new(id, code, name, kind, Some(&parent))?;
        self.repository.insert_location(&location).await?;
        Ok(location)
    }

    async fn create_root(
        &self,
        id: &str,
        code: &str,
        name: &str,
        kind: LocationKind,
    ) -> Result<Location, ApplicationError> {
        let location = Location::new(id, code, name, kind, None)?;
        self.repository.insert_location(&location).await?;
        Ok(location)
    }
}

pub struct GetLocation {
    repository: Arc<dyn InventoryRepository>,
}

impl GetLocation {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, id: &str) -> Result<Option<Location>, ApplicationError> {
        if id.trim().is_empty() {
            return Err(DomainError::EmptyField("identificador de ubicación").into());
        }
        Ok(self.repository.find_location(id.trim()).await?)
    }
}

pub struct ListLocations {
    repository: Arc<dyn InventoryRepository>,
}

impl ListLocations {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, include_inactive: bool) -> Result<Vec<Location>, ApplicationError> {
        Ok(self.repository.list_locations(include_inactive).await?)
    }
}

pub struct UpdateLocation {
    repository: Arc<dyn InventoryRepository>,
}

impl UpdateLocation {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        id: &str,
        code: &str,
        name: &str,
    ) -> Result<Location, ApplicationError> {
        if id.trim().is_empty() {
            return Err(DomainError::EmptyField("identificador de ubicación").into());
        }
        let existing = self
            .repository
            .find_location(id.trim())
            .await?
            .ok_or(RepositoryError::NotFound)?;
        let mut updated = existing;
        updated.update_details(code, name)?;
        self.repository.update_location(&updated).await?;
        Ok(updated)
    }
}

pub struct DeactivateLocation {
    repository: Arc<dyn InventoryRepository>,
}

impl DeactivateLocation {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, id: &str) -> Result<Location, ApplicationError> {
        let mut location = self
            .repository
            .find_location(id.trim())
            .await?
            .ok_or(RepositoryError::NotFound)?;
        self.repository.deactivate_location(id.trim()).await?;
        location.deactivate();
        Ok(location)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiveLine {
    pub sku: String,
    pub quantity: u32,
    pub lot_code: Option<String>,
}

pub struct ReceiveUnit {
    repository: Arc<dyn InventoryRepository>,
}

impl ReceiveUnit {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        id: &str,
        label: &str,
        lines: Vec<ReceiveLine>,
        event_id: &str,
        occurred_at_ms: i64,
        source: &str,
    ) -> Result<HandlingUnit, ApplicationError> {
        let lines = lines
            .into_iter()
            .map(|line| HandlingUnitLine::new(line.sku, line.quantity, line.lot_code.as_deref()))
            .collect::<Result<Vec<_>, _>>()?;
        let unit = HandlingUnit::new(id, label, lines)?;
        let event = MovementEvent::new(
            event_id,
            &unit.id,
            MovementKind::Received,
            None::<&str>,
            None::<&str>,
            occurred_at_ms,
            source,
        )?;
        self.repository.receive_unit(&unit, &event).await?;
        Ok(unit)
    }
}

pub struct DispatchUnit {
    repository: Arc<dyn InventoryRepository>,
}

impl DispatchUnit {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        event_id: &str,
        handling_unit_id: &str,
        origin_location_id: &str,
        occurred_at_ms: i64,
        source: &str,
    ) -> Result<(), ApplicationError> {
        let event = MovementEvent::new(
            event_id,
            handling_unit_id,
            MovementKind::Dispatched,
            Some(origin_location_id),
            None::<&str>,
            occurred_at_ms,
            source,
        )?;
        self.repository.record_movement(&event).await?;
        Ok(())
    }
}

pub fn map_error_for_ui(
    diagnostic_log: &crate::diagnostic_log::DiagnosticLog,
    operation: &str,
    error: &ApplicationError,
) -> String {
    let _ = diagnostic_log.record(operation, error);
    error.user_message().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MovementEvent;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeRepository {
        saved_skus: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl InventoryRepository for FakeRepository {
        async fn insert_product(&self, product: &Product) -> Result<(), RepositoryError> {
            self.saved_skus
                .lock()
                .unwrap()
                .push(product.sku.as_str().to_owned());
            Ok(())
        }

        async fn find_product(&self, _sku: &Sku) -> Result<Option<Product>, RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn list_products(
            &self,
            _include_inactive: bool,
        ) -> Result<Vec<Product>, RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn update_product(&self, _product: &Product) -> Result<(), RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn deactivate_product(&self, _sku: &Sku) -> Result<(), RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn insert_location(&self, _location: &Location) -> Result<(), RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn find_location(&self, _id: &str) -> Result<Option<Location>, RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn list_locations(
            &self,
            _include_inactive: bool,
        ) -> Result<Vec<Location>, RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn update_location(&self, _location: &Location) -> Result<(), RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn deactivate_location(&self, _id: &str) -> Result<(), RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn receive_unit(
            &self,
            _unit: &HandlingUnit,
            _event: &MovementEvent,
        ) -> Result<(), RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn record_movement(&self, _event: &MovementEvent) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn use_case_validates_domain_before_using_repository_port() {
        let repository = Arc::new(FakeRepository::default());
        let use_case = CreateProduct::new(repository.clone());

        let created = use_case
            .execute(" sku-7 ", " Caja", " unidad ")
            .await
            .unwrap();
        assert_eq!(created.sku.as_str(), "SKU-7");
        assert_eq!(repository.saved_skus.lock().unwrap().as_slice(), ["SKU-7"]);

        let error = use_case.execute("SKU CON ESPACIOS", "Caja", "unidad").await;
        assert!(matches!(
            error,
            Err(ApplicationError::Domain(DomainError::InvalidSku))
        ));
        assert_eq!(repository.saved_skus.lock().unwrap().len(), 1);
    }

    #[test]
    fn storage_errors_map_to_safe_user_messages() {
        let error = ApplicationError::Repository(RepositoryError::Storage(
            "detalle interno de sqlite".to_owned(),
        ));
        assert_eq!(
            error.user_message(),
            "No se pudo completar la operación. No se confirmaron cambios."
        );
        assert!(!error.user_message().contains("sqlite"));
        assert_eq!(
            ApplicationError::Domain(DomainError::InvalidLocationHierarchy).user_message(),
            "El nivel y la ubicación padre no forman una jerarquía válida."
        );
        assert_eq!(
            ApplicationError::Repository(RepositoryError::Conflict).user_message(),
            "El SKU, código o identificador ya existe; no se guardaron cambios."
        );
        assert_eq!(
            ApplicationError::Repository(RepositoryError::InUse).user_message(),
            "No se puede desactivar la ubicación mientras tenga dependencias activas u ocupación."
        );
    }

    #[test]
    fn ui_adapter_logs_internal_details_and_returns_safe_message() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-ui-error-{unique}"));
        let log_path = directory.join("logs").join("ariadna.log");
        let log = crate::diagnostic_log::DiagnosticLog::open(&log_path).unwrap();
        let error = ApplicationError::Repository(RepositoryError::Storage(
            "sqlite busy on transaction".to_owned(),
        ));

        let message = map_error_for_ui(&log, "product.create", &error);

        assert_eq!(
            message,
            "No se pudo completar la operación. No se confirmaron cambios."
        );
        assert!(!message.contains("sqlite"));
        let log_contents = std::fs::read_to_string(&log_path).unwrap();
        assert!(log_contents.contains("product.create"));
        assert!(log_contents.contains("sqlite busy on transaction"));
        drop(log);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
