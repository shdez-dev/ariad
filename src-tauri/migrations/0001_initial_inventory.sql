CREATE TABLE products (
    sku TEXT PRIMARY KEY NOT NULL COLLATE NOCASE
        CHECK (length(trim(sku)) BETWEEN 1 AND 64),
    description TEXT NOT NULL
        CHECK (length(trim(description)) BETWEEN 1 AND 250),
    unit_of_measure TEXT NOT NULL
        CHECK (length(trim(unit_of_measure)) BETWEEN 1 AND 24),
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at_ms INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    updated_at_ms INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER) * 1000)
);

CREATE TABLE locations (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(id)) > 0),
    code TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (length(trim(code)) BETWEEN 1 AND 64),
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 120),
    kind TEXT NOT NULL CHECK (kind IN ('zone', 'aisle', 'rack', 'position')),
    parent_id TEXT REFERENCES locations(id) ON DELETE RESTRICT,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at_ms INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    updated_at_ms INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    CHECK (
        (kind = 'zone' AND parent_id IS NULL)
        OR (kind <> 'zone' AND parent_id IS NOT NULL)
    )
);

CREATE INDEX idx_locations_parent_id ON locations(parent_id);
CREATE INDEX idx_locations_kind_active ON locations(kind, is_active);

CREATE TRIGGER validate_location_parent_insert
BEFORE INSERT ON locations
WHEN
    (NEW.kind = 'aisle' AND NOT EXISTS (
        SELECT 1 FROM locations WHERE id = NEW.parent_id AND kind = 'zone'
            AND (NEW.is_active = 0 OR is_active = 1)
    ))
    OR (NEW.kind = 'rack' AND NOT EXISTS (
        SELECT 1 FROM locations WHERE id = NEW.parent_id AND kind = 'aisle'
            AND (NEW.is_active = 0 OR is_active = 1)
    ))
    OR (NEW.kind = 'position' AND NOT EXISTS (
        SELECT 1 FROM locations WHERE id = NEW.parent_id AND kind = 'rack'
            AND (NEW.is_active = 0 OR is_active = 1)
    ))
BEGIN
    SELECT RAISE(ABORT, 'invalid_location_hierarchy');
END;

CREATE TRIGGER validate_location_parent_update
BEFORE UPDATE OF kind, parent_id, is_active ON locations
WHEN
    (NEW.kind = 'aisle' AND NOT EXISTS (
        SELECT 1 FROM locations WHERE id = NEW.parent_id AND kind = 'zone'
            AND (NEW.is_active = 0 OR is_active = 1)
    ))
    OR (NEW.kind = 'rack' AND NOT EXISTS (
        SELECT 1 FROM locations WHERE id = NEW.parent_id AND kind = 'aisle'
            AND (NEW.is_active = 0 OR is_active = 1)
    ))
    OR (NEW.kind = 'position' AND NOT EXISTS (
        SELECT 1 FROM locations WHERE id = NEW.parent_id AND kind = 'rack'
            AND (NEW.is_active = 0 OR is_active = 1)
    ))
BEGIN
    SELECT RAISE(ABORT, 'invalid_location_hierarchy');
END;

CREATE TRIGGER prevent_location_kind_change_with_children
BEFORE UPDATE OF kind ON locations
WHEN EXISTS (
    SELECT 1
    FROM locations AS child
    WHERE child.parent_id = OLD.id
      AND NOT (
          (NEW.kind = 'zone' AND child.kind = 'aisle')
          OR (NEW.kind = 'aisle' AND child.kind = 'rack')
          OR (NEW.kind = 'rack' AND child.kind = 'position')
      )
)
BEGIN
    SELECT RAISE(ABORT, 'location_kind_conflicts_with_children');
END;

CREATE TRIGGER prevent_active_parent_deactivation
BEFORE UPDATE OF is_active ON locations
WHEN NEW.is_active = 0 AND EXISTS (
    SELECT 1 FROM locations WHERE parent_id = OLD.id AND is_active = 1
)
BEGIN
    SELECT RAISE(ABORT, 'location_has_active_children');
END;

CREATE TABLE handling_units (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(id)) > 0),
    label TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (length(trim(label)) BETWEEN 1 AND 120),
    status TEXT NOT NULL CHECK (status IN ('received', 'stored', 'dispatched')),
    current_location_id TEXT REFERENCES locations(id) ON DELETE RESTRICT,
    created_at_ms INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    updated_at_ms INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    CHECK (
        (status = 'stored' AND current_location_id IS NOT NULL)
        OR (status IN ('received', 'dispatched') AND current_location_id IS NULL)
    )
);

CREATE UNIQUE INDEX uq_handling_units_active_location
    ON handling_units(current_location_id)
    WHERE status = 'stored' AND current_location_id IS NOT NULL;
CREATE INDEX idx_handling_units_status ON handling_units(status);

CREATE TRIGGER validate_handling_unit_position_insert
BEFORE INSERT ON handling_units
WHEN NEW.current_location_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM locations
    WHERE id = NEW.current_location_id AND kind = 'position' AND is_active = 1
)
BEGIN
    SELECT RAISE(ABORT, 'handling_unit_location_must_be_active_position');
END;

CREATE TRIGGER validate_handling_unit_position_update
BEFORE UPDATE OF current_location_id, status ON handling_units
WHEN NEW.current_location_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM locations
    WHERE id = NEW.current_location_id AND kind = 'position' AND is_active = 1
)
BEGIN
    SELECT RAISE(ABORT, 'handling_unit_location_must_be_active_position');
END;

CREATE TRIGGER prevent_occupied_position_deactivation
BEFORE UPDATE OF is_active ON locations
WHEN NEW.is_active = 0 AND EXISTS (
    SELECT 1 FROM handling_units
    WHERE current_location_id = OLD.id AND status = 'stored'
)
BEGIN
    SELECT RAISE(ABORT, 'location_occupied');
END;

CREATE TABLE handling_unit_lines (
    handling_unit_id TEXT NOT NULL REFERENCES handling_units(id) ON DELETE RESTRICT,
    product_sku TEXT NOT NULL COLLATE NOCASE REFERENCES products(sku) ON DELETE RESTRICT,
    lot_code TEXT NOT NULL DEFAULT '',
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    PRIMARY KEY (handling_unit_id, product_sku, lot_code)
);

CREATE INDEX idx_handling_unit_lines_product_sku ON handling_unit_lines(product_sku);

CREATE TABLE movement_events (
    event_id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(event_id)) > 0),
    handling_unit_id TEXT NOT NULL REFERENCES handling_units(id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (kind IN ('received', 'stored', 'moved', 'dispatched')),
    origin_location_id TEXT REFERENCES locations(id) ON DELETE RESTRICT,
    destination_location_id TEXT REFERENCES locations(id) ON DELETE RESTRICT,
    occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms > 0),
    source TEXT NOT NULL CHECK (length(trim(source)) > 0),
    result TEXT NOT NULL DEFAULT 'accepted' CHECK (result IN ('accepted', 'rejected')),
    rejection_reason TEXT,
    created_at_ms INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    CHECK (result <> 'rejected' OR rejection_reason IS NOT NULL),
    CHECK (
        (kind = 'received' AND origin_location_id IS NULL AND destination_location_id IS NULL)
        OR (kind = 'stored' AND origin_location_id IS NULL AND destination_location_id IS NOT NULL)
        OR (kind = 'moved' AND origin_location_id IS NOT NULL AND destination_location_id IS NOT NULL
            AND origin_location_id <> destination_location_id)
        OR (kind = 'dispatched' AND origin_location_id IS NOT NULL AND destination_location_id IS NULL)
    )
);

CREATE INDEX idx_movement_events_unit_time
    ON movement_events(handling_unit_id, occurred_at_ms, event_id);
CREATE INDEX idx_movement_events_origin ON movement_events(origin_location_id);
CREATE INDEX idx_movement_events_destination ON movement_events(destination_location_id);

CREATE TRIGGER movement_events_append_only_update
BEFORE UPDATE ON movement_events
BEGIN
    SELECT RAISE(ABORT, 'movement_events_are_append_only');
END;

CREATE TRIGGER movement_events_append_only_delete
BEFORE DELETE ON movement_events
BEGIN
    SELECT RAISE(ABORT, 'movement_events_are_append_only');
END;

CREATE TABLE alerts (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(id)) > 0),
    movement_event_id TEXT NOT NULL REFERENCES movement_events(event_id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (length(trim(kind)) > 0),
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'reviewed', 'resolved')),
    resolution_event_id TEXT REFERENCES movement_events(event_id) ON DELETE RESTRICT,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms > 0)
);

CREATE INDEX idx_alerts_status_created ON alerts(status, created_at_ms DESC);
CREATE INDEX idx_alerts_movement_event ON alerts(movement_event_id);
