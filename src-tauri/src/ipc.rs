use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    application::{
        AlertEntry, ApplicationError, CreateLocation, CreateProduct, DeactivateLocation,
        DeactivateProduct, DispatchUnit, EventOutcome, GetHandlingUnit, GetLocation,
        GetMovementHistory, GetProduct, InventorySearchKind, InventorySearchResult, ListAlerts,
        ListHandlingUnits, ListLocations, ListProducts, MarkAlertReviewed, MoveUnit, ReceiveLine,
        ReceiveUnit, RecordCorrection, RunSimulation, SearchInventory, SimulationScenario,
        SimulationStep, StoreUnit, UpdateLocation, UpdateProduct, map_error_for_ui,
    },
    diagnostic_log::DiagnosticLog,
    domain::{
        AlertRecordTarget, AlertStatus, DomainError, HandlingUnit, Location, LocationKind, Product,
    },
    state::AppState,
};

const DESKTOP_SOURCE: &str = "desktop_ui";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocationKindInput {
    Zone,
    Aisle,
    Rack,
    Position,
}

impl From<LocationKindInput> for LocationKind {
    fn from(value: LocationKindInput) -> Self {
        match value {
            LocationKindInput::Zone => Self::Zone,
            LocationKindInput::Aisle => Self::Aisle,
            LocationKindInput::Rack => Self::Rack,
            LocationKindInput::Position => Self::Position,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveLineInput {
    pub sku: String,
    pub quantity: u32,
    pub lot_code: Option<String>,
}

impl From<ReceiveLineInput> for ReceiveLine {
    fn from(value: ReceiveLineInput) -> Self {
        Self {
            sku: value.sku,
            quantity: value.quantity,
            lot_code: value.lot_code,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProductResponse {
    pub sku: String,
    pub description: String,
    pub unit_of_measure: String,
    pub active: bool,
}

impl From<Product> for ProductResponse {
    fn from(value: Product) -> Self {
        Self {
            sku: value.sku.as_str().to_owned(),
            description: value.description,
            unit_of_measure: value.unit_of_measure,
            active: value.active,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocationResponse {
    pub id: String,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub parent_id: Option<String>,
    pub active: bool,
}

impl From<Location> for LocationResponse {
    fn from(value: Location) -> Self {
        Self {
            id: value.id,
            code: value.code,
            name: value.name,
            kind: value.kind.as_str().to_owned(),
            parent_id: value.parent_id,
            active: value.active,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HandlingUnitLineResponse {
    pub sku: String,
    pub quantity: u32,
    pub lot_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HandlingUnitResponse {
    pub id: String,
    pub label: String,
    pub status: String,
    pub current_location_id: Option<String>,
    pub lines: Vec<HandlingUnitLineResponse>,
}

impl From<HandlingUnit> for HandlingUnitResponse {
    fn from(value: HandlingUnit) -> Self {
        Self {
            id: value.id,
            label: value.label,
            status: value.status.as_str().to_owned(),
            current_location_id: value.current_location_id,
            lines: value
                .lines
                .into_iter()
                .map(|line| HandlingUnitLineResponse {
                    sku: line.sku.as_str().to_owned(),
                    quantity: line.quantity,
                    lot_code: line.lot_code,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MovementOutcomeResponse {
    Applied,
    Duplicate,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MovementWriteResponse {
    pub event_id: String,
    pub outcome: MovementOutcomeResponse,
}

impl MovementWriteResponse {
    fn new(event_id: String, outcome: EventOutcome) -> Self {
        Self {
            event_id,
            outcome: match outcome {
                EventOutcome::Applied => MovementOutcomeResponse::Applied,
                EventOutcome::Duplicate => MovementOutcomeResponse::Duplicate,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionWriteResponse {
    pub correction_id: String,
    pub outcome: MovementOutcomeResponse,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MovementHistoryResponse {
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

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InventorySearchKindResponse {
    Product,
    HandlingUnit,
    Location,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InventorySearchResponse {
    pub kind: InventorySearchKindResponse,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub status: String,
}

impl From<InventorySearchResult> for InventorySearchResponse {
    fn from(value: InventorySearchResult) -> Self {
        Self {
            kind: match value.kind {
                InventorySearchKind::Product => InventorySearchKindResponse::Product,
                InventorySearchKind::HandlingUnit => InventorySearchKindResponse::HandlingUnit,
                InventorySearchKind::Location => InventorySearchKindResponse::Location,
            },
            id: value.id,
            title: value.title,
            subtitle: value.subtitle,
            status: value.status,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertStatusResponse {
    Open,
    Reviewed,
    Resolved,
}

impl From<AlertStatus> for AlertStatusResponse {
    fn from(value: AlertStatus) -> Self {
        match value {
            AlertStatus::Open => Self::Open,
            AlertStatus::Reviewed => Self::Reviewed,
            AlertStatus::Resolved => Self::Resolved,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertTargetKindResponse {
    MovementEvent,
    MovementAttempt,
}

impl From<&AlertRecordTarget> for AlertTargetKindResponse {
    fn from(value: &AlertRecordTarget) -> Self {
        match value {
            AlertRecordTarget::MovementEvent { .. } => Self::MovementEvent,
            AlertRecordTarget::MovementAttempt { .. } => Self::MovementAttempt,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AlertResponse {
    pub id: String,
    pub movement_event_id: Option<String>,
    pub movement_attempt_id: Option<i64>,
    pub record_id: String,
    pub target_kind: AlertTargetKindResponse,
    pub event_id: String,
    pub kind: String,
    pub status: AlertStatusResponse,
    pub created_at_ms: i64,
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

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SimulationScenarioInput {
    Normal,
    Duplicate,
    UnknownLabel,
    InvalidLocation,
    OccupiedDestination,
}

impl From<SimulationScenarioInput> for SimulationScenario {
    fn from(value: SimulationScenarioInput) -> Self {
        match value {
            SimulationScenarioInput::Normal => Self::Normal,
            SimulationScenarioInput::Duplicate => Self::Duplicate,
            SimulationScenarioInput::UnknownLabel => Self::UnknownLabel,
            SimulationScenarioInput::InvalidLocation => Self::InvalidLocation,
            SimulationScenarioInput::OccupiedDestination => Self::OccupiedDestination,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SimulationStepResponse {
    pub id: String,
    pub label: String,
    pub result: String,
    pub message: String,
    pub record_id: Option<String>,
    pub event_id: Option<String>,
    pub handling_unit_id: Option<String>,
    pub alert_id: Option<String>,
}

impl From<SimulationStep> for SimulationStepResponse {
    fn from(value: SimulationStep) -> Self {
        Self {
            id: value.id,
            label: value.label,
            result: value.result,
            message: value.message,
            record_id: value.record_id,
            event_id: value.event_id,
            handling_unit_id: value.handling_unit_id,
            alert_id: value.alert_id,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SimulationResponse {
    pub seed: String,
    pub scenario: String,
    pub steps: Vec<SimulationStepResponse>,
    pub summary: String,
}

impl From<crate::application::SimulationResponse> for SimulationResponse {
    fn from(value: crate::application::SimulationResponse) -> Self {
        Self {
            seed: value.seed,
            scenario: value.scenario.as_str().to_owned(),
            steps: value.steps.into_iter().map(Into::into).collect(),
            summary: value.summary,
        }
    }
}

impl From<AlertEntry> for AlertResponse {
    fn from(value: AlertEntry) -> Self {
        Self {
            id: value.alert.id,
            movement_event_id: value.alert.target.movement_event_id().map(str::to_owned),
            movement_attempt_id: value.alert.target.movement_attempt_id(),
            record_id: value.alert.target.record_id(),
            target_kind: (&value.alert.target).into(),
            event_id: value.alert.target.event_id().to_owned(),
            kind: value.alert.kind,
            status: value.alert.status.into(),
            created_at_ms: value.alert.created_at_ms,
            handling_unit_id: value.handling_unit_id,
            handling_unit_label: value.handling_unit_label,
            event_kind: value.event_kind,
            event_result: value.event_result,
            event_rejection_reason: value.event_rejection_reason,
            source: value.source,
            occurred_at_ms: value.occurred_at_ms,
            origin_location_code: value.origin_location_code,
            destination_location_code: value.destination_location_code,
        }
    }
}

impl From<crate::application::MovementHistoryEntry> for MovementHistoryResponse {
    fn from(value: crate::application::MovementHistoryEntry) -> Self {
        Self {
            record_id: value.record_id,
            event_id: value.event_id,
            handling_unit_id: value.handling_unit_id,
            kind: value.kind,
            origin_location_id: value.origin_location_id,
            destination_location_id: value.destination_location_id,
            occurred_at_ms: value.occurred_at_ms,
            recorded_at_ms: value.recorded_at_ms,
            source: value.source,
            result: value.result,
            rejection_reason: value.rejection_reason,
            corrects_event_id: value.corrects_event_id,
            correction_reason: value.correction_reason,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCode {
    Validation,
    Conflict,
    NotFound,
    EventIdConflict,
    InactiveProduct,
    InvalidTransition,
    InvalidLocationHierarchy,
    LocationHierarchyImmutable,
    InUse,
    PositionOccupied,
    InvalidDestination,
    CorrectionReferenceMismatch,
    CorrectionBeforeOriginal,
    InvalidAlertTransition,
    Storage,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub code: IpcErrorCode,
    pub message: String,
}

impl IpcError {
    fn from_application_error(
        diagnostic_log: &DiagnosticLog,
        operation: &str,
        error: &ApplicationError,
    ) -> Self {
        let code = match error {
            ApplicationError::Domain(domain_error) => match domain_error {
                DomainError::PositionOccupied => IpcErrorCode::PositionOccupied,
                DomainError::LocationMustBeActivePosition => IpcErrorCode::InvalidDestination,
                DomainError::InvalidLocationHierarchy | DomainError::InactiveLocationParent => {
                    IpcErrorCode::InvalidLocationHierarchy
                }
                _ => IpcErrorCode::Validation,
            },
            ApplicationError::Repository(repository_error) => match repository_error {
                crate::application::RepositoryError::Conflict => IpcErrorCode::Conflict,
                crate::application::RepositoryError::NotFound => IpcErrorCode::NotFound,
                crate::application::RepositoryError::EventIdConflict => {
                    IpcErrorCode::EventIdConflict
                }
                crate::application::RepositoryError::InactiveProduct => {
                    IpcErrorCode::InactiveProduct
                }
                crate::application::RepositoryError::InvalidTransition => {
                    IpcErrorCode::InvalidTransition
                }
                crate::application::RepositoryError::InvalidLocationHierarchy => {
                    IpcErrorCode::InvalidLocationHierarchy
                }
                crate::application::RepositoryError::LocationHierarchyImmutable => {
                    IpcErrorCode::LocationHierarchyImmutable
                }
                crate::application::RepositoryError::InUse => IpcErrorCode::InUse,
                crate::application::RepositoryError::PositionOccupied => {
                    IpcErrorCode::PositionOccupied
                }
                crate::application::RepositoryError::InvalidDestination => {
                    IpcErrorCode::InvalidDestination
                }
                crate::application::RepositoryError::CorrectionReferenceMismatch => {
                    IpcErrorCode::CorrectionReferenceMismatch
                }
                crate::application::RepositoryError::CorrectionBeforeOriginal => {
                    IpcErrorCode::CorrectionBeforeOriginal
                }
                crate::application::RepositoryError::InvalidAlertTransition => {
                    IpcErrorCode::InvalidAlertTransition
                }
                crate::application::RepositoryError::Storage(_) => IpcErrorCode::Storage,
            },
        };
        let message = map_error_for_ui(diagnostic_log, operation, error);
        Self { code, message }
    }
}

fn expose<T>(
    state: &AppState,
    operation: &str,
    result: Result<T, ApplicationError>,
) -> Result<T, IpcError> {
    result
        .map_err(|error| IpcError::from_application_error(&state.diagnostic_log, operation, &error))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn create_product(
    state: State<'_, AppState>,
    sku: String,
    description: String,
    unit_of_measure: String,
) -> Result<ProductResponse, IpcError> {
    let use_case = CreateProduct::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.product.create",
        use_case.execute(&sku, &description, &unit_of_measure).await,
    )
    .map(Into::into)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_product(
    state: State<'_, AppState>,
    sku: String,
) -> Result<Option<ProductResponse>, IpcError> {
    let use_case = GetProduct::new(state.inventory_repository.clone());
    expose(&state, "ipc.product.get", use_case.execute(&sku).await)
        .map(|product| product.map(Into::into))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_products(
    state: State<'_, AppState>,
    include_inactive: bool,
) -> Result<Vec<ProductResponse>, IpcError> {
    let use_case = ListProducts::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.product.list",
        use_case.execute(include_inactive).await,
    )
    .map(|products| products.into_iter().map(Into::into).collect())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn search_inventory(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<InventorySearchResponse>, IpcError> {
    let use_case = SearchInventory::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.inventory.search",
        use_case.execute(&query).await,
    )
    .map(|results| results.into_iter().map(Into::into).collect())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn update_product(
    state: State<'_, AppState>,
    sku: String,
    description: String,
    unit_of_measure: String,
) -> Result<ProductResponse, IpcError> {
    let use_case = UpdateProduct::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.product.update",
        use_case.execute(&sku, &description, &unit_of_measure).await,
    )
    .map(Into::into)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deactivate_product(
    state: State<'_, AppState>,
    sku: String,
) -> Result<ProductResponse, IpcError> {
    let use_case = DeactivateProduct::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.product.deactivate",
        use_case.execute(&sku).await,
    )
    .map(Into::into)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn create_location(
    state: State<'_, AppState>,
    id: String,
    code: String,
    name: String,
    kind: LocationKindInput,
    parent_id: Option<String>,
) -> Result<LocationResponse, IpcError> {
    let use_case = CreateLocation::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.location.create",
        use_case
            .execute(&id, &code, &name, kind.into(), parent_id.as_deref())
            .await,
    )
    .map(Into::into)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_location(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<LocationResponse>, IpcError> {
    let use_case = GetLocation::new(state.inventory_repository.clone());
    expose(&state, "ipc.location.get", use_case.execute(&id).await)
        .map(|location| location.map(Into::into))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_locations(
    state: State<'_, AppState>,
    include_inactive: bool,
) -> Result<Vec<LocationResponse>, IpcError> {
    let use_case = ListLocations::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.location.list",
        use_case.execute(include_inactive).await,
    )
    .map(|locations| locations.into_iter().map(Into::into).collect())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn update_location(
    state: State<'_, AppState>,
    id: String,
    code: String,
    name: String,
) -> Result<LocationResponse, IpcError> {
    let use_case = UpdateLocation::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.location.update",
        use_case.execute(&id, &code, &name).await,
    )
    .map(Into::into)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deactivate_location(
    state: State<'_, AppState>,
    id: String,
) -> Result<LocationResponse, IpcError> {
    let use_case = DeactivateLocation::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.location.deactivate",
        use_case.execute(&id).await,
    )
    .map(Into::into)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_handling_unit(
    state: State<'_, AppState>,
    identifier: String,
) -> Result<Option<HandlingUnitResponse>, IpcError> {
    let use_case = GetHandlingUnit::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.handling_unit.get",
        use_case.execute(&identifier).await,
    )
    .map(|unit| unit.map(Into::into))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_handling_units(
    state: State<'_, AppState>,
    include_dispatched: bool,
) -> Result<Vec<HandlingUnitResponse>, IpcError> {
    let use_case = ListHandlingUnits::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.handling_unit.list",
        use_case.execute(include_dispatched).await,
    )
    .map(|units| units.into_iter().map(Into::into).collect())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn receive_unit(
    state: State<'_, AppState>,
    id: String,
    label: String,
    lines: Vec<ReceiveLineInput>,
    event_id: String,
    occurred_at_ms: i64,
) -> Result<MovementWriteResponse, IpcError> {
    let use_case = ReceiveUnit::new(state.inventory_repository.clone());
    let outcome = expose(
        &state,
        "ipc.movement.receive",
        use_case
            .execute(
                &id,
                &label,
                lines.into_iter().map(Into::into).collect(),
                &event_id,
                occurred_at_ms,
                DESKTOP_SOURCE,
            )
            .await,
    )?;
    Ok(MovementWriteResponse::new(event_id, outcome))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn move_unit(
    state: State<'_, AppState>,
    event_id: String,
    handling_unit_id: String,
    origin_location_id: String,
    destination_location_id: String,
    occurred_at_ms: i64,
) -> Result<MovementWriteResponse, IpcError> {
    let use_case = MoveUnit::new(state.inventory_repository.clone());
    let outcome = expose(
        &state,
        "ipc.movement.move",
        use_case
            .execute(
                &event_id,
                &handling_unit_id,
                &origin_location_id,
                &destination_location_id,
                occurred_at_ms,
                DESKTOP_SOURCE,
            )
            .await,
    )?;
    Ok(MovementWriteResponse::new(event_id, outcome))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn store_unit(
    state: State<'_, AppState>,
    event_id: String,
    handling_unit_id: String,
    destination_location_id: String,
    occurred_at_ms: i64,
) -> Result<MovementWriteResponse, IpcError> {
    let use_case = StoreUnit::new(state.inventory_repository.clone());
    let outcome = expose(
        &state,
        "ipc.movement.store",
        use_case
            .execute(
                &event_id,
                &handling_unit_id,
                &destination_location_id,
                occurred_at_ms,
                DESKTOP_SOURCE,
            )
            .await,
    )?;
    Ok(MovementWriteResponse::new(event_id, outcome))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn dispatch_unit(
    state: State<'_, AppState>,
    event_id: String,
    handling_unit_id: String,
    origin_location_id: String,
    occurred_at_ms: i64,
) -> Result<MovementWriteResponse, IpcError> {
    let use_case = DispatchUnit::new(state.inventory_repository.clone());
    let outcome = expose(
        &state,
        "ipc.movement.dispatch",
        use_case
            .execute(
                &event_id,
                &handling_unit_id,
                &origin_location_id,
                occurred_at_ms,
                DESKTOP_SOURCE,
            )
            .await,
    )?;
    Ok(MovementWriteResponse::new(event_id, outcome))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn record_movement_correction(
    state: State<'_, AppState>,
    correction_id: String,
    handling_unit_id: String,
    corrects_event_id: String,
    reason: String,
    occurred_at_ms: i64,
) -> Result<CorrectionWriteResponse, IpcError> {
    let use_case = RecordCorrection::new(state.inventory_repository.clone());
    let outcome = expose(
        &state,
        "ipc.movement.correct",
        use_case
            .execute(
                &correction_id,
                &handling_unit_id,
                &corrects_event_id,
                &reason,
                occurred_at_ms,
                DESKTOP_SOURCE,
            )
            .await,
    )?;
    Ok(CorrectionWriteResponse {
        correction_id,
        outcome: match outcome {
            EventOutcome::Applied => MovementOutcomeResponse::Applied,
            EventOutcome::Duplicate => MovementOutcomeResponse::Duplicate,
        },
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_movement_history(
    state: State<'_, AppState>,
    handling_unit_id: String,
) -> Result<Vec<MovementHistoryResponse>, IpcError> {
    let use_case = GetMovementHistory::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.movement.history",
        use_case.execute(&handling_unit_id).await,
    )
    .map(|entries| entries.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn list_alerts(state: State<'_, AppState>) -> Result<Vec<AlertResponse>, IpcError> {
    let use_case = ListAlerts::new(state.inventory_repository.clone());
    expose(&state, "ipc.alert.list", use_case.execute().await)
        .map(|alerts| alerts.into_iter().map(Into::into).collect())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn mark_alert_reviewed(
    state: State<'_, AppState>,
    alert_id: String,
) -> Result<AlertResponse, IpcError> {
    let use_case = MarkAlertReviewed::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.alert.review",
        use_case.execute(&alert_id).await,
    )
    .map(Into::into)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn run_simulation(
    state: State<'_, AppState>,
    seed: String,
    scenario: SimulationScenarioInput,
) -> Result<SimulationResponse, IpcError> {
    let use_case = RunSimulation::new(state.inventory_repository.clone());
    expose(
        &state,
        "ipc.simulation.run",
        use_case.execute(&seed, scenario.into()).await,
    )
    .map(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_responses_use_camel_case_and_string_enums() {
        let product = ProductResponse::from(
            Product::new("sku-1", "Caja", "unidad").expect("producto válido"),
        );
        let value = serde_json::to_value(product).unwrap();
        assert_eq!(value["sku"], "SKU-1");
        assert_eq!(value["unitOfMeasure"], "unidad");

        let outcome = serde_json::to_value(MovementWriteResponse::new(
            "event-1".to_owned(),
            EventOutcome::Duplicate,
        ))
        .unwrap();
        assert_eq!(outcome["eventId"], "event-1");
        assert_eq!(outcome["outcome"], "duplicate");

        let correction = serde_json::to_value(CorrectionWriteResponse {
            correction_id: "correction-1".to_owned(),
            outcome: MovementOutcomeResponse::Applied,
        })
        .unwrap();
        assert_eq!(correction["correctionId"], "correction-1");
        assert_eq!(correction["outcome"], "applied");

        let search = InventorySearchResponse::from(InventorySearchResult {
            kind: InventorySearchKind::HandlingUnit,
            id: "unit-1".to_owned(),
            title: "PALLET-1".to_owned(),
            subtitle: "P1 · SKU-1".to_owned(),
            status: "stored".to_owned(),
        });
        let search_json = serde_json::to_value(search).unwrap();
        assert_eq!(search_json["kind"], "handling_unit");
        assert_eq!(search_json["title"], "PALLET-1");

        let alert = AlertResponse::from(AlertEntry {
            alert: crate::domain::Alert::new(
                "alert-1",
                "event-1",
                "manual_review",
                1_800_000_000_000,
            )
            .unwrap(),
            handling_unit_id: "unit-1".to_owned(),
            handling_unit_label: Some("PALLET-1".to_owned()),
            event_kind: "received".to_owned(),
            event_result: "accepted".to_owned(),
            event_rejection_reason: None,
            source: "simulator".to_owned(),
            occurred_at_ms: 1_800_000_000_010,
            origin_location_code: None,
            destination_location_code: None,
        });
        let alert_json = serde_json::to_value(alert).unwrap();
        assert_eq!(alert_json["movementEventId"], "event-1");
        assert_eq!(alert_json["handlingUnitLabel"], "PALLET-1");
        assert_eq!(alert_json["status"], "open");
        assert_eq!(alert_json["recordId"], "event-1");
        assert_eq!(alert_json["targetKind"], "movement_event");
        assert_eq!(alert_json["source"], "simulator");

        let attempt_alert = AlertResponse::from(AlertEntry {
            alert: crate::domain::Alert::from_persisted(
                "alert-attempt",
                AlertRecordTarget::MovementAttempt {
                    attempt_id: 17,
                    event_id: "event-duplicate".to_owned(),
                },
                "duplicate_reading",
                1_800_000_000_020,
                AlertStatus::Open,
            )
            .unwrap(),
            handling_unit_id: "UNKNOWN-LABEL-1".to_owned(),
            handling_unit_label: None,
            event_kind: "stored".to_owned(),
            event_result: "duplicate".to_owned(),
            event_rejection_reason: None,
            source: "simulator".to_owned(),
            occurred_at_ms: 1_800_000_000_019,
            origin_location_code: None,
            destination_location_code: Some("P1".to_owned()),
        });
        let attempt_alert_json = serde_json::to_value(attempt_alert).unwrap();
        assert_eq!(attempt_alert_json["targetKind"], "movement_attempt");
        assert_eq!(
            attempt_alert_json["movementEventId"],
            serde_json::Value::Null
        );
        assert_eq!(attempt_alert_json["movementAttemptId"], 17);
        assert_eq!(attempt_alert_json["recordId"], "attempt:17");
        assert_eq!(attempt_alert_json["eventId"], "event-duplicate");
        assert_eq!(
            attempt_alert_json["handlingUnitLabel"],
            serde_json::Value::Null
        );
        assert_eq!(attempt_alert_json["eventResult"], "duplicate");
        assert_eq!(attempt_alert_json["destinationLocationCode"], "P1");

        let scenario: SimulationScenarioInput =
            serde_json::from_str("\"occupied_destination\"").unwrap();
        assert_eq!(scenario, SimulationScenarioInput::OccupiedDestination);
        let run = SimulationResponse::from(crate::application::SimulationResponse {
            seed: "seed-1".to_owned(),
            scenario: scenario.into(),
            steps: vec![SimulationStep {
                id: "occupied-destination".to_owned(),
                label: "Destino ocupado".to_owned(),
                result: "rejected".to_owned(),
                message: "La posición de destino ya está ocupada.".to_owned(),
                record_id: Some("attempt:17".to_owned()),
                event_id: Some("event-duplicate".to_owned()),
                handling_unit_id: Some("unit-1".to_owned()),
                alert_id: Some("alert-attempt-17".to_owned()),
            }],
            summary: "Escenario terminado.".to_owned(),
        });
        let run_json = serde_json::to_value(run).unwrap();
        assert_eq!(run_json["scenario"], "occupied_destination");
        assert_eq!(run_json["steps"][0]["id"], "occupied-destination");
        assert_eq!(run_json["steps"][0]["recordId"], "attempt:17");
        assert_eq!(run_json["steps"][0]["alertId"], "alert-attempt-17");
    }

    #[test]
    fn ipc_unit_history_and_input_dtos_preserve_camel_case_contract() {
        let unit = HandlingUnit::new(
            "unit-1",
            "PALLET-1",
            vec![crate::domain::HandlingUnitLine::new("sku-1", 2, Some("LOT-1")).unwrap()],
        )
        .unwrap();
        let unit_json = serde_json::to_value(HandlingUnitResponse::from(unit)).unwrap();
        assert_eq!(unit_json["currentLocationId"], serde_json::Value::Null);
        assert_eq!(unit_json["lines"][0]["lotCode"], "LOT-1");
        assert_eq!(unit_json["status"], "received");

        let line: ReceiveLineInput = serde_json::from_value(serde_json::json!({
            "sku": "SKU-1",
            "quantity": 2,
            "lotCode": null
        }))
        .unwrap();
        assert_eq!(line.lot_code, None);

        let history = MovementHistoryResponse::from(crate::application::MovementHistoryEntry {
            record_id: "event-1".to_owned(),
            event_id: "event-1".to_owned(),
            handling_unit_id: "unit-1".to_owned(),
            kind: "received".to_owned(),
            origin_location_id: None,
            destination_location_id: None,
            occurred_at_ms: 1_800_000_000_000,
            recorded_at_ms: 1_800_000_000_010,
            source: "desktop_ui".to_owned(),
            result: "accepted".to_owned(),
            rejection_reason: None,
            corrects_event_id: None,
            correction_reason: None,
        });
        let history_json = serde_json::to_value(history).unwrap();
        assert_eq!(history_json["occurredAtMs"], 1_800_000_000_000_i64);
        assert_eq!(history_json["correctsEventId"], serde_json::Value::Null);
    }

    #[test]
    fn ipc_location_kind_uses_stable_snake_case_values() {
        let kind: LocationKindInput = serde_json::from_str("\"position\"").unwrap();
        assert_eq!(kind, LocationKindInput::Position);
        assert_eq!(LocationKind::from(kind).as_str(), "position");
    }

    #[test]
    fn ipc_errors_keep_stable_code_and_safe_message() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-ipc-error-{unique}"));
        let log = DiagnosticLog::open(directory.join("ariadna.log")).unwrap();
        let error = ApplicationError::Repository(crate::application::RepositoryError::Storage(
            "sqlite secret detail".to_owned(),
        ));

        let result = IpcError::from_application_error(&log, "ipc.product.get", &error);

        assert_eq!(result.code, IpcErrorCode::Storage);
        assert_eq!(
            result.message,
            "No se pudo completar la operación. No se confirmaron cambios."
        );
        assert!(!result.message.contains("sqlite"));
        let serialized = serde_json::to_value(&result).unwrap();
        assert_eq!(serialized["code"], "storage");
        assert_eq!(serialized["message"], result.message);
        let contents = std::fs::read_to_string(directory.join("ariadna.log")).unwrap();
        assert!(contents.contains("sqlite secret detail"));

        let transition_error = ApplicationError::Repository(
            crate::application::RepositoryError::InvalidAlertTransition,
        );
        let transition =
            IpcError::from_application_error(&log, "ipc.alert.review", &transition_error);
        assert_eq!(transition.code, IpcErrorCode::InvalidAlertTransition);
        assert_eq!(
            serde_json::to_value(transition).unwrap()["code"],
            "invalid_alert_transition"
        );
        drop(log);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
