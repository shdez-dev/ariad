import type { HandlingUnitDto, LocationDto, LocationKind, ProductDto } from "./ipc.ts";

const expectedParentKind: Record<LocationKind, LocationKind | null> = {
  zone: null,
  aisle: "zone",
  rack: "aisle",
  position: "rack",
};

export const locationKindLabels: Record<LocationKind, string> = {
  zone: "Zona",
  aisle: "Pasillo",
  rack: "Rack",
  position: "Posición",
};

export function requiredLocationParentKind(kind: LocationKind): LocationKind | null {
  return expectedParentKind[kind];
}

function normalizedQuery(query: string): string {
  return query.trim().toLocaleLowerCase("es");
}

export function filterProducts(products: ProductDto[], query: string): ProductDto[] {
  const term = normalizedQuery(query);
  if (!term) return products;
  return products.filter((product) =>
    `${product.sku} ${product.description} ${product.unitOfMeasure}`
      .toLocaleLowerCase("es")
      .includes(term),
  );
}

export function filterLocations(locations: LocationDto[], query: string): LocationDto[] {
  const term = normalizedQuery(query);
  if (!term) return locations;
  return locations.filter((location) =>
    `${location.code} ${location.name} ${locationKindLabels[location.kind]}`
      .toLocaleLowerCase("es")
      .includes(term),
  );
}

export function filterHandlingUnits(units: HandlingUnitDto[], query: string): HandlingUnitDto[] {
  const term = normalizedQuery(query);
  if (!term) return units;
  return units.filter((unit) =>
    `${unit.id} ${unit.label} ${unit.status} ${unit.lines.map((line) => line.sku).join(" ")}`
      .toLocaleLowerCase("es")
      .includes(term),
  );
}

export function unitsAwaitingStorage(units: HandlingUnitDto[]): HandlingUnitDto[] {
  return units.filter((unit) => unit.status === "received" && unit.currentLocationId === null);
}

export function storedUnits(units: HandlingUnitDto[]): HandlingUnitDto[] {
  return units.filter((unit) => unit.status === "stored" && unit.currentLocationId !== null);
}

export function activePositions(locations: LocationDto[]): LocationDto[] {
  return locations
    .filter((location) => location.kind === "position" && location.active)
    .sort((left, right) =>
      left.code.localeCompare(right.code, "es", { numeric: true, sensitivity: "base" }),
    );
}

export function getLocationPath(
  location: LocationDto | null | undefined,
  locations: LocationDto[],
): string {
  if (!location) return "Sin ubicación";
  const locationsById = new Map(locations.map((item) => [item.id, item]));
  const path: string[] = [];
  const visited = new Set<string>();
  let current: LocationDto | undefined = location;

  while (current && !visited.has(current.id)) {
    visited.add(current.id);
    path.push(current.code);
    current = current.parentId ? locationsById.get(current.parentId) : undefined;
  }

  if (current) path.push("Jerarquía inválida");
  return path.reverse().join(" / ");
}

export function availableLocationParents(
  kind: LocationKind,
  locations: LocationDto[],
): LocationDto[] {
  const parentKind = expectedParentKind[kind];
  if (!parentKind) return [];
  return locations
    .filter((location) => location.kind === parentKind && location.active)
    .sort((left, right) =>
      left.code.localeCompare(right.code, "es", { numeric: true, sensitivity: "base" }),
    );
}

