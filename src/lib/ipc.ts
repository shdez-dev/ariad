import { invoke } from "@tauri-apps/api/core";

export type ProductDto = {
  sku: string;
  description: string;
  unitOfMeasure: string;
  active: boolean;
};

export type LocationKind = "zone" | "aisle" | "rack" | "position";

export type LocationDto = {
  id: string;
  code: string;
  name: string;
  kind: LocationKind;
  parentId: string | null;
  active: boolean;
};

export type HandlingUnitLineDto = {
  sku: string;
  quantity: number;
  lotCode: string | null;
};

export type HandlingUnitDto = {
  id: string;
  label: string;
  status: "received" | "stored" | "dispatched";
  currentLocationId: string | null;
  lines: HandlingUnitLineDto[];
};

export type MovementOutcome = "applied" | "duplicate";

export type MovementWriteDto = {
  eventId: string;
  outcome: MovementOutcome;
};

export type CorrectionWriteDto = {
  correctionId: string;
  outcome: MovementOutcome;
};

export type MovementHistoryEntryDto = {
  recordId: string;
  eventId: string;
  handlingUnitId: string;
  kind: string;
  originLocationId: string | null;
  destinationLocationId: string | null;
  occurredAtMs: number;
  recordedAtMs: number;
  source: string;
  result: string;
  rejectionReason: string | null;
  correctsEventId: string | null;
  correctionReason: string | null;
};

export type InventorySearchResultDto = {
  kind: "product" | "handling_unit" | "location";
  id: string;
  title: string;
  subtitle: string;
  status: string;
};

export type AlertDto = {
  id: string;
  recordId: string;
  movementEventId: string | null;
  movementAttemptId?: number | null;
  targetKind?: string;
  eventId: string;
  kind: string;
  status: "open" | "reviewed" | "resolved";
  createdAtMs: number;
  handlingUnitId: string;
  handlingUnitLabel: string | null;
  eventKind: string;
  eventResult: string;
  eventRejectionReason: string | null;
  occurredAtMs: number;
  originLocationCode: string | null;
  destinationLocationCode: string | null;
  source?: string | null;
};

export type SimulationScenario =
  | "normal"
  | "duplicate"
  | "unknown_label"
  | "invalid_location"
  | "occupied_destination";

export type SimulationStepDto = {
  id: string;
  label: string;
  result: string;
  message: string;
  recordId?: string | null;
  eventId?: string | null;
  handlingUnitId?: string | null;
  alertId?: string | null;
};

export type SimulationRunInput = {
  seed: string;
  scenario: SimulationScenario;
};

export type SimulationRunDto = {
  seed: string;
  scenario: SimulationScenario;
  steps: SimulationStepDto[];
  summary: string;
};

export type IpcErrorCode =
  | "validation"
  | "conflict"
  | "not_found"
  | "event_id_conflict"
  | "inactive_product"
  | "invalid_transition"
  | "invalid_location_hierarchy"
  | "location_hierarchy_immutable"
  | "in_use"
  | "position_occupied"
  | "invalid_destination"
  | "correction_reference_mismatch"
  | "correction_before_original"
  | "invalid_alert_transition"
  | "storage";

type IpcErrorPayload = {
  code: IpcErrorCode;
  message: string;
};

export class AriadnaIpcError extends Error {
  readonly code: IpcErrorCode;

  constructor(payload: IpcErrorPayload) {
    super(payload.message);
    this.name = "AriadnaIpcError";
    this.code = payload.code;
  }
}

function isIpcErrorPayload(value: unknown): value is IpcErrorPayload {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<IpcErrorPayload>;
  return typeof candidate.code === "string" && typeof candidate.message === "string";
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    if (isIpcErrorPayload(error)) throw new AriadnaIpcError(error);
    if (error instanceof Error) throw error;
    throw new Error("No se pudo completar la comunicación con Ariadna.");
  }
}

async function callInventoryChange<T>(
  command: string,
  args: Record<string, unknown>,
): Promise<T> {
  const result = await call<T>(command, args);
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event("ariadna:inventory-changed"));
  }
  return result;
}

export function newEventId(): string {
  return globalThis.crypto.randomUUID();
}

export function newEventMetadata(): { eventId: string; occurredAtMs: number } {
  return { eventId: newEventId(), occurredAtMs: Date.now() };
}

export type ReceiveLineInput = {
  sku: string;
  quantity: number;
  lotCode?: string | null;
};

export const inventoryIpc = {
  createProduct: (input: Omit<ProductDto, "active">) =>
    call<ProductDto>("create_product", input),
  getProduct: (sku: string) => call<ProductDto | null>("get_product", { sku }),
  listProducts: (includeInactive = false) =>
    call<ProductDto[]>("list_products", { includeInactive }),
  searchInventory: (query: string) =>
    call<InventorySearchResultDto[]>("search_inventory", { query }),
  updateProduct: (input: Omit<ProductDto, "active">) =>
    call<ProductDto>("update_product", input),
  deactivateProduct: (sku: string) =>
    call<ProductDto>("deactivate_product", { sku }),

  createLocation: (input: {
    id: string;
    code: string;
    name: string;
    kind: LocationKind;
    parentId?: string | null;
  }) => callInventoryChange<LocationDto>("create_location", input),
  getLocation: (id: string) => call<LocationDto | null>("get_location", { id }),
  listLocations: (includeInactive = false) =>
    call<LocationDto[]>("list_locations", { includeInactive }),
  updateLocation: (input: { id: string; code: string; name: string }) =>
    callInventoryChange<LocationDto>("update_location", input),
  deactivateLocation: (id: string) =>
    callInventoryChange<LocationDto>("deactivate_location", { id }),

  getHandlingUnit: (identifier: string) =>
    call<HandlingUnitDto | null>("get_handling_unit", { identifier }),
  listHandlingUnits: (includeDispatched = false) =>
    call<HandlingUnitDto[]>("list_handling_units", { includeDispatched }),
  receiveUnit: (input: {
    id: string;
    label: string;
    lines: ReceiveLineInput[];
    eventId: string;
    occurredAtMs: number;
  }) => callInventoryChange<MovementWriteDto>("receive_unit", input),
  storeUnit: (input: {
    eventId: string;
    handlingUnitId: string;
    destinationLocationId: string;
    occurredAtMs: number;
  }) => callInventoryChange<MovementWriteDto>("store_unit", input),
  moveUnit: (input: {
    eventId: string;
    handlingUnitId: string;
    originLocationId: string;
    destinationLocationId: string;
    occurredAtMs: number;
  }) => callInventoryChange<MovementWriteDto>("move_unit", input),
  dispatchUnit: (input: {
    eventId: string;
    handlingUnitId: string;
    originLocationId: string;
    occurredAtMs: number;
  }) => callInventoryChange<MovementWriteDto>("dispatch_unit", input),
  recordMovementCorrection: (input: {
    correctionId: string;
    handlingUnitId: string;
    correctsEventId: string;
    reason: string;
    occurredAtMs: number;
  }) => call<CorrectionWriteDto>("record_movement_correction", input),
  getMovementHistory: (handlingUnitId: string) =>
    call<MovementHistoryEntryDto[]>("get_movement_history", { handlingUnitId }),
  listAlerts: () => call<AlertDto[]>("list_alerts"),
  markAlertReviewed: (alertId: string) =>
    call<AlertDto>("mark_alert_reviewed", { alertId }),
  runSimulation: (input: SimulationRunInput) =>
    callInventoryChange<SimulationRunDto>("run_simulation", input),
};
