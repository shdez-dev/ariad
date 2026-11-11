CREATE TRIGGER IF NOT EXISTS prevent_location_reparent
BEFORE UPDATE OF parent_id ON locations
WHEN NEW.parent_id IS NOT OLD.parent_id
BEGIN
    SELECT RAISE(ABORT, 'location_parent_is_immutable');
END;
