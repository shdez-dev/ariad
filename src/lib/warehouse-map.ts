import type { HandlingUnitDto, LocationDto, LocationKind } from "./ipc.ts";

export type LocationTreeNode = {
  location: LocationDto;
  children: LocationTreeNode[];
};

export type PositionState = "free" | "occupied" | "inactive" | "inconsistent";

export type WarehouseMapModel = {
  zones: LocationTreeNode[];
  unassigned: LocationTreeNode[];
  locationsById: Map<string, LocationTreeNode>;
  unitsByLocation: Map<string, HandlingUnitDto[]>;
  summary: {
    zoneCount: number;
    positionCount: number;
    activePositionCount: number;
    freePositionCount: number;
    occupiedPositionCount: number;
    inactivePositionCount: number;
    handlingUnitCount: number;
  };
};

const parentKind: Record<LocationKind, LocationKind | null> = {
  zone: null,
  aisle: "zone",
  rack: "aisle",
  position: "rack",
};

const kindOrder: Record<LocationKind, number> = {
  zone: 0,
  aisle: 1,
  rack: 2,
  position: 3,
};

function compareLocations(left: LocationDto, right: LocationDto): number {
  return (
    kindOrder[left.kind] - kindOrder[right.kind] ||
    left.code.localeCompare(right.code, "es", { numeric: true, sensitivity: "base" }) ||
    left.id.localeCompare(right.id)
  );
}

function sortTree(nodes: LocationTreeNode[]): void {
  nodes.sort((left, right) => compareLocations(left.location, right.location));
  for (const node of nodes) sortTree(node.children);
}

export function buildWarehouseMap(
  locations: LocationDto[],
  handlingUnits: HandlingUnitDto[],
): WarehouseMapModel {
  const locationsById = new Map<string, LocationTreeNode>();
  for (const location of locations) {
    locationsById.set(location.id, { location, children: [] });
  }

  const zones: LocationTreeNode[] = [];
  const unassigned: LocationTreeNode[] = [];

  for (const node of locationsById.values()) {
    const expectedParentKind = parentKind[node.location.kind];
    if (expectedParentKind === null && node.location.parentId === null) {
      zones.push(node);
      continue;
    }

    const parent = node.location.parentId
      ? locationsById.get(node.location.parentId)
      : undefined;
    if (parent && parent.location.kind === expectedParentKind) {
      parent.children.push(node);
    } else {
      unassigned.push(node);
    }
  }

  sortTree(zones);
  sortTree(unassigned);

  const unitsByLocation = new Map<string, HandlingUnitDto[]>();
  const visibleUnits = handlingUnits.filter((unit) => unit.status !== "dispatched");
  for (const unit of visibleUnits) {
    if (unit.status !== "stored" || unit.currentLocationId === null) continue;
    const units = unitsByLocation.get(unit.currentLocationId) ?? [];
    units.push(unit);
    unitsByLocation.set(unit.currentLocationId, units);
  }

  const allNodes = [...locationsById.values()];
  const positions = allNodes.filter((node) => node.location.kind === "position");
  const freePositionCount = positions.filter(
    (node) => node.location.active && getPositionState(node.location, unitsByLocation) === "free",
  ).length;
  const occupiedPositionCount = positions.filter((node) => {
    const state = getPositionState(node.location, unitsByLocation);
    return state === "occupied" || state === "inconsistent";
  }).length;
  const inactivePositionCount = positions.filter((node) => !node.location.active).length;

  return {
    zones,
    unassigned,
    locationsById,
    unitsByLocation,
    summary: {
      zoneCount: allNodes.filter((node) => node.location.kind === "zone").length,
      positionCount: positions.length,
      activePositionCount: positions.filter((node) => node.location.active).length,
      freePositionCount,
      occupiedPositionCount,
      inactivePositionCount,
      handlingUnitCount: visibleUnits.length,
    },
  };
}

export function getPositionState(
  location: LocationDto,
  unitsByLocation: ReadonlyMap<string, HandlingUnitDto[]>,
): PositionState {
  if (!location.active) return "inactive";
  const count = unitsByLocation.get(location.id)?.length ?? 0;
  if (count > 1) return "inconsistent";
  return count === 1 ? "occupied" : "free";
}
