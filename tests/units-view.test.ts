import assert from "node:assert/strict";
import test from "node:test";

import {
  AriadnaIpcError,
  inventoryIpc,
  type HandlingUnitDto,
  type LocationDto,
  type MovementHistoryEntryDto,
} from "../src/lib/ipc.ts";
import { mountUnitsView } from "../src/views/units-view.ts";

type Listener = (event: { preventDefault(): void }) => void;

let focusedElement: FakeElement | null = null;
let scrolledElement: FakeElement | null = null;

class FakeElement {
  readonly tagName: string;
  readonly children: FakeElement[] = [];
  readonly dataset: Record<string, string> = {};
  readonly attributes = new Map<string, string>();
  disabled = false;
  hidden = false;
  value = "";
  className = "";
  type = "";
  tabIndex = 0;
  colSpan = 1;
  private ownText = "";
  private parent: FakeElement | null = null;
  private readonly listeners = new Map<string, Listener[]>();

  constructor(tagName = "div") {
    this.tagName = tagName;
  }

  get textContent(): string {
    return this.ownText + this.children.map((child) => child.textContent).join("");
  }

  set textContent(value: string) {
    this.ownText = value;
    for (const child of this.children) child.parent = null;
    this.children.splice(0);
  }

  append(...children: FakeElement[]): void {
    for (const child of children) {
      child.parent = this;
      this.children.push(child);
    }
  }

  replaceChildren(...children: FakeElement[]): void {
    this.textContent = "";
    this.append(...children);
  }

  addEventListener(name: string, listener: Listener): void {
    const listeners = this.listeners.get(name) ?? [];
    listeners.push(listener);
    this.listeners.set(name, listeners);
  }

  dispatch(name: string): void {
    for (const listener of this.listeners.get(name) ?? []) listener({ preventDefault() {} });
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  focus(): void {
    focusedElement = this;
  }

  scrollIntoView(): void {
    scrolledElement = this;
  }

  querySelector<T extends Element>(selector: string): T | null {
    return this.querySelectorAll<T>(selector)[0] ?? null;
  }

  querySelectorAll<T extends Element>(selector: string): T[] {
    const matches: FakeElement[] = [];
    const visit = (node: FakeElement) => {
      for (const child of node.children) {
        if (matchesSelector(child, selector)) matches.push(child);
        visit(child);
      }
    };
    visit(this);
    return matches as unknown as T[];
  }
}

function matchesSelector(element: FakeElement, selector: string): boolean {
  if (selector.startsWith(".")) return element.className.split(/\s+/).includes(selector.slice(1));
  const attribute = selector.match(/^\[data-([a-z-]+)\]$/);
  if (!attribute) return false;
  const key = attribute[1].replace(/-([a-z])/g, (_match, letter: string) => letter.toUpperCase());
  return key in element.dataset;
}

type UnitElements = Record<string, FakeElement>;

async function withUnitsView(
  overrides: Partial<typeof inventoryIpc>,
  run: (elements: UnitElements, view: ReturnType<typeof mountUnitsView>) => Promise<void>,
): Promise<void> {
  const elements: UnitElements = {
    "#units-table-body": new FakeElement("tbody"),
    "#units-status": new FakeElement("p"),
    "#units-filter": new FakeElement("input"),
    "#units-refresh": new FakeElement("button"),
    "#unit-details": new FakeElement("aside"),
    "#unit-detail-announcement": new FakeElement("p"),
  };
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const previousIpc = new Map(
    Object.keys(overrides).map((key) => [key, Object.getOwnPropertyDescriptor(inventoryIpc, key)]),
  );
  focusedElement = null;
  scrolledElement = null;

  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: {
      querySelector<T extends Element>(selector: string): T | null {
        return (elements[selector] as unknown as T | undefined) ?? null;
      },
      createElement<T extends HTMLElement>(tagName: string): T {
        return new FakeElement(tagName) as unknown as T;
      },
    },
  });
  Object.assign(inventoryIpc, overrides);

  try {
    const view = mountUnitsView();
    await run(elements, view);
  } finally {
    for (const [key, descriptor] of previousIpc) {
      if (descriptor) Object.defineProperty(inventoryIpc, key, descriptor);
      else delete (inventoryIpc as Record<string, unknown>)[key];
    }
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else delete (globalThis as typeof globalThis & { document?: unknown }).document;
  }
}

async function waitFor(condition: () => boolean, description: string): Promise<void> {
  const deadline = Date.now() + 2_000;
  while (!condition()) {
    if (Date.now() >= deadline) throw new Error(`Tiempo de espera agotado: ${description}`);
    await new Promise<void>((resolve) => setImmediate(resolve));
  }
}

function location(
  id: string,
  code: string,
  kind: LocationDto["kind"],
  parentId: string | null,
): LocationDto {
  return { id, code, name: `Nombre ${code}`, kind, parentId, active: true };
}

function historyEntry(overrides: Partial<MovementHistoryEntryDto> = {}): MovementHistoryEntryDto {
  return {
    recordId: "record-store-1",
    eventId: "event-store-1",
    handlingUnitId: "unit-1",
    kind: "stored",
    originLocationId: null,
    destinationLocationId: "position-1",
    occurredAtMs: 1_800_000_000_000,
    recordedAtMs: 1_800_000_000_100,
    source: "manual",
    result: "accepted",
    rejectionReason: null,
    correctsEventId: null,
    correctionReason: null,
    ...overrides,
  };
}

test("lista, filtra y abre el detalle persistido con la cronología de la unidad", async () => {
  const unit: HandlingUnitDto = {
    id: "unit-1",
    label: "PAL-A",
    status: "stored",
    currentLocationId: "position-1",
    lines: [{ sku: "SKU-A", quantity: 4, lotCode: "LOT-A" }],
  };
  const otherUnit: HandlingUnitDto = {
    id: "unit-2",
    label: "PAL-B",
    status: "received",
    currentLocationId: null,
    lines: [{ sku: "SKU-B", quantity: 1, lotCode: null }],
  };
  const locations = [
    location("zone-1", "Z-1", "zone", null),
    location("aisle-1", "A-1", "aisle", "zone-1"),
    location("rack-1", "R-1", "rack", "aisle-1"),
    location("position-1", "P-01", "position", "rack-1"),
  ];
  const history = [
    historyEntry(),
    historyEntry({
      recordId: "record-rejected-1",
      eventId: "event-rejected-1",
      kind: "moved",
      originLocationId: "position-1",
      destinationLocationId: "position-2",
      occurredAtMs: 1_800_000_010_000,
      recordedAtMs: 1_800_000_010_100,
      source: "simulator",
      result: "rejected",
      rejectionReason: "position_occupied",
    }),
  ];

  await withUnitsView({
    listHandlingUnits: async () => [unit, otherUnit],
    listLocations: async () => locations,
    getHandlingUnit: async (identifier) => identifier === unit.id ? unit : null,
    getMovementHistory: async () => history,
  }, async (elements, view) => {
    await view.refresh();
    assert.equal(elements["#units-table-body"].children.length, 2);
    assert.match(elements["#units-status"].textContent, /2 unidades cargadas/);
    assert.equal(elements["#units-refresh"].disabled, false);
    assert.equal(elements["#units-refresh"].attributes.has("aria-busy"), false);

    const filter = elements["#units-filter"];
    filter.value = "sku-a";
    filter.dispatch("input");
    assert.equal(elements["#units-table-body"].children.length, 1);
    const row = elements["#units-table-body"].children[0];
    assert.match(row.textContent, /PAL-A/);
    assert.match(row.textContent, /Almacenado/);
    assert.match(row.textContent, /Z-1/);
    const openButton = row.children[4].children[0];
    assert.equal(openButton.attributes.get("aria-label"), "Ver detalle del pallet PAL-A");

    filter.value = "sin-coincidencia";
    filter.dispatch("input");
    assert.equal(elements["#units-table-body"].children.length, 1);
    assert.match(elements["#units-table-body"].textContent, /No hay unidades que coincidan con el filtro/);

    filter.value = "SKU-A";
    filter.dispatch("input");
    elements["#units-table-body"].children[0].children[4].children[0].dispatch("click");
    await waitFor(
      () => elements["#unit-detail-announcement"].textContent.includes("Detalle e historial del pallet PAL-A cargados"),
      "la consulta de detalle e historial",
    );

    const detail = elements["#unit-details"];
    assert.match(detail.textContent, /PAL-A/);
    assert.match(detail.textContent, /unit-1/);
    assert.match(detail.textContent, /Almacenado/);
    assert.match(detail.textContent, /Ubicación/);
    assert.match(detail.textContent, /P-01/);
    assert.match(detail.textContent, /SKU-A/);
    assert.match(detail.textContent, /Lote LOT-A/);
    assert.match(detail.textContent, /× 4/);
    assert.match(detail.textContent, /Cronología de movimientos/);
    assert.match(detail.textContent, /Aceptado/);
    assert.match(detail.textContent, /Rechazado/);
    assert.match(detail.textContent, /Fecha simulada/);
    assert.match(detail.textContent, /registrado/);
    assert.match(detail.textContent, /position_occupied/);
    assert.equal(detail.attributes.has("aria-busy"), false);
    const timeline = detail.querySelector<FakeElement>(".movement-history-list");
    assert.deepEqual(timeline?.children.map((entry) => entry.dataset.historyRecordId), [
      "record-store-1",
      "record-rejected-1",
    ]);
  });
});

test("abre y enfoca el intento exacto aunque no exista una ficha de unidad", async () => {
  const history = [
    historyEntry({
      recordId: "record-other",
      eventId: "event-shared",
      handlingUnitId: "unknown-label",
      kind: "received",
      result: "rejected",
      rejectionReason: "handling_unit_not_found",
    }),
    historyEntry({
      recordId: "record-target",
      eventId: "event-shared",
      handlingUnitId: "unknown-label",
      kind: "stored",
      destinationLocationId: null,
      result: "rejected",
      rejectionReason: "location_not_found_or_invalid",
    }),
  ];
  const queried: string[] = [];

  await withUnitsView({
    listHandlingUnits: async () => [],
    listLocations: async () => [],
    getHandlingUnit: async (identifier) => {
      queried.push(`unit:${identifier}`);
      return null;
    },
    getMovementHistory: async (identifier) => {
      queried.push(`history:${identifier}`);
      return history;
    },
  }, async (elements, view) => {
    await view.refresh();
    await view.openByIdentifier("SIM-UNKNOWN-LABEL", "record-target");

    const detail = elements["#unit-details"];
    assert.deepEqual(queried, ["unit:SIM-UNKNOWN-LABEL", "history:SIM-UNKNOWN-LABEL"]);
    assert.match(detail.textContent, /Intento sin unidad registrada/);
    assert.match(detail.textContent, /Registro exacto/);
    assert.match(detail.textContent, /record-target/);
    assert.match(detail.textContent, /location_not_found_or_invalid/);
    assert.doesNotMatch(detail.textContent, /record-other/);
    assert.equal(focusedElement?.dataset.historyRecordId, "record-target");
    assert.equal(scrolledElement?.dataset.historyRecordId, "record-target");
    assert.match(elements["#units-status"].textContent, /Unidad no registrada/);
    assert.match(elements["#unit-detail-announcement"].textContent, /Se abrió el registro de historial sin ficha de unidad/);
    assert.equal(detail.attributes.has("aria-busy"), false);
  });
});

test("expone el error de carga y libera los controles ocupados", async () => {
  await withUnitsView({
    listHandlingUnits: async () => {
      throw new AriadnaIpcError({ code: "storage", message: "SQLite no está disponible." });
    },
    listLocations: async () => [],
  }, async (elements, view) => {
    await view.refresh();

    assert.equal(elements["#units-table-body"].children.length, 1);
    assert.match(elements["#units-table-body"].textContent, /No se pudo cargar la lista de unidades/);
    assert.equal(elements["#units-status"].dataset.kind, "error");
    assert.match(elements["#units-status"].textContent, /SQLite no está disponible/);
    assert.match(elements["#unit-details"].textContent, /SQLite no está disponible/);
    assert.match(elements["#unit-detail-announcement"].textContent, /No se pudo cargar la lista de unidades/);
    assert.equal(elements["#units-refresh"].disabled, false);
    assert.equal(elements["#units-refresh"].attributes.has("aria-busy"), false);
    assert.equal(elements["#unit-details"].attributes.has("aria-busy"), false);
  });
});
