CREATE TABLE alerts_new (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(id)) > 0),
    movement_event_id TEXT REFERENCES movement_events(event_id) ON DELETE RESTRICT,
    movement_attempt_id INTEGER REFERENCES movement_attempts(attempt_id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (length(trim(kind)) > 0),
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'reviewed', 'resolved')),
    resolution_event_id TEXT REFERENCES movement_events(event_id) ON DELETE RESTRICT,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms > 0),
    CHECK (
        (movement_event_id IS NOT NULL AND movement_attempt_id IS NULL)
        OR (movement_event_id IS NULL AND movement_attempt_id IS NOT NULL)
    )
);

INSERT INTO alerts_new (
    id,
    movement_event_id,
    movement_attempt_id,
    kind,
    status,
    resolution_event_id,
    created_at_ms
)
SELECT
    id,
    movement_event_id,
    NULL,
    kind,
    status,
    resolution_event_id,
    created_at_ms
FROM alerts;

DROP TABLE alerts;
ALTER TABLE alerts_new RENAME TO alerts;

CREATE INDEX idx_alerts_status_created ON alerts(status, created_at_ms DESC);
CREATE INDEX idx_alerts_movement_event ON alerts(movement_event_id);
CREATE INDEX idx_alerts_movement_attempt ON alerts(movement_attempt_id);

CREATE TRIGGER alerts_record_target_immutable
BEFORE UPDATE OF movement_event_id, movement_attempt_id ON alerts
WHEN NEW.movement_event_id IS NOT OLD.movement_event_id
  OR NEW.movement_attempt_id IS NOT OLD.movement_attempt_id
BEGIN
    SELECT RAISE(ABORT, 'alert_record_target_is_immutable');
END;
