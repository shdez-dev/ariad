use std::collections::HashSet;

use thiserror::Error;

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
    #[error("La cantidad de una línea debe ser mayor que cero.")]
    QuantityMustBePositive,
    #[error("Una unidad no puede repetir la misma combinación de SKU y lote.")]
    DuplicateHandlingUnitLine,
    #[error("La fecha del evento debe ser posterior al inicio de Unix.")]
    InvalidTimestamp,
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
        if lines.is_empty() {
            return Err(DomainError::HandlingUnitNeedsLines);
        }
        let id = required(id.as_ref(), "identificador de unidad")?;
        let label = required(label.as_ref(), "etiqueta de unidad")?;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub id: String,
    pub movement_event_id: String,
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
        if created_at_ms <= 0 {
            return Err(DomainError::InvalidTimestamp);
        }
        Ok(Self {
            id: required(id.as_ref(), "identificador de alerta")?,
            movement_event_id: required(movement_event_id.as_ref(), "evento relacionado")?,
            kind: required(kind.as_ref(), "tipo de alerta")?,
            created_at_ms,
            status: AlertStatus::Open,
        })
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
}
