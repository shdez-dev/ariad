CREATE TABLE movement_attempts (
    attempt_id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL CHECK (length(trim(event_id)) > 0),
    handling_unit_id TEXT NOT NULL CHECK (length(trim(handling_unit_id)) > 0),
    kind TEXT NOT NULL CHECK (length(trim(kind)) > 0),
    origin_location_id TEXT,
    destination_location_id TEXT,
    occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms > 0),
    source TEXT NOT NULL CHECK (length(trim(source)) > 0),
    result TEXT NOT NULL CHECK (result IN ('duplicate', 'rejected')),
    rejection_reason TEXT,
    created_at_ms INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    CHECK (
        (result = 'duplicate' AND rejection_reason IS NULL)
        OR (result = 'rejected' AND rejection_reason IS NOT NULL)
    )
);

CREATE INDEX idx_movement_attempts_unit_time
    ON movement_attempts(handling_unit_id, occurred_at_ms, attempt_id);
CREATE INDEX idx_movement_attempts_event_id ON movement_attempts(event_id);

CREATE TABLE movement_corrections (
    correction_id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(correction_id)) > 0),
    handling_unit_id TEXT NOT NULL REFERENCES handling_units(id) ON DELETE RESTRICT,
    corrects_event_id TEXT NOT NULL REFERENCES movement_events(event_id) ON DELETE RESTRICT,
    reason TEXT NOT NULL CHECK (length(trim(reason)) BETWEEN 1 AND 500),
    occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms > 0),
    source TEXT NOT NULL CHECK (length(trim(source)) > 0),
    created_at_ms INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') AS INTEGER) * 1000),
    CHECK (correction_id <> corrects_event_id)
);

CREATE INDEX idx_movement_corrections_unit_time
    ON movement_corrections(handling_unit_id, occurred_at_ms, correction_id);
CREATE INDEX idx_movement_corrections_original
    ON movement_corrections(corrects_event_id);

CREATE TRIGGER validate_movement_correction_unit
BEFORE INSERT ON movement_corrections
WHEN NOT EXISTS (
    SELECT 1 FROM movement_events
    WHERE event_id = NEW.corrects_event_id
      AND handling_unit_id = NEW.handling_unit_id
)
BEGIN
    SELECT RAISE(ABORT, 'correction_reference_mismatch');
END;

CREATE TRIGGER validate_movement_correction_chronology
BEFORE INSERT ON movement_corrections
WHEN EXISTS (
    SELECT 1 FROM movement_events
    WHERE event_id = NEW.corrects_event_id
      AND occurred_at_ms > NEW.occurred_at_ms
)
BEGIN
    SELECT RAISE(ABORT, 'correction_precedes_original');
END;

CREATE TRIGGER prevent_movement_event_id_collision_with_correction
BEFORE INSERT ON movement_events
WHEN EXISTS (
    SELECT 1 FROM movement_corrections WHERE correction_id = NEW.event_id
)
OR EXISTS (
    SELECT 1 FROM movement_attempts
    WHERE event_id = NEW.event_id AND result = 'rejected'
)
BEGIN
    SELECT RAISE(ABORT, 'movement_event_id_conflict');
END;

CREATE TRIGGER prevent_correction_id_collision_with_history
BEFORE INSERT ON movement_corrections
WHEN EXISTS (
    SELECT 1 FROM movement_events WHERE event_id = NEW.correction_id
)
OR EXISTS (
    SELECT 1 FROM movement_attempts WHERE event_id = NEW.correction_id
)
BEGIN
    SELECT RAISE(ABORT, 'correction_id_conflict');
END;

CREATE TRIGGER prevent_movement_attempt_id_collision_with_correction
BEFORE INSERT ON movement_attempts
WHEN EXISTS (
    SELECT 1 FROM movement_corrections WHERE correction_id = NEW.event_id
)
BEGIN
    SELECT RAISE(ABORT, 'movement_attempt_id_conflict');
END;

CREATE TRIGGER movement_attempts_append_only_update
BEFORE UPDATE ON movement_attempts
BEGIN
    SELECT RAISE(ABORT, 'movement_attempts_are_append_only');
END;

CREATE TRIGGER movement_attempts_append_only_delete
BEFORE DELETE ON movement_attempts
BEGIN
    SELECT RAISE(ABORT, 'movement_attempts_are_append_only');
END;

CREATE TRIGGER movement_corrections_append_only_update
BEFORE UPDATE ON movement_corrections
BEGIN
    SELECT RAISE(ABORT, 'movement_corrections_are_append_only');
END;

CREATE TRIGGER movement_corrections_append_only_delete
BEFORE DELETE ON movement_corrections
BEGIN
    SELECT RAISE(ABORT, 'movement_corrections_are_append_only');
END;
