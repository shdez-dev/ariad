import assert from "node:assert/strict";
import test from "node:test";

import type { HandlingUnitDto, LocationDto, LocationKind } from "../src/lib/ipc.ts";
import { buildWarehouseMap, getPositionState } from "../src/lib/warehouse-map.ts";

function location(
  id: string,
  kind: LocationKind,
  parentId: string | null,
  active = true,
): LocationDto {
  return { id, code: id, name: `Nombre ${id}`, kind, parentId, active };
}

function unit(
  id: string,
  status: HandlingUnitDto["status"],
  currentLocationId: string | null,
): HandlingUnitDto {
  return {
    id,
    label: `PAL-${id}`,
    status,
    currentLocationId,
    lines: [{ sku: "SKU-1", quantity: 4, lotCode: null }],
  };
}

test("arma y ordena la jerarquía y calcula la ocupación persistida", () => {
  const model = buildWarehouseMap(
    [
      location("p-2", "position", "r-1"),
      location("z-2", "zone", null),
      location("r-1", "rack", "a-1"),
      location("p-1", "position", "r-1"),
      location("a-1", "aisle", "z-2"),
      location("p-off", "position", "r-1", false),
    ],
    [
      unit("stored", "stored", "p-1"),
      unit("received", "received", null),
      unit("dispatched", "dispatched", "p-2"),
    ],
  );

  assert.equal(model.zones.length, 1);
  assert.equal(model.zones[0].children[0].location.id, "a-1");
  assert.equal(model.zones[0].children[0].children[0].location.id, "r-1");
  assert.deepEqual(
    model.zones[0].children[0].children[0].children.map((node) => node.location.id),
    ["p-1", "p-2", "p-off"],
  );
  assert.equal(getPositionState(model.locationsById.get("p-1")!.location, model.unitsByLocation), "occupied");
  assert.equal(getPositionState(model.locationsById.get("p-2")!.location, model.unitsByLocation), "free");
  assert.equal(getPositionState(model.locationsById.get("p-off")!.location, model.unitsByLocation), "inactive");
  assert.deepEqual(model.summary, {
    zoneCount: 1,
    positionCount: 3,
    activePositionCount: 2,
    freePositionCount: 1,
    occupiedPositionCount: 1,
    inactivePositionCount: 1,
    handlingUnitCount: 2,
  });
});

test("mantiene visibles ubicaciones con una jerarquía incompleta", () => {
  const model = buildWarehouseMap(
    [location("orphan-rack", "rack", "missing-aisle"), location("orphan-position", "position", "orphan-rack")],
    [],
  );

  assert.equal(model.unassigned.length, 1);
  assert.equal(model.unassigned[0].location.id, "orphan-rack");
  assert.equal(model.unassigned[0].children[0].location.id, "orphan-position");
});

test("señala una posición con más de una unidad como inconsistente", () => {
  const model = buildWarehouseMap(
    [location("p-1", "position", "missing-rack")],
    [unit("one", "stored", "p-1"), unit("two", "stored", "p-1")],
  );

  const position = model.locationsById.get("p-1")!.location;
  assert.equal(getPositionState(position, model.unitsByLocation), "inconsistent");
  assert.equal(model.summary.occupiedPositionCount, 1);
});
