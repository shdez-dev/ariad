import assert from "node:assert/strict";
import test from "node:test";

import type { HandlingUnitDto, LocationDto, ProductDto } from "../src/lib/ipc.ts";
import {
  availableLocationParents,
  filterHandlingUnits,
  filterLocations,
  filterProducts,
  getLocationPath,
  requiredLocationParentKind,
} from "../src/lib/catalog-model.ts";

const products: ProductDto[] = [
  { sku: "CAJA-01", description: "Caja de cartón", unitOfMeasure: "unidad", active: true },
  { sku: "LATA-02", description: "Lata de conserva", unitOfMeasure: "unidad", active: false },
];

const locations: LocationDto[] = [
  { id: "z1", code: "Z-1", name: "Zona Norte", kind: "zone", parentId: null, active: true },
  { id: "a1", code: "A-1", name: "Pasillo 1", kind: "aisle", parentId: "z1", active: true },
  { id: "a2", code: "A-2", name: "Pasillo 2", kind: "aisle", parentId: "z1", active: false },
  { id: "r1", code: "R-1", name: "Rack 1", kind: "rack", parentId: "a1", active: true },
  { id: "p1", code: "P-1", name: "Posición 1", kind: "position", parentId: "r1", active: true },
];

const units: HandlingUnitDto[] = [
  {
    id: "hu-1",
    label: "PALLET-01",
    status: "stored",
    currentLocationId: "p1",
    lines: [{ sku: "CAJA-01", quantity: 6, lotCode: null }],
  },
];

test("filtra productos por SKU y descripción sin ocultar los inactivos", () => {
  assert.deepEqual(filterProducts(products, "caja").map((product) => product.sku), ["CAJA-01"]);
  assert.deepEqual(filterProducts(products, "lata").map((product) => product.active), [false]);
});

test("filtra ubicaciones y unidades por identificadores legibles", () => {
  assert.deepEqual(filterLocations(locations, "norte").map((location) => location.id), ["z1"]);
  assert.deepEqual(filterHandlingUnits(units, "caja-01").map((unit) => unit.id), ["hu-1"]);
});

test("construye rutas de ubicación y limita el padre al nivel activo compatible", () => {
  assert.equal(getLocationPath(locations[4], locations), "Z-1 / A-1 / R-1 / P-1");
  assert.deepEqual(availableLocationParents("rack", locations).map((location) => location.id), ["a1"]);
  assert.deepEqual(availableLocationParents("zone", locations), []);
  assert.equal(requiredLocationParentKind("position"), "rack");
});
