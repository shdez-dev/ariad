use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{
    Alert, AlertIntent, DomainError, HandlingUnit, HandlingUnitLine, Location, LocationKind,
    MovementCorrection, MovementEvent, MovementKind, Product, Sku,
};

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RepositoryError {
    #[error("el registro ya existe")]
    Conflict,
    #[error("el registro relacionado no existe")]
    NotFound,
    #[error("el identificador del evento ya existe con otro contenido")]
    EventIdConflict,
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
    #[error("la posición de destino ya está ocupada")]
    PositionOccupied,
    #[error("el destino no es una posición activa")]
    InvalidDestination,
    #[error("el evento de origen de la corrección no existe o pertenece a otra unidad")]
    CorrectionReferenceMismatch,
    #[error("la corrección no puede tener una fecha anterior al evento original")]
    CorrectionBeforeOriginal,
    #[error("la alerta no admite el cambio solicitado")]
    InvalidAlertTransition,
    #[error("error de persistencia: {0}")]
    Storage(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventOutcome {
    Applied,
    Duplicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovementHistoryEntry {
    pub record_id: String,
    pub event_id: String,
    pub handling_unit_id: String,
    pub kind: String,
    pub origin_location_id: Option<String>,
    pub destination_location_id: Option<String>,
    pub occurred_at_ms: i64,
    pub recorded_at_ms: i64,
    pub source: String,
    pub result: String,
    pub rejection_reason: Option<String>,
    pub corrects_event_id: Option<String>,
    pub correction_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventorySearchKind {
    Product,
    HandlingUnit,
    Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventorySearchResult {
    pub kind: InventorySearchKind,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertEntry {
    pub alert: Alert,
    pub handling_unit_id: String,
    pub handling_unit_label: Option<String>,
    pub event_kind: String,
    pub event_result: String,
    pub event_rejection_reason: Option<String>,
    pub source: String,
    pub occurred_at_ms: i64,
    pub origin_location_code: Option<String>,
    pub destination_location_code: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MovementRecordResult {
    Applied,
    Duplicate,
    Rejected(RepositoryError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovementRecordOutcome {
    pub result: MovementRecordResult,
    pub record_id: String,
    pub alert_id: Option<String>,
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

    async fn find_handling_unit(
        &self,
        id_or_label: &str,
    ) -> Result<Option<HandlingUnit>, RepositoryError>;

    async fn list_handling_units(
        &self,
        include_dispatched: bool,
    ) -> Result<Vec<HandlingUnit>, RepositoryError>;

    async fn receive_unit(
        &self,
        unit: &HandlingUnit,
        event: &MovementEvent,
    ) -> Result<EventOutcome, RepositoryError>;

    async fn record_movement(&self, event: &MovementEvent)
    -> Result<EventOutcome, RepositoryError>;

    async fn record_movement_with_alert(
        &self,
        event: &MovementEvent,
        intent: &AlertIntent,
    ) -> Result<MovementRecordOutcome, RepositoryError> {
        let _ = (event, intent);
        Err(RepositoryError::Storage(
            "el repositorio no admite alertas atómicas".to_owned(),
        ))
    }

    async fn record_correction(
        &self,
        correction: &MovementCorrection,
    ) -> Result<EventOutcome, RepositoryError>;

    async fn list_movement_history(
        &self,
        handling_unit_id: &str,
    ) -> Result<Vec<MovementHistoryEntry>, RepositoryError>;

    async fn list_alerts(&self) -> Result<Vec<AlertEntry>, RepositoryError>;

    async fn mark_alert_reviewed(&self, alert_id: &str) -> Result<AlertEntry, RepositoryError>;
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
            Self::Repository(RepositoryError::EventIdConflict) => {
                "El identificador del evento ya se usó con datos distintos; no se aplicaron cambios."
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
            Self::Repository(RepositoryError::PositionOccupied) => {
                "La posición de destino ya está ocupada; no se aplicaron cambios."
            }
            Self::Repository(RepositoryError::InvalidDestination) => {
                "El destino debe ser una posición existente y activa."
            }
            Self::Repository(RepositoryError::CorrectionReferenceMismatch) => {
                "La corrección debe apuntar a un evento de la misma unidad."
            }
            Self::Repository(RepositoryError::CorrectionBeforeOriginal) => {
                "La fecha de la corrección debe ser igual o posterior al evento original."
            }
            Self::Repository(RepositoryError::InvalidAlertTransition) => {
                "La alerta ya está resuelta y no se puede marcar como revisada."
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

pub struct GetHandlingUnit {
    repository: Arc<dyn InventoryRepository>,
}

impl GetHandlingUnit {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        id_or_label: &str,
    ) -> Result<Option<HandlingUnit>, ApplicationError> {
        let id_or_label = id_or_label.trim();
        if id_or_label.is_empty() {
            return Err(DomainError::EmptyField("identificador o etiqueta de unidad").into());
        }
        Ok(self.repository.find_handling_unit(id_or_label).await?)
    }
}

pub struct ListHandlingUnits {
    repository: Arc<dyn InventoryRepository>,
}

impl ListHandlingUnits {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        include_dispatched: bool,
    ) -> Result<Vec<HandlingUnit>, ApplicationError> {
        Ok(self
            .repository
            .list_handling_units(include_dispatched)
            .await?)
    }
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
    ) -> Result<EventOutcome, ApplicationError> {
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
        Ok(self.repository.receive_unit(&unit, &event).await?)
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
    ) -> Result<EventOutcome, ApplicationError> {
        let event = MovementEvent::new(
            event_id,
            handling_unit_id,
            MovementKind::Dispatched,
            Some(origin_location_id),
            None::<&str>,
            occurred_at_ms,
            source,
        )?;
        Ok(self.repository.record_movement(&event).await?)
    }
}

pub struct StoreUnit {
    repository: Arc<dyn InventoryRepository>,
}

impl StoreUnit {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        event_id: &str,
        handling_unit_id: &str,
        destination_location_id: &str,
        occurred_at_ms: i64,
        source: &str,
    ) -> Result<EventOutcome, ApplicationError> {
        let event = MovementEvent::new(
            event_id,
            handling_unit_id,
            MovementKind::Stored,
            None::<&str>,
            Some(destination_location_id),
            occurred_at_ms,
            source,
        )?;
        Ok(self.repository.record_movement(&event).await?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_with_alert(
        &self,
        event_id: &str,
        handling_unit_id: &str,
        destination_location_id: &str,
        occurred_at_ms: i64,
        source: &str,
        alert_intent: &AlertIntent,
    ) -> Result<MovementRecordOutcome, ApplicationError> {
        let event = MovementEvent::new(
            event_id,
            handling_unit_id,
            MovementKind::Stored,
            None::<&str>,
            Some(destination_location_id),
            occurred_at_ms,
            source,
        )?;
        Ok(self
            .repository
            .record_movement_with_alert(&event, alert_intent)
            .await?)
    }
}

pub struct MoveUnit {
    repository: Arc<dyn InventoryRepository>,
}

impl MoveUnit {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        event_id: &str,
        handling_unit_id: &str,
        origin_location_id: &str,
        destination_location_id: &str,
        occurred_at_ms: i64,
        source: &str,
    ) -> Result<EventOutcome, ApplicationError> {
        let event = MovementEvent::new(
            event_id,
            handling_unit_id,
            MovementKind::Moved,
            Some(origin_location_id),
            Some(destination_location_id),
            occurred_at_ms,
            source,
        )?;
        Ok(self.repository.record_movement(&event).await?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_with_alert(
        &self,
        event_id: &str,
        handling_unit_id: &str,
        origin_location_id: &str,
        destination_location_id: &str,
        occurred_at_ms: i64,
        source: &str,
        alert_intent: &AlertIntent,
    ) -> Result<MovementRecordOutcome, ApplicationError> {
        let event = MovementEvent::new(
            event_id,
            handling_unit_id,
            MovementKind::Moved,
            Some(origin_location_id),
            Some(destination_location_id),
            occurred_at_ms,
            source,
        )?;
        Ok(self
            .repository
            .record_movement_with_alert(&event, alert_intent)
            .await?)
    }
}

pub struct RecordCorrection {
    repository: Arc<dyn InventoryRepository>,
}

impl RecordCorrection {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        correction_id: &str,
        handling_unit_id: &str,
        corrects_event_id: &str,
        reason: &str,
        occurred_at_ms: i64,
        source: &str,
    ) -> Result<EventOutcome, ApplicationError> {
        let correction = MovementCorrection::new(
            correction_id,
            handling_unit_id,
            corrects_event_id,
            reason,
            occurred_at_ms,
            source,
        )?;
        Ok(self.repository.record_correction(&correction).await?)
    }
}

pub struct GetMovementHistory {
    repository: Arc<dyn InventoryRepository>,
}

pub struct SearchInventory {
    repository: Arc<dyn InventoryRepository>,
}

impl SearchInventory {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        query: &str,
    ) -> Result<Vec<InventorySearchResult>, ApplicationError> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let products = self.repository.list_products(false).await?;
        let units = self.repository.list_handling_units(true).await?;
        let locations = self.repository.list_locations(false).await?;
        let mut results = Vec::new();

        for product in products {
            if contains_term(
                &query,
                [
                    product.sku.as_str(),
                    product.description.as_str(),
                    product.unit_of_measure.as_str(),
                ],
            ) {
                results.push(InventorySearchResult {
                    kind: InventorySearchKind::Product,
                    id: product.sku.as_str().to_owned(),
                    title: product.sku.as_str().to_owned(),
                    subtitle: format!("{} · {}", product.description, product.unit_of_measure),
                    status: "active".to_owned(),
                });
            }
        }

        for unit in units {
            let location = unit
                .current_location_id
                .as_deref()
                .and_then(|id| locations.iter().find(|location| location.id == id));
            let line_matches: Vec<&str> = unit.lines.iter().map(|line| line.sku.as_str()).collect();
            let mut searchable = vec![unit.id.as_str(), unit.label.as_str()];
            if let Some(location_id) = unit.current_location_id.as_deref() {
                searchable.push(location_id);
            }
            if let Some(location) = location {
                searchable.push(location.code.as_str());
                searchable.push(location.name.as_str());
            }
            searchable.extend(line_matches);
            if contains_term(&query, searchable) {
                let location_text = location
                    .map(|location| format!("{} · {}", location.code, location.name))
                    .unwrap_or_else(|| "Sin ubicación".to_owned());
                let skus = unit
                    .lines
                    .iter()
                    .map(|line| line.sku.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                results.push(InventorySearchResult {
                    kind: InventorySearchKind::HandlingUnit,
                    id: unit.id,
                    title: unit.label,
                    subtitle: format!("{} · {}", location_text, skus),
                    status: unit.status.as_str().to_owned(),
                });
            }
        }

        for location in locations {
            if contains_term(
                &query,
                [
                    location.id.as_str(),
                    location.code.as_str(),
                    location.name.as_str(),
                ],
            ) {
                results.push(InventorySearchResult {
                    kind: InventorySearchKind::Location,
                    id: location.id,
                    title: location.code,
                    subtitle: format!("{} · {}", location.name, location.kind.as_str()),
                    status: "active".to_owned(),
                });
            }
        }

        Ok(results)
    }
}

fn contains_term<'a>(query: &str, values: impl IntoIterator<Item = &'a str>) -> bool {
    values
        .into_iter()
        .any(|value| value.to_lowercase().contains(query))
}

pub struct ListAlerts {
    repository: Arc<dyn InventoryRepository>,
}

impl ListAlerts {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self) -> Result<Vec<AlertEntry>, ApplicationError> {
        Ok(self.repository.list_alerts().await?)
    }
}

pub struct MarkAlertReviewed {
    repository: Arc<dyn InventoryRepository>,
}

impl MarkAlertReviewed {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, alert_id: &str) -> Result<AlertEntry, ApplicationError> {
        let alert_id = alert_id.trim();
        if alert_id.is_empty() {
            return Err(DomainError::EmptyField("identificador de alerta").into());
        }
        Ok(self.repository.mark_alert_reviewed(alert_id).await?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimulationScenario {
    Normal,
    Duplicate,
    UnknownLabel,
    InvalidLocation,
    OccupiedDestination,
}

impl SimulationScenario {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Duplicate => "duplicate",
            Self::UnknownLabel => "unknown_label",
            Self::InvalidLocation => "invalid_location",
            Self::OccupiedDestination => "occupied_destination",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationStep {
    pub id: String,
    pub label: String,
    pub result: String,
    pub message: String,
    pub record_id: Option<String>,
    pub event_id: Option<String>,
    pub handling_unit_id: Option<String>,
    pub alert_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationResponse {
    pub seed: String,
    pub scenario: SimulationScenario,
    pub steps: Vec<SimulationStep>,
    pub summary: String,
}

pub struct RunSimulation {
    repository: Arc<dyn InventoryRepository>,
}

const SIMULATED_PRODUCT_DESCRIPTION: &str = "Producto sintético de Ariadna";
const SIMULATED_PRODUCT_UNIT: &str = "unidad";

impl RunSimulation {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        seed: &str,
        scenario: SimulationScenario,
    ) -> Result<SimulationResponse, ApplicationError> {
        let seed = seed.trim();
        if seed.is_empty() {
            return Err(DomainError::EmptyField("semilla de simulación").into());
        }
        if seed.len() > 128 {
            return Err(DomainError::FieldTooLong("semilla de simulación").into());
        }

        let seed = seed.to_owned();
        let seed_hash = stable_seed_hash(&seed);
        let run = SimulationRunContext {
            seed_hash,
            base_time_ms: 1_800_000_000_000 + (seed_hash % 1_000_000) as i64 * 1_000,
        };
        let catalog = self.ensure_catalog(&run).await?;
        let steps = match scenario {
            SimulationScenario::Normal => self.run_normal(&run, &catalog).await?,
            SimulationScenario::Duplicate => self.run_duplicate(&run, &catalog).await?,
            SimulationScenario::UnknownLabel => self.run_unknown_label(&run, &catalog).await?,
            SimulationScenario::InvalidLocation => {
                self.run_invalid_location(&run, &catalog).await?
            }
            SimulationScenario::OccupiedDestination => {
                self.run_occupied_destination(&run, &catalog).await?
            }
        };

        let summary = match scenario {
            SimulationScenario::Normal => {
                "Recorrido normal de recepción, ubicación, traslado y despacho completado."
            }
            SimulationScenario::Duplicate => {
                "Se registró la lectura duplicada y su alerta quedó enlazada al intento exacto."
            }
            SimulationScenario::UnknownLabel => {
                "La etiqueta desconocida se rechazó y quedó asociada a una alerta revisable."
            }
            SimulationScenario::InvalidLocation => {
                "La ubicación inválida se rechazó y quedó asociada a una alerta revisable."
            }
            SimulationScenario::OccupiedDestination => {
                "El destino ocupado se rechazó y quedó asociado a una alerta revisable."
            }
        }
        .to_owned();

        Ok(SimulationResponse {
            seed,
            scenario,
            steps,
            summary,
        })
    }

    async fn ensure_catalog(
        &self,
        run: &SimulationRunContext,
    ) -> Result<SimulationCatalog, ApplicationError> {
        let prefix = run.prefix();
        let sku = format!("SIM.{:016X}", run.seed_hash);
        let product = GetProduct::new(self.repository.clone())
            .execute(&sku)
            .await?;
        match product {
            Some(product)
                if product.description != SIMULATED_PRODUCT_DESCRIPTION
                    || product.unit_of_measure != SIMULATED_PRODUCT_UNIT =>
            {
                return Err(RepositoryError::Conflict.into());
            }
            Some(product) if product.active => {}
            Some(_) => return Err(RepositoryError::InactiveProduct.into()),
            None => {
                CreateProduct::new(self.repository.clone())
                    .execute(&sku, SIMULATED_PRODUCT_DESCRIPTION, SIMULATED_PRODUCT_UNIT)
                    .await?;
            }
        }

        let zone_id = format!("{prefix}-zone");
        let aisle_id = format!("{prefix}-aisle");
        let rack_id = format!("{prefix}-rack");
        let first_position_id = format!("{prefix}-position-a");
        let second_position_id = format!("{prefix}-position-b");
        self.ensure_location(
            &zone_id,
            &format!("SZ{:016X}", run.seed_hash),
            "Zona sintética",
            LocationKind::Zone,
            None,
        )
        .await?;
        self.ensure_location(
            &aisle_id,
            &format!("SA{:016X}", run.seed_hash),
            "Pasillo sintético",
            LocationKind::Aisle,
            Some(&zone_id),
        )
        .await?;
        self.ensure_location(
            &rack_id,
            &format!("SR{:016X}", run.seed_hash),
            "Rack sintético",
            LocationKind::Rack,
            Some(&aisle_id),
        )
        .await?;
        self.ensure_location(
            &first_position_id,
            &format!("SP{:016X}A", run.seed_hash),
            "Posición sintética A",
            LocationKind::Position,
            Some(&rack_id),
        )
        .await?;
        self.ensure_location(
            &second_position_id,
            &format!("SP{:016X}B", run.seed_hash),
            "Posición sintética B",
            LocationKind::Position,
            Some(&rack_id),
        )
        .await?;

        Ok(SimulationCatalog {
            sku,
            rack_id,
            first_position_id,
            second_position_id,
        })
    }

    async fn ensure_location(
        &self,
        id: &str,
        code: &str,
        name: &str,
        kind: LocationKind,
        parent_id: Option<&str>,
    ) -> Result<(), ApplicationError> {
        match GetLocation::new(self.repository.clone())
            .execute(id)
            .await?
        {
            Some(location)
                if location.active
                    && location.kind == kind
                    && location.code == code
                    && location.name == name
                    && location.parent_id.as_deref() == parent_id =>
            {
                Ok(())
            }
            Some(_) => Err(RepositoryError::Conflict.into()),
            None => {
                CreateLocation::new(self.repository.clone())
                    .execute(id, code, name, kind, parent_id)
                    .await?;
                Ok(())
            }
        }
    }

    async fn run_normal(
        &self,
        run: &SimulationRunContext,
        catalog: &SimulationCatalog,
    ) -> Result<Vec<SimulationStep>, ApplicationError> {
        let unit_id = run.id("normal-unit");
        let label = run.label("NORMAL");
        let mut steps = Vec::new();
        steps.push(
            self.receive_step(run, catalog, "normal-receive", &unit_id, &label, 1)
                .await?,
        );
        steps.push(
            self.store_step(run, "normal-store", &unit_id, &catalog.first_position_id, 2)
                .await?,
        );
        steps.push(
            self.move_step(
                run,
                "normal-move",
                &unit_id,
                &catalog.first_position_id,
                &catalog.second_position_id,
                3,
            )
            .await?,
        );
        steps.push(
            self.dispatch_step(
                run,
                "normal-dispatch",
                &unit_id,
                &catalog.second_position_id,
                4,
            )
            .await?,
        );
        Ok(steps)
    }

    async fn run_duplicate(
        &self,
        run: &SimulationRunContext,
        catalog: &SimulationCatalog,
    ) -> Result<Vec<SimulationStep>, ApplicationError> {
        let unit_id = run.id("duplicate-unit");
        let label = run.label("DUPLICATE");
        let mut steps = Vec::new();
        steps.push(
            self.receive_step(run, catalog, "duplicate-receive", &unit_id, &label, 1)
                .await?,
        );
        steps.push(
            self.store_step(
                run,
                "duplicate-store",
                &unit_id,
                &catalog.first_position_id,
                2,
            )
            .await?,
        );
        let event_id = run.id("duplicate-move");
        let first = MoveUnit::new(self.repository.clone())
            .execute(
                &event_id,
                &unit_id,
                &catalog.first_position_id,
                &catalog.second_position_id,
                run.time(3),
                "simulator",
            )
            .await?;
        steps.push(step_from_event_outcome(
            "duplicate-first",
            "Primer traslado",
            &event_id,
            &unit_id,
            first,
        ));
        let intent = run.alert_intent("duplicate", "Lectura duplicada", run.time(4))?;
        let duplicate = MoveUnit::new(self.repository.clone())
            .execute_with_alert(
                &event_id,
                &unit_id,
                &catalog.first_position_id,
                &catalog.second_position_id,
                run.time(3),
                "simulator",
                &intent,
            )
            .await?;
        steps.push(step_from_record_outcome(
            "duplicate-retry",
            "Reintento duplicado",
            &event_id,
            &unit_id,
            duplicate,
        ));
        Ok(steps)
    }

    async fn run_unknown_label(
        &self,
        run: &SimulationRunContext,
        catalog: &SimulationCatalog,
    ) -> Result<Vec<SimulationStep>, ApplicationError> {
        let unknown_ref = run.id("unknown-label");
        let event_id = run.id("unknown-label-read");
        let intent = run.alert_intent("unknown-label", "Etiqueta desconocida", run.time(1))?;
        let result = StoreUnit::new(self.repository.clone())
            .execute_with_alert(
                &event_id,
                &unknown_ref,
                &catalog.first_position_id,
                run.time(1),
                "simulator",
                &intent,
            )
            .await?;
        Ok(vec![step_from_record_outcome(
            "unknown-label",
            "Lectura de etiqueta desconocida",
            &event_id,
            &unknown_ref,
            result,
        )])
    }

    async fn run_invalid_location(
        &self,
        run: &SimulationRunContext,
        catalog: &SimulationCatalog,
    ) -> Result<Vec<SimulationStep>, ApplicationError> {
        let unit_id = run.id("invalid-location-unit");
        let label = run.label("INVALID");
        let mut steps = vec![
            self.receive_step(
                run,
                catalog,
                "invalid-location-receive",
                &unit_id,
                &label,
                1,
            )
            .await?,
        ];
        let event_id = run.id("invalid-location-store");
        let intent = run.alert_intent("invalid-location", "Posición inválida", run.time(2))?;
        let result = StoreUnit::new(self.repository.clone())
            .execute_with_alert(
                &event_id,
                &unit_id,
                &catalog.rack_id,
                run.time(2),
                "simulator",
                &intent,
            )
            .await?;
        steps.push(step_from_record_outcome(
            "invalid-location",
            "Asignación a una ubicación que no es posición",
            &event_id,
            &unit_id,
            result,
        ));
        Ok(steps)
    }

    async fn run_occupied_destination(
        &self,
        run: &SimulationRunContext,
        catalog: &SimulationCatalog,
    ) -> Result<Vec<SimulationStep>, ApplicationError> {
        let blocker_id = run.id("occupied-blocker");
        let blocker_label = run.label("BLOCKER");
        let probe_id = run.id("occupied-probe");
        let probe_label = run.label("PROBE");
        let mut steps = vec![
            self.receive_step(
                run,
                catalog,
                "occupied-blocker-receive",
                &blocker_id,
                &blocker_label,
                1,
            )
            .await?,
            self.store_step(
                run,
                "occupied-blocker-store",
                &blocker_id,
                &catalog.first_position_id,
                2,
            )
            .await?,
            self.receive_step(
                run,
                catalog,
                "occupied-probe-receive",
                &probe_id,
                &probe_label,
                3,
            )
            .await?,
        ];
        let event_id = run.id("occupied-probe-store");
        let intent = run.alert_intent("occupied-destination", "Destino ocupado", run.time(4))?;
        let result = StoreUnit::new(self.repository.clone())
            .execute_with_alert(
                &event_id,
                &probe_id,
                &catalog.first_position_id,
                run.time(4),
                "simulator",
                &intent,
            )
            .await?;
        steps.push(step_from_record_outcome(
            "occupied-destination",
            "Asignación a un destino ocupado",
            &event_id,
            &probe_id,
            result,
        ));
        Ok(steps)
    }

    async fn receive_step(
        &self,
        run: &SimulationRunContext,
        catalog: &SimulationCatalog,
        step_id: &str,
        unit_id: &str,
        label: &str,
        time_offset: i64,
    ) -> Result<SimulationStep, ApplicationError> {
        let event_id = run.id(step_id);
        let outcome = ReceiveUnit::new(self.repository.clone())
            .execute(
                unit_id,
                label,
                vec![ReceiveLine {
                    sku: catalog.sku.clone(),
                    quantity: 4,
                    lot_code: Some("SIM-LOT".to_owned()),
                }],
                &event_id,
                run.time(time_offset),
                "simulator",
            )
            .await?;
        Ok(step_from_event_outcome(
            step_id,
            "Recepción",
            &event_id,
            unit_id,
            outcome,
        ))
    }

    async fn store_step(
        &self,
        run: &SimulationRunContext,
        step_id: &str,
        unit_id: &str,
        destination: &str,
        time_offset: i64,
    ) -> Result<SimulationStep, ApplicationError> {
        let event_id = run.id(step_id);
        let outcome = StoreUnit::new(self.repository.clone())
            .execute(
                &event_id,
                unit_id,
                destination,
                run.time(time_offset),
                "simulator",
            )
            .await?;
        Ok(step_from_event_outcome(
            step_id,
            "Asignación inicial",
            &event_id,
            unit_id,
            outcome,
        ))
    }

    async fn move_step(
        &self,
        run: &SimulationRunContext,
        step_id: &str,
        unit_id: &str,
        origin: &str,
        destination: &str,
        time_offset: i64,
    ) -> Result<SimulationStep, ApplicationError> {
        let event_id = run.id(step_id);
        let outcome = MoveUnit::new(self.repository.clone())
            .execute(
                &event_id,
                unit_id,
                origin,
                destination,
                run.time(time_offset),
                "simulator",
            )
            .await?;
        Ok(step_from_event_outcome(
            step_id, "Traslado", &event_id, unit_id, outcome,
        ))
    }

    async fn dispatch_step(
        &self,
        run: &SimulationRunContext,
        step_id: &str,
        unit_id: &str,
        origin: &str,
        time_offset: i64,
    ) -> Result<SimulationStep, ApplicationError> {
        let event_id = run.id(step_id);
        let outcome = DispatchUnit::new(self.repository.clone())
            .execute(
                &event_id,
                unit_id,
                origin,
                run.time(time_offset),
                "simulator",
            )
            .await?;
        Ok(step_from_event_outcome(
            step_id, "Despacho", &event_id, unit_id, outcome,
        ))
    }
}

struct SimulationRunContext {
    seed_hash: u64,
    base_time_ms: i64,
}

impl SimulationRunContext {
    fn prefix(&self) -> String {
        format!("sim-{:016x}", self.seed_hash)
    }

    fn id(&self, suffix: &str) -> String {
        format!("{}-{suffix}", self.prefix())
    }

    fn label(&self, suffix: &str) -> String {
        format!("SIM-{:016X}-{suffix}", self.seed_hash)
    }

    fn time(&self, offset: i64) -> i64 {
        self.base_time_ms + offset
    }

    fn alert_intent(
        &self,
        suffix: &str,
        kind: &str,
        created_at_ms: i64,
    ) -> Result<AlertIntent, DomainError> {
        AlertIntent::new(self.id(&format!("alert-{suffix}")), kind, created_at_ms)
    }
}

struct SimulationCatalog {
    sku: String,
    rack_id: String,
    first_position_id: String,
    second_position_id: String,
}

fn stable_seed_hash(seed: &str) -> u64 {
    seed.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn step_from_event_outcome(
    id: &str,
    label: &str,
    event_id: &str,
    handling_unit_id: &str,
    outcome: EventOutcome,
) -> SimulationStep {
    let (result, message, record_id) = match outcome {
        EventOutcome::Applied => (
            "applied",
            "El movimiento fue aplicado y persistido.",
            Some(event_id.to_owned()),
        ),
        EventOutcome::Duplicate => (
            "duplicate",
            "El evento ya estaba registrado; no se repitieron sus efectos.",
            None,
        ),
    };
    SimulationStep {
        id: id.to_owned(),
        label: label.to_owned(),
        result: result.to_owned(),
        message: message.to_owned(),
        record_id,
        event_id: Some(event_id.to_owned()),
        handling_unit_id: Some(handling_unit_id.to_owned()),
        alert_id: None,
    }
}

fn step_from_record_outcome(
    id: &str,
    label: &str,
    event_id: &str,
    handling_unit_id: &str,
    outcome: MovementRecordOutcome,
) -> SimulationStep {
    let (result, message) = match outcome.result {
        MovementRecordResult::Applied => ("applied", "El movimiento fue aplicado y persistido."),
        MovementRecordResult::Duplicate => (
            "duplicate",
            "La lectura duplicada se registró sin repetir sus efectos.",
        ),
        MovementRecordResult::Rejected(error) => (
            "rejected",
            ApplicationError::Repository(error).user_message(),
        ),
    };
    SimulationStep {
        id: id.to_owned(),
        label: label.to_owned(),
        result: result.to_owned(),
        message: message.to_owned(),
        record_id: Some(outcome.record_id),
        event_id: Some(event_id.to_owned()),
        handling_unit_id: Some(handling_unit_id.to_owned()),
        alert_id: outcome.alert_id,
    }
}

impl GetMovementHistory {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        handling_unit_id: &str,
    ) -> Result<Vec<MovementHistoryEntry>, ApplicationError> {
        let handling_unit_id = handling_unit_id.trim();
        if handling_unit_id.is_empty() {
            return Err(DomainError::EmptyField("unidad de manipulación").into());
        }
        Ok(self
            .repository
            .list_movement_history(handling_unit_id)
            .await?)
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

        async fn find_handling_unit(
            &self,
            _id_or_label: &str,
        ) -> Result<Option<HandlingUnit>, RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn list_handling_units(
            &self,
            _include_dispatched: bool,
        ) -> Result<Vec<HandlingUnit>, RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn receive_unit(
            &self,
            _unit: &HandlingUnit,
            _event: &MovementEvent,
        ) -> Result<EventOutcome, RepositoryError> {
            Err(RepositoryError::Storage("not used by this fake".to_owned()))
        }

        async fn record_movement(
            &self,
            _event: &MovementEvent,
        ) -> Result<EventOutcome, RepositoryError> {
            Ok(EventOutcome::Applied)
        }

        async fn record_correction(
            &self,
            _correction: &MovementCorrection,
        ) -> Result<EventOutcome, RepositoryError> {
            Ok(EventOutcome::Applied)
        }

        async fn list_movement_history(
            &self,
            _handling_unit_id: &str,
        ) -> Result<Vec<MovementHistoryEntry>, RepositoryError> {
            Ok(Vec::new())
        }

        async fn list_alerts(&self) -> Result<Vec<AlertEntry>, RepositoryError> {
            Ok(Vec::new())
        }

        async fn mark_alert_reviewed(
            &self,
            _alert_id: &str,
        ) -> Result<AlertEntry, RepositoryError> {
            Err(RepositoryError::NotFound)
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
