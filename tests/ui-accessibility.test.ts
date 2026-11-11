import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { locationKindLabels } from "../src/lib/catalog-model.ts";

test("los niveles de ubicación se presentan en español", () => {
  assert.equal(locationKindLabels.zone, "Zona");
  assert.equal(locationKindLabels.aisle, "Pasillo");
  assert.equal(locationKindLabels.rack, "Rack");
  assert.equal(locationKindLabels.position, "Posición");
});

test("los cuatro filtros tienen nombres accesibles estables", async () => {
  const html = await readFile(new URL("../index.html", import.meta.url), "utf8");
  const inputTags = html.match(/<input\b[^>]*>/g) ?? [];
  const expectedFields = [
    ["products-filter", "Filtrar productos por SKU o descripción"],
    ["locations-filter", "Filtrar ubicaciones por código, nombre o nivel"],
    ["units-filter", "Filtrar unidades por etiqueta, ID, SKU o ubicación"],
    ["inventory-search-input", "Buscar productos, pallets o ubicaciones"],
  ] as const;

  for (const [id, accessibleName] of expectedFields) {
    const input = inputTags.find((tag) => tag.match(/\bid="([^"]+)"/)?.[1] === id);
    assert.ok(input, `No se encontró el control ${id}`);
    assert.equal(input.match(/\baria-label="([^"]+)"/)?.[1], accessibleName, id);
  }
});
