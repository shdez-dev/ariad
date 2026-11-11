use std::collections::HashSet;

use thiserror::Error;

pub const MAX_HANDLING_UNIT_LINES: usize = 100;
pub const MAX_LOT_CODE_LENGTH: usize = 120;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("El campo {0} es obligatorio.")]
    EmptyField(&'static str),
    #[error("El campo {0} excede la longitud permitida.")]
    FieldTooLong(&'static str),
    #[error("El SKU debe contener solo letras ASCII, números, guion, guion bajo o punto.")]
    InvalidSku,
    #[error("La jerarquía de la ubicación no es válida.")]
    InvalidLocationHierarchy,
    #[error("No se puede crear o activar una ubicación bajo un ancestro inactivo.")]
    InactiveLocationParent,
    #[error("El destino debe ser una posición activa.")]
    LocationMustBeActivePosition,
    #[error("La posición ya está ocupada por otra unidad activa.")]
    PositionOccupied,
    #[error("La operación no está permitida para el estado actual de la unidad.")]
    InvalidTransition,
    #[error("La unidad debe contener al menos una línea de producto.")]
    HandlingUnitNeedsLines,
    #[error("La unidad supera el máximo de líneas de producto permitido.")]
    TooManyHandlingUnitLines,
    #[error("La cantidad de una línea debe ser mayor que cero.")]
    QuantityMustBePositive,
    #[error("Una unidad no puede repetir la misma combinación de SKU y lote.")]
    DuplicateHandlingUnitLine,
    #[error("La fecha del evento debe ser posterior al inicio de Unix.")]
    InvalidTimestamp,
    #[error("La corrección no puede referirse a sí misma.")]
    InvalidCorrectionReference,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Sku(String);

impl Sku {
    pub fn new(value: impl AsRef<str>) -> Result<Self, DomainError> {
        let normalized = value.as_ref().trim().to_ascii_uppercase();
        if normalized.is_empty() {
            return Err(DomainError::EmptyField("SKU"));
        }
        if normalized.len() > 64
            || !normalized
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
        {
            return Err(DomainError::InvalidSku);
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Product {
    pub sku: Sku,
    pub description: String,
    pub unit_of_measure: String,
    pub active: bool,
}

impl Product {
    pub fn new(
        sku: impl AsRef<str>,
        description: impl AsRef<str>,
        unit_of_measure: impl AsRef<str>,
    ) -> Result<Self, DomainError> {
        let description = description.as_ref().trim();
        let unit_of_measure = unit_of_measure.as_ref().trim();
        if description.is_empty() {
            return Err(DomainError::EmptyField("descripción"));
        }
        if description.len() > 250 {
            return Err(DomainError::FieldTooLong("descripción"));
        }
        if unit_of_measure.is_empty() {
            return Err(DomainError::EmptyField("unidad de medida"));
        }
        if unit_of_measure.len() > 24 {
            return Err(DomainError::FieldTooLong("unidad de medida"));
        }

        Ok(Self {
            sku: Sku::new(sku)?,
            description: description.to_owned(),
            unit_of_measure: unit_of_measure.to_owned(),
            active: true,
        })
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationKind {
    Zone,
    Aisle,
    Rack,
    Position,
}

impl LocationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zone => "zone",
            Self::Aisle => "aisle",
            Self::Rack => "rack",
            Self::Position => "position",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub id: String,
    pub code: String,
    pub name: String,
    pub kind: LocationKind,
    pub parent_id: Option<String>,
    pub active: bool,
}

impl Location {
    pub fn new(
        id: impl AsRef<str>,
        code: impl AsRef<str>,
        name: impl AsRef<str>,
        kind: LocationKind,
        parent: Option<&Location>,
    ) -> Result<Self, DomainError> {
        let id = required(id.as_ref(), "identificador de ubicación")?;
        let code = required(code.as_ref(), "código de ubicación")?;
        let name = required(name.as_ref(), "nombre de ubicación")?;
        if code.len() > 64 {
            return Err(DomainError::FieldTooLong("código de ubicación"));
        }
        if name.len() > 120 {
            return Err(DomainError::FieldTooLong("nombre de ubicación"));
        }
        let expected_parent = match kind {
            LocationKind::Zone => None,
            LocationKind::Aisle => Some(LocationKind::Zone),
            LocationKind::Rack => Some(LocationKind::Aisle),
            LocationKind::Position => Some(LocationKind::Rack),
        };

        match (expected_parent, parent) {
            (None, None) => {}
            (Some(expected), Some(actual)) if actual.kind != expected => {
                return Err(DomainError::InvalidLocationHierarchy);
            }
            (Some(_), Some(actual)) if !actual.active => {
                return Err(DomainError::InactiveLocationParent);
            }
            (Some(_), Some(_)) => {}
            _ => return Err(DomainError::InvalidLocationHierarchy),
        }

        Ok(Self {
            id,
            code,
            name,
            kind,
            parent_id: parent.map(|location| location.id.clone()),
            active: true,
        })
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }

    pub fn update_details(
        &mut self,
        code: impl AsRef<str>,
        name: impl AsRef<str>,
    ) -> Result<(), DomainError> {
        let code = required(code.as_ref(), "código de ubicación")?;
        let name = required(name.as_ref(), "nombre de ubicación")?;
        if code.len() > 64 {
            return Err(DomainError::FieldTooLong("código de ubicación"));
        }
        if name.len() > 120 {
            return Err(DomainError::FieldTooLong("nombre de ubicación"));
        }
        self.code = code;
        self.name = name;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlingUnitLine {
    pub sku: Sku,
    pub quantity: u32,
    pub lot_code: Option<String>,
}

impl HandlingUnitLine {
    pub fn new(
        sku: impl AsRef<str>,
        quantity: u32,
        lot_code: Option<impl AsRef<str>>,
    ) -> Result<Self, DomainError> {
        if quantity == 0 {
            return Err(DomainError::QuantityMustBePositive);
        }

        let lot_code = lot_code
            .map(|value| value.as_ref().trim().to_owned())
            .filter(|value| !value.is_empty());
        if lot_code.as_ref().is_some_and(|value| {
            value.len() > MAX_LOT_CODE_LENGTH * 4 || value.chars().count() > MAX_LOT_CODE_LENGTH
        }) {
            return Err(DomainError::FieldTooLong("código de lote"));
        }

        Ok(Self {
            sku: Sku::new(sku)?,
            quantity,
            lot_code,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandlingUnitStatus {
    Received,
    Stored,
    Dispatched,
}

impl HandlingUnitStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Stored => "stored",
            Self::Dispatched => "dispatched",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlingUnit {
    pub id: String,
    pub label: String,
    pub status: HandlingUnitStatus,
    pub current_location_id: Option<String>,
    pub lines: Vec<HandlingUnitLine>,
}

impl HandlingUnit {
    pub fn new(
        id: impl AsRef<str>,
        label: impl AsRef<str>,
        lines: Vec<HandlingUnitLine>,
    ) -> Result<Self, DomainError> {
        if lines.len() > MAX_HANDLING_UNIT_LINES {
            return Err(DomainError::TooManyHandlingUnitLines);
        }
        if lines.is_empty() {
            return Err(DomainError::HandlingUnitNeedsLines);
        }
        let id = required(id.as_ref(), "identificador de unidad")?;
        let label = required(label.as_ref(), "etiqueta de unidad")?;
        if label.len() > 120 {
            return Err(DomainError::FieldTooLong("etiqueta de unidad"));
        }
        let mut seen_lines = HashSet::new();
        for line in &lines {
            if !seen_lines.insert((line.sku.as_str().to_owned(), line.lot_code.clone())) {
                return Err(DomainError::DuplicateHandlingUnitLine);
            }
        }

        Ok(Self {
            id,
            label,
            status: HandlingUnitStatus::Received,
            current_location_id: None,
            lines,
        })
    }

    pub fn assign_location(
        &mut self,
        location: &Location,
        occupied_by_another_unit: bool,
    ) -> Result<(), DomainError> {
        if self.status == HandlingUnitStatus::Dispatched {
            return Err(DomainError::InvalidTransition);
        }
        if location.kind != LocationKind::Position || !location.active {
            return Err(DomainError::LocationMustBeActivePosition);
        }
        if occupied_by_another_unit
            && self.current_location_id.as_deref() != Some(location.id.as_str())
        {
            return Err(DomainError::PositionOccupied);
        }

        self.current_location_id = Some(location.id.clone());
        self.status = HandlingUnitStatus::Stored;
        Ok(())
    }

    pub fn dispatch(&mut self) -> Result<(), DomainError> {
        if self.status != HandlingUnitStatus::Stored {
            return Err(DomainError::InvalidTransition);
        }

        self.current_location_id = None;
        self.status = HandlingUnitStatus::Dispatched;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementKind {
    Received,
    Stored,
    Moved,
    Dispatched,
}

impl MovementKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Stored => "stored",
            Self::Moved => "moved",
            Self::Dispatched => "dispatched",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovementEvent {
    pub event_id: String,
    pub handling_unit_id: String,
    pub kind: MovementKind,
    pub origin_location_id: Option<String>,
    pub destination_location_id: Option<String>,
    pub occurred_at_ms: i64,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovementCorrection {
    pub correction_id: String,
    pub handling_unit_id: String,
    pub corrects_event_id: String,
    pub reason: String,
    pub occurred_at_ms: i64,
    pub source: String,
}

impl MovementCorrection {
    pub fn new(
        correction_id: impl AsRef<str>,
        handling_unit_id: impl AsRef<str>,
        corrects_event_id: impl AsRef<str>,
        reason: impl AsRef<str>,
        occurred_at_ms: i64,
        source: impl AsRef<str>,
    ) -> Result<Self, DomainError> {
        if occurred_at_ms <= 0 {
            return Err(DomainError::InvalidTimestamp);
        }
        let correction_id = required(correction_id.as_ref(), "identificador de corrección")?;
        let corrects_event_id = required(corrects_event_id.as_ref(), "evento corregido")?;
        if correction_id == corrects_event_id {
            return Err(DomainError::InvalidCorrectionReference);
        }
        let reason = required(reason.as_ref(), "motivo de corrección")?;
        if reason.len() > 500 {
            return Err(DomainError::FieldTooLong("motivo de corrección"));
        }

        Ok(Self {
            correction_id,
            handling_unit_id: required(handling_unit_id.as_ref(), "unidad de manipulación")?,
            corrects_event_id,
            reason,
            occurred_at_ms,
            source: required(source.as_ref(), "fuente de corrección")?,
        })
    }
}

impl MovementEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_id: impl AsRef<str>,
        handling_unit_id: impl AsRef<str>,
        kind: MovementKind,
        origin_location_id: Option<impl AsRef<str>>,
        destination_location_id: Option<impl AsRef<str>>,
        occurred_at_ms: i64,
        source: impl AsRef<str>,
    ) -> Result<Self, DomainError> {
        if occurred_at_ms <= 0 {
            return Err(DomainError::InvalidTimestamp);
        }

        Ok(Self {
            event_id: required(event_id.as_ref(), "identificador de evento")?,
            handling_unit_id: required(handling_unit_id.as_ref(), "unidad de manipulación")?,
            kind,
            origin_location_id: optional_required(origin_location_id, "ubicación de origen")?,
            destination_location_id: optional_required(
                destination_location_id,
                "ubicación de destino",
            )?,
            occurred_at_ms,
            source: required(source.as_ref(), "fuente del evento")?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertStatus {
    Open,
    Reviewed,
    Resolved,
}

impl AlertStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Reviewed => "reviewed",
            Self::Resolved => "resolved",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlertRecordTarget {
    MovementEvent { event_id: String },
    MovementAttempt { attempt_id: i64, event_id: String },
}

impl AlertRecordTarget {
    pub fn record_id(&self) -> String {
        match self {
            Self::MovementEvent { event_id } => event_id.clone(),
            Self::MovementAttempt { attempt_id, .. } => format!("attempt:{attempt_id}"),
        }
    }

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::MovementEvent { .. } => "movement_event",
            Self::MovementAttempt { .. } => "movement_attempt",
        }
    }

    pub fn event_id(&self) -> &str {
        match self {
            Self::MovementEvent { event_id } | Self::MovementAttempt { event_id, .. } => event_id,
        }
    }

    pub fn movement_event_id(&self) -> Option<&str> {
        match self {
            Self::MovementEvent { event_id } => Some(event_id),
            Self::MovementAttempt { .. } => None,
        }
    }

    pub const fn movement_attempt_id(&self) -> Option<i64> {
        match self {
            Self::MovementEvent { .. } => None,
            Self::MovementAttempt { attempt_id, .. } => Some(*attempt_id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertIntent {
    pub id_prefix: String,
    pub kind: String,
    pub created_at_ms: i64,
}

impl AlertIntent {
    pub fn new(
        id_prefix: impl AsRef<str>,
        kind: impl AsRef<str>,
        created_at_ms: i64,
    ) -> Result<Self, DomainError> {
        if created_at_ms <= 0 {
            return Err(DomainError::InvalidTimestamp);
        }
        Ok(Self {
            id_prefix: required(id_prefix.as_ref(), "prefijo de alerta")?,
            kind: required(kind.as_ref(), "tipo de alerta")?,
            created_at_ms,
        })
    }

    pub fn id_for_target(&self, target: &AlertRecordTarget) -> String {
        match target {
            AlertRecordTarget::MovementEvent { .. } => {
                format!("{}-event", self.id_prefix)
            }
            AlertRecordTarget::MovementAttempt { attempt_id, .. } => {
                format!("{}-attempt-{attempt_id}", self.id_prefix)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub id: String,
    pub target: AlertRecordTarget,
    pub kind: String,
    pub created_at_ms: i64,
    pub status: AlertStatus,
}

impl Alert {
    pub fn new(
        id: impl AsRef<str>,
        movement_event_id: impl AsRef<str>,
        kind: impl AsRef<str>,
        created_at_ms: i64,
    ) -> Result<Self, DomainError> {
        Self::from_persisted(
            id,
            AlertRecordTarget::MovementEvent {
                event_id: required(movement_event_id.as_ref(), "evento relacionado")?,
            },
            kind,
            created_at_ms,
            AlertStatus::Open,
        )
    }

    pub fn from_persisted(
        id: impl AsRef<str>,
        target: AlertRecordTarget,
        kind: impl AsRef<str>,
        created_at_ms: i64,
        status: AlertStatus,
    ) -> Result<Self, DomainError> {
        if created_at_ms <= 0 {
            return Err(DomainError::InvalidTimestamp);
        }
        Ok(Self {
            id: required(id.as_ref(), "identificador de alerta")?,
            target,
            kind: required(kind.as_ref(), "tipo de alerta")?,
            created_at_ms,
            status,
        })
    }

    pub fn from_intent(
        intent: &AlertIntent,
        target: AlertRecordTarget,
    ) -> Result<Self, DomainError> {
        Self::from_persisted(
            intent.id_for_target(&target),
            target,
            &intent.kind,
            intent.created_at_ms,
            AlertStatus::Open,
        )
    }
}

fn required(value: &str, field: &'static str) -> Result<String, DomainError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(DomainError::EmptyField(field));
    }
    Ok(value.to_owned())
}

fn optional_required(
    value: Option<impl AsRef<str>>,
    field: &'static str,
) -> Result<Option<String>, DomainError> {
    value
        .map(|value| required(value.as_ref(), field))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(sku: &str, quantity: u32) -> HandlingUnitLine {
        HandlingUnitLine::new(sku, quantity, None::<&str>).unwrap()
    }

    fn location_tree() -> (Location, Location, Location, Location) {
        let zone = Location::new("z1", "Z1", "Zona 1", LocationKind::Zone, None).unwrap();
        let aisle =
            Location::new("a1", "A1", "Pasillo 1", LocationKind::Aisle, Some(&zone)).unwrap();
        let rack = Location::new("r1", "R1", "Rack 1", LocationKind::Rack, Some(&aisle)).unwrap();
        let position = Location::new(
            "p1",
            "P1",
            "Posición 1",
            LocationKind::Position,
            Some(&rack),
        )
        .unwrap();
        (zone, aisle, rack, position)
    }

    #[test]
    fn normalizes_sku_and_rejects_invalid_values() {
        assert_eq!(Sku::new(" sku-01 ").unwrap().as_str(), "SKU-01");
        assert_eq!(Sku::new("SKU con espacios"), Err(DomainError::InvalidSku));
        assert_eq!(Sku::new("  "), Err(DomainError::EmptyField("SKU")));
    }

    #[test]
    fn enforces_location_hierarchy() {
        let zone = Location::new("z1", "Z1", "Zona", LocationKind::Zone, None).unwrap();
        assert!(Location::new("a1", "A1", "Pasillo", LocationKind::Aisle, Some(&zone)).is_ok());
        assert_eq!(
            Location::new("r1", "R1", "Rack", LocationKind::Rack, Some(&zone)),
            Err(DomainError::InvalidLocationHierarchy)
        );
    }

    #[test]
    fn rejects_child_location_under_inactive_parent() {
        let mut zone = Location::new("z1", "Z1", "Zona", LocationKind::Zone, None).unwrap();
        zone.deactivate();
        assert_eq!(
            Location::new("a1", "A1", "Pasillo", LocationKind::Aisle, Some(&zone)),
            Err(DomainError::InactiveLocationParent)
        );
    }

    #[test]
    fn requires_positive_unique_handling_unit_lines() {
        assert_eq!(
            HandlingUnitLine::new("SKU-1", 0, None::<&str>),
            Err(DomainError::QuantityMustBePositive)
        );
        assert_eq!(
            HandlingUnit::new("u1", "PALLET-1", vec![line("sku-1", 1), line("SKU-1", 2)]),
            Err(DomainError::DuplicateHandlingUnitLine)
        );
    }

    #[test]
    fn rejects_occupied_position_without_mutating_unit() {
        let (_, _, _, position) = location_tree();
        let mut unit = HandlingUnit::new("u1", "PALLET-1", vec![line("SKU-1", 1)]).unwrap();
        let before = unit.clone();

        assert_eq!(
            unit.assign_location(&position, true),
            Err(DomainError::PositionOccupied)
        );
        assert_eq!(unit, before);
    }

    #[test]
    fn rejects_dispatch_from_received_state_without_mutating_unit() {
        let mut unit = HandlingUnit::new("u1", "PALLET-1", vec![line("SKU-1", 1)]).unwrap();
        let before = unit.clone();

        assert_eq!(unit.dispatch(), Err(DomainError::InvalidTransition));
        assert_eq!(unit, before);
    }

    #[test]
    fn stores_and_dispatches_unit_consistently() {
        let (_, _, _, position) = location_tree();
        let mut unit = HandlingUnit::new("u1", "PALLET-1", vec![line("SKU-1", 1)]).unwrap();

        unit.assign_location(&position, false).unwrap();
        assert_eq!(unit.status, HandlingUnitStatus::Stored);
        assert_eq!(unit.current_location_id.as_deref(), Some("p1"));

        unit.dispatch().unwrap();
        assert_eq!(unit.status, HandlingUnitStatus::Dispatched);
        assert_eq!(unit.current_location_id, None);
    }

    #[test]
    fn validates_correction_identity_reason_and_timestamp() {
        let correction = MovementCorrection::new(
            " correction-1 ",
            " unit-1 ",
            " event-1 ",
            " Conteo manual ",
            1_800_000_000_000,
            " manual ",
        )
        .unwrap();
        assert_eq!(correction.correction_id, "correction-1");
        assert_eq!(correction.corrects_event_id, "event-1");
        assert_eq!(correction.reason, "Conteo manual");

        assert_eq!(
            MovementCorrection::new(
                "event-1",
                "unit-1",
                "event-1",
                "Motivo",
                1_800_000_000_000,
                "manual",
            ),
            Err(DomainError::InvalidCorrectionReference)
        );
        assert_eq!(
            MovementCorrection::new(
                "correction-2",
                "unit-1",
                "event-1",
                " ",
                1_800_000_000_000,
                "manual",
            ),
            Err(DomainError::EmptyField("motivo de corrección"))
        );
        assert_eq!(
            MovementCorrection::new(
                "correction-3",
                "unit-1",
                "event-1",
                "x".repeat(501),
                1_800_000_000_000,
                "manual",
            ),
            Err(DomainError::FieldTooLong("motivo de corrección"))
        );
    }

    #[test]
    fn validates_product_boundaries_and_deactivation() {
        let mut product = Product::new(" sku-01 ", "  Caja de cartón  ", "  unidad ").unwrap();
        assert_eq!(product.sku.as_str(), "SKU-01");
        assert_eq!(product.description, "Caja de cartón");
        assert_eq!(product.unit_of_measure, "unidad");
        assert!(product.active);

        assert_eq!(
            Product::new("SKU-1", "  ", "unidad"),
            Err(DomainError::EmptyField("descripción"))
        );
        assert_eq!(
            Product::new("SKU-1", "Caja", "  "),
            Err(DomainError::EmptyField("unidad de medida"))
        );
        assert_eq!(
            Product::new("SKU-1", "x".repeat(251), "unidad"),
            Err(DomainError::FieldTooLong("descripción"))
        );
        assert_eq!(
            Product::new("SKU-1", "Caja", "u".repeat(25)),
            Err(DomainError::FieldTooLong("unidad de medida"))
        );

        let boundary = Product::new("SKU-2", "x".repeat(250), "u".repeat(24)).unwrap();
        assert_eq!(boundary.description.len(), 250);
        assert_eq!(boundary.unit_of_measure.len(), 24);

        product.deactivate();
        assert!(!product.active);
    }

    #[test]
    fn accepts_each_location_parent_level_and_rejects_mismatches() {
        let (zone, aisle, rack, position) = location_tree();
        assert_eq!(aisle.parent_id.as_deref(), Some(zone.id.as_str()));
        assert_eq!(rack.parent_id.as_deref(), Some(aisle.id.as_str()));
        assert_eq!(position.parent_id.as_deref(), Some(rack.id.as_str()));

        assert_eq!(
            Location::new(
                "zone-child",
                "Z2",
                "Zona 2",
                LocationKind::Zone,
                Some(&zone)
            ),
            Err(DomainError::InvalidLocationHierarchy)
        );
        assert_eq!(
            Location::new("aisle-root", "A2", "Pasillo 2", LocationKind::Aisle, None),
            Err(DomainError::InvalidLocationHierarchy)
        );
        assert_eq!(
            Location::new(
                "rack-under-zone",
                "R2",
                "Rack 2",
                LocationKind::Rack,
                Some(&zone)
            ),
            Err(DomainError::InvalidLocationHierarchy)
        );
        assert_eq!(
            Location::new(
                "position-under-aisle",
                "P2",
                "Posición 2",
                LocationKind::Position,
                Some(&aisle),
            ),
            Err(DomainError::InvalidLocationHierarchy)
        );
    }

    #[test]
    fn validates_location_fields_boundaries_and_update_atomicity() {
        assert_eq!(
            Location::new(" ", "Z1", "Zona", LocationKind::Zone, None),
            Err(DomainError::EmptyField("identificador de ubicación"))
        );
        assert_eq!(
            Location::new("z1", " ", "Zona", LocationKind::Zone, None),
            Err(DomainError::EmptyField("código de ubicación"))
        );
        assert_eq!(
            Location::new("z1", "Z1", " ", LocationKind::Zone, None),
            Err(DomainError::EmptyField("nombre de ubicación"))
        );

        let boundary = Location::new(
            "z-limit",
            "c".repeat(64),
            "n".repeat(120),
            LocationKind::Zone,
            None,
        )
        .unwrap();
        assert_eq!(boundary.code.len(), 64);
        assert_eq!(boundary.name.len(), 120);
        assert_eq!(
            Location::new(
                "z-long-code",
                "c".repeat(65),
                "Zona",
                LocationKind::Zone,
                None,
            ),
            Err(DomainError::FieldTooLong("código de ubicación"))
        );
        assert_eq!(
            Location::new(
                "z-long-name",
                "Z2",
                "n".repeat(121),
                LocationKind::Zone,
                None,
            ),
            Err(DomainError::FieldTooLong("nombre de ubicación"))
        );

        let (_, _, _, mut position) = location_tree();
        let before = position.clone();
        assert_eq!(
            position.update_details("P".repeat(65), "Posición nueva"),
            Err(DomainError::FieldTooLong("código de ubicación"))
        );
        assert_eq!(position, before);
        assert_eq!(
            position.update_details("P2", "n".repeat(121)),
            Err(DomainError::FieldTooLong("nombre de ubicación"))
        );
        assert_eq!(position, before);

        position.update_details(" P2 ", " Posición nueva ").unwrap();
        assert_eq!(position.code, "P2");
        assert_eq!(position.name, "Posición nueva");
        position.deactivate();
        assert!(!position.active);
    }

    #[test]
    fn normalizes_lines_and_distinguishes_lots_for_duplicate_detection() {
        let line_with_lot = HandlingUnitLine::new(" sku-1 ", u32::MAX, Some(" lot-a ")).unwrap();
        assert_eq!(line_with_lot.sku.as_str(), "SKU-1");
        assert_eq!(line_with_lot.quantity, u32::MAX);
        assert_eq!(line_with_lot.lot_code.as_deref(), Some("lot-a"));
        assert_eq!(
            HandlingUnitLine::new("SKU-1", 1, Some("  "))
                .unwrap()
                .lot_code,
            None
        );

        let lines = vec![
            HandlingUnitLine::new("sku-1", 1, Some("lot-a")).unwrap(),
            HandlingUnitLine::new("SKU-1", 2, Some("lot-b")).unwrap(),
        ];
        assert!(HandlingUnit::new("unit-lots", "PALLET-LOTS", lines).is_ok());

        let duplicate_lines = vec![
            HandlingUnitLine::new("sku-1", 1, Some(" lot-a ")).unwrap(),
            HandlingUnitLine::new("SKU-1", 2, Some("lot-a")).unwrap(),
        ];
        assert_eq!(
            HandlingUnit::new("unit-duplicate-lot", "PALLET-DUP", duplicate_lines),
            Err(DomainError::DuplicateHandlingUnitLine)
        );

        assert!(HandlingUnitLine::new("SKU-1", 1, Some("L".repeat(MAX_LOT_CODE_LENGTH))).is_ok());
        assert_eq!(
            HandlingUnitLine::new("SKU-1", 1, Some("L".repeat(MAX_LOT_CODE_LENGTH + 1))),
            Err(DomainError::FieldTooLong("código de lote"))
        );
    }

    #[test]
    fn enforces_handling_unit_line_count_limit() {
        let boundary_lines = (1..=MAX_HANDLING_UNIT_LINES)
            .map(|index| line(&format!("SKU-{index}"), 1))
            .collect();
        let unit = HandlingUnit::new("unit-boundary", "PALLET-BOUNDARY", boundary_lines).unwrap();
        assert_eq!(unit.lines.len(), MAX_HANDLING_UNIT_LINES);

        let too_many_lines = (1..=MAX_HANDLING_UNIT_LINES + 1)
            .map(|index| line(&format!("SKU-{index}"), 1))
            .collect();
        assert_eq!(
            HandlingUnit::new("unit-too-many", "PALLET-TOO-MANY", too_many_lines),
            Err(DomainError::TooManyHandlingUnitLines)
        );
    }

    #[test]
    fn validates_handling_unit_required_fields_and_label_boundary() {
        assert_eq!(
            HandlingUnit::new("unit-empty", "PALLET-EMPTY", vec![]),
            Err(DomainError::HandlingUnitNeedsLines)
        );
        assert_eq!(
            HandlingUnit::new(" ", "PALLET-1", vec![line("SKU-1", 1)]),
            Err(DomainError::EmptyField("identificador de unidad"))
        );
        assert_eq!(
            HandlingUnit::new("unit-1", " ", vec![line("SKU-1", 1)]),
            Err(DomainError::EmptyField("etiqueta de unidad"))
        );

        let boundary =
            HandlingUnit::new("unit-boundary", "L".repeat(120), vec![line("SKU-1", 1)]).unwrap();
        assert_eq!(boundary.label.len(), 120);
        assert_eq!(
            HandlingUnit::new("unit-too-long", "L".repeat(121), vec![line("SKU-1", 1)]),
            Err(DomainError::FieldTooLong("etiqueta de unidad"))
        );
    }

    #[test]
    fn receives_moves_and_dispatches_a_unit_through_valid_states() {
        let (_, _, rack, first_position) = location_tree();
        let second_position = Location::new(
            "p2",
            "P2",
            "Posición 2",
            LocationKind::Position,
            Some(&rack),
        )
        .unwrap();
        let mut unit = HandlingUnit::new("unit-1", "PALLET-1", vec![line("SKU-1", 2)]).unwrap();
        assert_eq!(unit.status, HandlingUnitStatus::Received);
        assert_eq!(unit.current_location_id, None);

        unit.assign_location(&first_position, false).unwrap();
        assert_eq!(unit.status, HandlingUnitStatus::Stored);
        assert_eq!(unit.current_location_id.as_deref(), Some("p1"));

        unit.assign_location(&second_position, false).unwrap();
        assert_eq!(unit.status, HandlingUnitStatus::Stored);
        assert_eq!(unit.current_location_id.as_deref(), Some("p2"));

        unit.dispatch().unwrap();
        assert_eq!(unit.status, HandlingUnitStatus::Dispatched);
        assert_eq!(unit.current_location_id, None);
    }

    #[test]
    fn rejects_invalid_assignment_and_dispatch_without_mutating_the_unit() {
        let (_, _, rack, position) = location_tree();
        let other_position = Location::new(
            "p2",
            "P2",
            "Posición 2",
            LocationKind::Position,
            Some(&rack),
        )
        .unwrap();
        let mut inactive_position = other_position.clone();
        inactive_position.deactivate();
        let mut unit = HandlingUnit::new("unit-1", "PALLET-1", vec![line("SKU-1", 1)]).unwrap();
        let received = unit.clone();

        assert_eq!(
            unit.assign_location(&rack, false),
            Err(DomainError::LocationMustBeActivePosition)
        );
        assert_eq!(unit, received);
        assert_eq!(
            unit.assign_location(&inactive_position, false),
            Err(DomainError::LocationMustBeActivePosition)
        );
        assert_eq!(unit, received);

        unit.assign_location(&position, false).unwrap();
        let stored = unit.clone();
        assert_eq!(
            unit.assign_location(&other_position, true),
            Err(DomainError::PositionOccupied)
        );
        assert_eq!(unit, stored);
        unit.assign_location(&position, true).unwrap();
        assert_eq!(unit, stored);

        unit.dispatch().unwrap();
        let dispatched = unit.clone();
        assert_eq!(
            unit.assign_location(&position, false),
            Err(DomainError::InvalidTransition)
        );
        assert_eq!(unit, dispatched);
    }

    #[test]
    fn validates_movement_event_identity_timestamp_and_optional_locations() {
        for timestamp in [0, -1] {
            assert_eq!(
                MovementEvent::new(
                    "event-1",
                    "unit-1",
                    MovementKind::Received,
                    None::<&str>,
                    None::<&str>,
                    timestamp,
                    "desktop_ui",
                ),
                Err(DomainError::InvalidTimestamp)
            );
        }

        assert_eq!(
            MovementEvent::new(
                " ",
                "unit-1",
                MovementKind::Received,
                None::<&str>,
                None::<&str>,
                1,
                "desktop_ui",
            ),
            Err(DomainError::EmptyField("identificador de evento"))
        );
        assert_eq!(
            MovementEvent::new(
                "event-1",
                " ",
                MovementKind::Received,
                None::<&str>,
                None::<&str>,
                1,
                "desktop_ui",
            ),
            Err(DomainError::EmptyField("unidad de manipulación"))
        );
        assert_eq!(
            MovementEvent::new(
                "event-1",
                "unit-1",
                MovementKind::Moved,
                Some(" "),
                None::<&str>,
                1,
                "desktop_ui",
            ),
            Err(DomainError::EmptyField("ubicación de origen"))
        );

        let event = MovementEvent::new(
            " event-1 ",
            " unit-1 ",
            MovementKind::Moved,
            Some(" p1 "),
            Some(" p2 "),
            1,
            " desktop_ui ",
        )
        .unwrap();
        assert_eq!(event.event_id, "event-1");
        assert_eq!(event.handling_unit_id, "unit-1");
        assert_eq!(event.origin_location_id.as_deref(), Some("p1"));
        assert_eq!(event.destination_location_id.as_deref(), Some("p2"));
        assert_eq!(event.source, "desktop_ui");
    }

    #[test]
    fn validates_correction_timestamp_required_fields_and_maximum_reason() {
        assert_eq!(
            MovementCorrection::new("correction-1", "unit-1", "event-1", "motivo", 0, "manual"),
            Err(DomainError::InvalidTimestamp)
        );
        assert_eq!(
            MovementCorrection::new(" ", "unit-1", "event-1", "motivo", 1, "manual"),
            Err(DomainError::EmptyField("identificador de corrección"))
        );
        assert_eq!(
            MovementCorrection::new(
                "correction-1",
                "unit-1",
                "event-1",
                "x".repeat(501),
                1,
                "manual"
            ),
            Err(DomainError::FieldTooLong("motivo de corrección"))
        );

        let correction = MovementCorrection::new(
            " correction-1 ",
            " unit-1 ",
            " event-1 ",
            "x".repeat(500),
            1,
            " manual ",
        )
        .unwrap();
        assert_eq!(correction.correction_id, "correction-1");
        assert_eq!(correction.handling_unit_id, "unit-1");
        assert_eq!(correction.corrects_event_id, "event-1");
        assert_eq!(correction.reason.len(), 500);
        assert_eq!(correction.source, "manual");
    }

    #[test]
    fn creates_alerts_with_exact_event_and_attempt_targets() {
        assert_eq!(
            AlertIntent::new(" ", "duplicate", 1),
            Err(DomainError::EmptyField("prefijo de alerta"))
        );
        assert_eq!(
            AlertIntent::new("prefix", " ", 1),
            Err(DomainError::EmptyField("tipo de alerta"))
        );
        assert_eq!(
            AlertIntent::new("prefix", "duplicate", 0),
            Err(DomainError::InvalidTimestamp)
        );

        let intent = AlertIntent::new(" sim-1 ", " duplicate ", 1).unwrap();
        let event_target = AlertRecordTarget::MovementEvent {
            event_id: "event-1".to_owned(),
        };
        let first_attempt = AlertRecordTarget::MovementAttempt {
            attempt_id: 7,
            event_id: "event-1".to_owned(),
        };
        let second_attempt = AlertRecordTarget::MovementAttempt {
            attempt_id: 8,
            event_id: "event-1".to_owned(),
        };

        let event_alert = Alert::from_intent(&intent, event_target.clone()).unwrap();
        let attempt_alert = Alert::from_intent(&intent, first_attempt.clone()).unwrap();
        let next_attempt_alert = Alert::from_intent(&intent, second_attempt.clone()).unwrap();
        assert_eq!(event_alert.id, "sim-1-event");
        assert_eq!(event_alert.status, AlertStatus::Open);
        assert_eq!(event_alert.target.record_id(), "event-1");
        assert_eq!(event_alert.target.kind(), "movement_event");
        assert_eq!(event_alert.target.movement_event_id(), Some("event-1"));
        assert_eq!(event_alert.target.movement_attempt_id(), None);

        assert_eq!(attempt_alert.id, "sim-1-attempt-7");
        assert_eq!(attempt_alert.target.record_id(), "attempt:7");
        assert_eq!(attempt_alert.target.kind(), "movement_attempt");
        assert_eq!(attempt_alert.target.event_id(), "event-1");
        assert_eq!(attempt_alert.target.movement_event_id(), None);
        assert_eq!(attempt_alert.target.movement_attempt_id(), Some(7));
        assert_ne!(attempt_alert.id, next_attempt_alert.id);

        let reviewed = Alert::from_persisted(
            "alert-reviewed",
            second_attempt.clone(),
            "duplicate",
            2,
            AlertStatus::Reviewed,
        )
        .unwrap();
        assert_eq!(reviewed.status, AlertStatus::Reviewed);
        assert_eq!(reviewed.target, second_attempt);
    }

    #[test]
    fn domain_enums_have_stable_external_names() {
        assert_eq!(LocationKind::Zone.as_str(), "zone");
        assert_eq!(LocationKind::Aisle.as_str(), "aisle");
        assert_eq!(LocationKind::Rack.as_str(), "rack");
        assert_eq!(LocationKind::Position.as_str(), "position");
        assert_eq!(HandlingUnitStatus::Received.as_str(), "received");
        assert_eq!(HandlingUnitStatus::Stored.as_str(), "stored");
        assert_eq!(HandlingUnitStatus::Dispatched.as_str(), "dispatched");
        assert_eq!(MovementKind::Received.as_str(), "received");
        assert_eq!(MovementKind::Stored.as_str(), "stored");
        assert_eq!(MovementKind::Moved.as_str(), "moved");
        assert_eq!(MovementKind::Dispatched.as_str(), "dispatched");
        assert_eq!(AlertStatus::Open.as_str(), "open");
        assert_eq!(AlertStatus::Reviewed.as_str(), "reviewed");
        assert_eq!(AlertStatus::Resolved.as_str(), "resolved");
    }
}
