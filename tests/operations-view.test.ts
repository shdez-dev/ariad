import assert from "node:assert/strict";
import test from "node:test";

import {
  AriadnaIpcError,
  inventoryIpc,
  type HandlingUnitDto,
  type LocationDto,
  type MovementHistoryEntryDto,
  type MovementWriteDto,
} from "../src/lib/ipc.ts";
import { mountOperationsView } from "../src/views/operations-view.ts";

type Listener = (event: { preventDefault(): void }) => void;

let focusedElement: FakeElement | null = null;

class FakeElement {
  readonly tagName: string;
  readonly children: FakeElement[] = [];
  readonly dataset: Record<string, string> = {};
  readonly attributes = new Map<string, string>();
  readonly classList = {
    toggle: (name: string, force?: boolean) => {
      const classes = new Set(this.className.split(/\s+/).filter(Boolean));
      const shouldHave = force ?? !classes.has(name);
      if (shouldHave) classes.add(name);
      else classes.delete(name);
      this.className = [...classes].join(" ");
      return shouldHave;
    },
  };
  value = "";
  className = "";
  type = "";
  title = "";
  hidden = false;
  disabled = false;
  required = false;
  min = "";
  max = "";
  step = "";
  maxLength = 0;
  autocomplete = "";
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

  focus(): void {
    focusedElement = this;
  }

  reportValidity(): boolean {
    return true;
  }

  reset(): void {}

  remove(): void {
    if (!this.parent) return;
    const index = this.parent.children.indexOf(this);
    if (index >= 0) this.parent.children.splice(index, 1);
    this.parent = null;
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

const products = [{ sku: "SKU-1", description: "Producto de prueba", unitOfMeasure: "unidad", active: true }];

function operationSelectors(): string[] {
  return [
    "#operations-status", "#operations-refresh", "#operation-result-status", "#operation-availability",
    "#operation-retry", "#operation-history-details", "#operation-history-note",
    "#receive-operation-form", "#receive-operation-fields", "#receive-label", "#receive-lines",
    "#receive-line-add", "#receive-submit", "#store-operation-form", "#store-operation-fields",
    "#store-unit", "#store-unit-summary", "#store-destination", "#store-submit",
    "#move-operation-form", "#move-operation-fields", "#move-unit", "#move-origin-summary",
    "#move-destination", "#move-submit", "#dispatch-operation-form", "#dispatch-operation-fields",
    "#dispatch-unit", "#dispatch-origin-summary", "#dispatch-submit",
  ];
}

async function withOperationsView(
  overrides: Partial<typeof inventoryIpc>,
  run: (elements: Map<string, FakeElement>) => Promise<void>,
  confirm: () => boolean = () => true,
): Promise<void> {
  const elements = new Map(operationSelectors().map((selector) => [selector, new FakeElement()]));
  const tabs = ["receive", "store", "move", "dispatch"].map((mode) => {
    const tab = new FakeElement("button");
    tab.dataset.operationTab = mode;
    return tab;
  });
  for (const tab of tabs) elements.set(`tab:${tab.dataset.operationTab}`, tab);
  const panels = ["receive", "store", "move", "dispatch"].map((mode) => {
    const panel = new FakeElement("section");
    panel.dataset.operationPanel = mode;
    return panel;
  });
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousIpc = new Map(
    Object.keys(overrides).map((key) => [key, Object.getOwnPropertyDescriptor(inventoryIpc, key)]),
  );
  focusedElement = null;
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: {
      querySelector<T extends Element>(selector: string): T | null {
        return (elements.get(selector) as unknown as T | undefined) ?? null;
      },
      querySelectorAll<T extends Element>(selector: string): T[] {
        const matches = selector === "[data-operation-tab]" ? tabs : selector === "[data-operation-panel]" ? panels : [];
        return matches as unknown as T[];
      },
      createElement<T extends HTMLElement>(tagName: string): T {
        return new FakeElement(tagName) as unknown as T;
      },
    },
  });
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { confirm },
  });
  Object.assign(inventoryIpc, overrides);

  try {
    const view = mountOperationsView();
    await view.refresh();
    await run(elements);
  } finally {
    for (const [key, descriptor] of previousIpc) {
      if (descriptor) Object.defineProperty(inventoryIpc, key, descriptor);
      else delete (inventoryIpc as Record<string, unknown>)[key];
    }
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else delete (globalThis as typeof globalThis & { document?: unknown }).document;
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else delete (globalThis as typeof globalThis & { window?: unknown }).window;
  }
}

async function waitFor(condition: () => boolean, description: string): Promise<void> {
  const deadline = Date.now() + 5_000;
  while (!condition()) {
    if (Date.now() >= deadline) throw new Error(`Tiempo de espera agotado: ${description}`);
    await new Promise<void>((resolve) => setImmediate(resolve));
  }
}

function prepareReceipt(elements: Map<string, FakeElement>): void {
  elements.get("#receive-label")!.value = "Recepción de prueba";
  const line = elements.get("#receive-lines")!.children[0];
  assert.ok(line, "la vista debe crear una línea inicial de recepción");
  line.querySelector<HTMLSelectElement>("[data-receive-sku]")!.value = "SKU-1";
  line.querySelector<HTMLInputElement>("[data-receive-quantity]")!.value = "3";
  line.querySelector<HTMLInputElement>("[data-receive-lot]")!.value = "LOTE-7";
}

test("reintenta una recepción incierta con el mismo identificador y payload y reconoce el duplicado", async () => {
  const writes: Array<Record<string, unknown>> = [];
  let writeAttempt = 0;
  let handlingUnitId = "";
  await withOperationsView({
    listProducts: async () => products,
    listHandlingUnits: async () => [],
    listLocations: async () => [],
    receiveUnit: async (input) => {
      writes.push(structuredClone(input));
      handlingUnitId = input.id;
      writeAttempt += 1;
      if (writeAttempt === 1) throw new AriadnaIpcError({ code: "storage", message: "No se pudo confirmar el almacenamiento." });
      return { eventId: input.eventId, outcome: "duplicate" };
    },
    getMovementHistory: async () => [{
      recordId: "record-1",
      eventId: writes[0].eventId as string,
      handlingUnitId,
      kind: "received",
      originLocationId: null,
      destinationLocationId: null,
      occurredAtMs: 1_800_000_000_000,
      recordedAtMs: 1_800_000_000_100,
      source: "manual",
      result: "duplicate",
      rejectionReason: null,
      correctsEventId: null,
      correctionReason: null,
    }],
  }, async (elements) => {
    prepareReceipt(elements);
    elements.get("#receive-operation-form")!.dispatch("submit");

    const retry = elements.get("#operation-retry")!;
    await waitFor(() => writes.length === 1 && !retry.hidden, "aparece el reintento de recepción");
    assert.equal(retry.hidden, false, "un fallo de almacenamiento debe ofrecer un reintento explícito");
    assert.equal(elements.get("#operation-result-status")!.dataset.kind, "error");
    assert.match(elements.get("#operation-result-status")!.textContent, /No se pudo confirmar el almacenamiento/);
    assert.equal(elements.get("#receive-operation-fields")!.disabled, true);
    assert.match(elements.get("#operation-availability")!.textContent, /datos e identificador se conservan/);

    retry.dispatch("click");
    await waitFor(
      () => retry.hidden && elements.get("#operation-result-status")!.textContent.includes("ya estaba aplicado"),
      "se confirma el duplicado de recepción",
    );

    assert.equal(writes.length, 2);
    assert.deepEqual(writes[1], writes[0], "el reintento debe conservar todo el payload de la recepción");
    assert.equal(retry.hidden, true, "una respuesta confirmada debe limpiar el reintento pendiente");
    assert.match(elements.get("#operation-result-status")!.textContent, /ya estaba aplicado; no se repitieron sus efectos/);
    assert.match(elements.get("#operation-history-details")!.textContent, /Duplicado, sin repetir efectos/);
  });
});

test("mantiene la confirmación de Rust cuando falla la actualización posterior a la recepción", async () => {
  let productReads = 0;
  await withOperationsView({
    listProducts: async () => {
      productReads += 1;
      if (productReads >= 3) throw new Error("SQLite no respondió durante la actualización.");
      return products;
    },
    listHandlingUnits: async () => [],
    listLocations: async () => [],
    receiveUnit: async (input) => ({ eventId: input.eventId, outcome: "applied" }),
    getMovementHistory: async () => [],
  }, async (elements) => {
    prepareReceipt(elements);
    elements.get("#receive-operation-form")!.dispatch("submit");
    await waitFor(
      () => elements.get("#operation-result-status")!.textContent.toLocaleLowerCase("es-CL").includes("no se pudieron actualizar los selectores"),
      "se muestra la confirmación y el fallo de recarga",
    );

    assert.match(elements.get("#operation-result-status")!.textContent, /Recepción confirmada por Rust/);
    assert.match(elements.get("#operation-result-status")!.textContent, /no se pudieron actualizar los selectores/i);
    assert.equal(elements.get("#receive-operation-fields")!.disabled, true, "los formularios dependientes deben bloquearse con datos obsoletos");
    assert.equal(elements.get("#operation-retry")!.hidden, true, "una escritura confirmada no debe ofrecerse para reintento incierto");
  });
});

test("el reintento de traslado conserva evento, origen y destino", async () => {
  const unit = {
    id: "unit-7",
    label: "Pallet de prueba",
    status: "stored" as const,
    currentLocationId: "position-a",
    lines: [{ sku: "SKU-1", quantity: 2, lotCode: "LOTE-7" }],
  };
  const locations = [
    { id: "position-a", code: "P-01", name: "Posición A", kind: "position" as const, parentId: null, active: true },
    { id: "position-b", code: "P-02", name: "Posición B", kind: "position" as const, parentId: null, active: true },
  ];
  const writes: Array<Record<string, unknown>> = [];
  let attempts = 0;
  await withOperationsView({
    listProducts: async () => products,
    listHandlingUnits: async () => [unit],
    listLocations: async () => locations,
    moveUnit: async (input) => {
      writes.push(structuredClone(input));
      attempts += 1;
      if (attempts === 1) throw new AriadnaIpcError({ code: "storage", message: "No se confirmó el traslado." });
      return { eventId: input.eventId, outcome: "duplicate" };
    },
    getMovementHistory: async () => [{
      recordId: "move-record-7",
      eventId: writes[0].eventId as string,
      handlingUnitId: unit.id,
      kind: "moved",
      originLocationId: "position-a",
      destinationLocationId: "position-b",
      occurredAtMs: 1_800_000_000_000,
      recordedAtMs: 1_800_000_000_100,
      source: "manual",
      result: "duplicate",
      rejectionReason: null,
      correctsEventId: null,
      correctionReason: null,
    }],
  }, async (elements) => {
    const moveTab = [...document.querySelectorAll<HTMLButtonElement>("[data-operation-tab]")]
      .find((tab) => tab.dataset.operationTab === "move");
    assert.ok(moveTab, "la vista debe exponer la pestaña de traslado");
    moveTab.dispatch("click");
    const movePanel = [...document.querySelectorAll<HTMLElement>("[data-operation-panel]")]
      .find((panel) => panel.dataset.operationPanel === "move");
    assert.ok(movePanel);
    assert.equal(movePanel.hidden, false, "la pestaña debe mostrar el formulario de traslado");
    elements.get("#move-unit")!.value = unit.id;
    elements.get("#move-destination")!.value = "position-b";
    elements.get("#move-operation-form")!.dispatch("submit");

    const retry = elements.get("#operation-retry")!;
    await waitFor(() => writes.length === 1 && !retry.hidden, "aparece el reintento de traslado");
    retry.dispatch("click");
    await waitFor(
      () => retry.hidden && elements.get("#operation-result-status")!.textContent.includes("ya estaba aplicado"),
      "se confirma el traslado duplicado",
    );

    assert.equal(writes.length, 2);
    assert.deepEqual(writes[1], writes[0], "el reintento debe conservar eventId, unidad, origen y destino");
    assert.equal(writes[0].handlingUnitId, unit.id);
    assert.equal(writes[0].originLocationId, "position-a");
    assert.equal(writes[0].destinationLocationId, "position-b");
    assert.match(elements.get("#operation-history-details")!.textContent, /Duplicado, sin repetir efectos/);
  });
});

test("completa recepción, asignación y traslado con datos refrescados y el historial exacto", async () => {
  let unitId = "";
  const locations: LocationDto[] = [
    { id: "position-1", code: "P-01", name: "Posición 1", kind: "position", parentId: null, active: true },
    { id: "position-2", code: "P-02", name: "Posición 2", kind: "position", parentId: null, active: true },
    { id: "position-3", code: "P-03", name: "Posición 3", kind: "position", parentId: null, active: true },
  ];
  let unit: HandlingUnitDto | null = null;
  const history: MovementHistoryEntryDto[] = [];
  let receiptInput: Parameters<typeof inventoryIpc.receiveUnit>[0] | null = null;
  let storeInput: Parameters<typeof inventoryIpc.storeUnit>[0] | null = null;
  let moveInput: Parameters<typeof inventoryIpc.moveUnit>[0] | null = null;
  let recordNumber = 0;

  const addHistory = (
    eventId: string,
    kind: string,
    originLocationId: string | null,
    destinationLocationId: string | null,
    occurredAtMs: number,
  ) => {
    recordNumber += 1;
    history.unshift({
      recordId: `record-${recordNumber}`,
      eventId,
      handlingUnitId: unitId,
      kind,
      originLocationId,
      destinationLocationId,
      occurredAtMs,
      recordedAtMs: occurredAtMs,
      source: "manual",
      result: "accepted",
      rejectionReason: null,
      correctsEventId: null,
      correctionReason: null,
    });
  };

  await withOperationsView({
    listProducts: async () => products,
    listHandlingUnits: async () => unit ? [unit] : [],
    listLocations: async () => locations,
    receiveUnit: async (input) => {
      receiptInput = structuredClone(input);
      unitId = input.id;
      unit = {
        id: input.id,
        label: input.label,
        status: "received",
        currentLocationId: null,
        lines: input.lines.map((line) => ({ ...line })),
      };
      addHistory(input.eventId, "received", null, null, input.occurredAtMs);
      return { eventId: input.eventId, outcome: "applied" };
    },
    storeUnit: async (input) => {
      storeInput = structuredClone(input);
      assert.ok(unit);
      assert.equal(unit.status, "received");
      unit = { ...unit, status: "stored", currentLocationId: input.destinationLocationId };
      addHistory(input.eventId, "stored", null, input.destinationLocationId, input.occurredAtMs);
      return { eventId: input.eventId, outcome: "applied" };
    },
    moveUnit: async (input) => {
      moveInput = structuredClone(input);
      assert.ok(unit);
      assert.equal(unit.status, "stored");
      assert.equal(unit.currentLocationId, input.originLocationId);
      unit = { ...unit, currentLocationId: input.destinationLocationId };
      addHistory(input.eventId, "moved", input.originLocationId, input.destinationLocationId, input.occurredAtMs);
      return { eventId: input.eventId, outcome: "applied" };
    },
    getMovementHistory: async (requestedUnitId) => {
      assert.equal(requestedUnitId, unitId);
      return history;
    },
  }, async (elements) => {
    prepareReceipt(elements);
    elements.get("#receive-operation-form")!.dispatch("submit");
    await waitFor(
      () => elements.get("#operation-result-status")!.dataset.kind === "success"
        && elements.get("#store-unit")!.children.some((item) => item.value === unitId),
      "la recepción y la unidad pendiente de ubicación",
    );

    assert.ok(receiptInput);
    assert.equal(receiptInput!.label, "Recepción de prueba");
    assert.deepEqual(receiptInput!.lines, [{ sku: "SKU-1", quantity: 3, lotCode: "LOTE-7" }]);
    assert.equal(elements.get("#store-destination")!.children.some((item) => item.value === "position-1"), true);

    elements.get("tab:store")!.dispatch("click");
    elements.get("#store-unit")!.value = unitId;
    elements.get("#store-destination")!.value = "position-1";
    elements.get("#store-operation-form")!.dispatch("submit");
    await waitFor(
      () => elements.get("#operation-result-status")!.dataset.kind === "success"
        && elements.get("#move-unit")!.children.some((item) => item.value === unitId),
      "la asignación de posición y la unidad almacenada",
    );

    assert.ok(storeInput);
    assert.equal(storeInput!.handlingUnitId, unitId);
    assert.equal(storeInput!.destinationLocationId, "position-1");
    assert.equal(elements.get("#move-submit")!.disabled, false);

    elements.get("tab:move")!.dispatch("click");
    elements.get("#move-unit")!.value = unitId;
    elements.get("#move-unit")!.dispatch("change");
    elements.get("#move-destination")!.value = "position-2";
    elements.get("#move-operation-form")!.dispatch("submit");
    await waitFor(
      () => elements.get("#operation-result-status")!.dataset.kind === "success"
        && elements.get("#operation-history-details")!.textContent.includes("P-02"),
      "el traslado y el historial actualizado",
    );

    assert.ok(moveInput);
    assert.equal(moveInput!.handlingUnitId, unitId);
    assert.equal(moveInput!.originLocationId, "position-1");
    assert.equal(moveInput!.destinationLocationId, "position-2");
    assert.equal(elements.get("#move-destination")!.children.some((item) => item.value === "position-1"), true);
    assert.equal(elements.get("#move-destination")!.children.some((item) => item.value === "position-2"), false);
    assert.match(elements.get("#operation-history-details")!.textContent, /Traslado/);
    assert.match(elements.get("#operation-history-details")!.textContent, /P-01/);
    assert.match(elements.get("#operation-history-details")!.textContent, /P-02/);
  });
});

test("si la posición se ocupa antes de confirmar, informa el rechazo y refresca destinos sin mutar la unidad", async () => {
  const receivedUnit: HandlingUnitDto = {
    id: "unit-awaiting-location",
    label: "PAL-RECEIVED-1",
    status: "received",
    currentLocationId: null,
    lines: [{ sku: "SKU-1", quantity: 2, lotCode: null }],
  };
  let competingUnit: HandlingUnitDto | null = null;
  let storeInput: Parameters<typeof inventoryIpc.storeUnit>[0] | null = null;
  let historyReads = 0;
  const locations: LocationDto[] = [
    { id: "position-1", code: "P-01", name: "Posición 1", kind: "position", parentId: null, active: true },
    { id: "position-2", code: "P-02", name: "Posición 2", kind: "position", parentId: null, active: true },
    { id: "position-3", code: "P-03", name: "Posición 3", kind: "position", parentId: null, active: true },
  ];

  await withOperationsView({
    listProducts: async () => products,
    listHandlingUnits: async () => competingUnit ? [receivedUnit, competingUnit] : [receivedUnit],
    listLocations: async () => locations,
    storeUnit: async (input) => {
      storeInput = structuredClone(input);
      competingUnit = {
        id: "unit-from-concurrent-receipt",
        label: "PAL-CONCURRENT-1",
        status: "stored",
        currentLocationId: input.destinationLocationId,
        lines: [{ sku: "SKU-1", quantity: 1, lotCode: null }],
      };
      throw new AriadnaIpcError({
        code: "position_occupied",
        message: "La posición fue ocupada antes de confirmar.",
      });
    },
    getMovementHistory: async () => {
      historyReads += 1;
      return [];
    },
  }, async (elements) => {
    elements.get("tab:store")!.dispatch("click");
    elements.get("#store-unit")!.value = receivedUnit.id;
    elements.get("#store-destination")!.value = "position-2";
    elements.get("#store-operation-form")!.dispatch("submit");

    await waitFor(
      () => elements.get("#operation-result-status")!.dataset.kind === "error"
        && !elements.get("#store-submit")!.disabled,
      "el rechazo de posición ocupada y la reactivación del formulario",
    );

    assert.ok(storeInput);
    assert.equal(storeInput!.handlingUnitId, receivedUnit.id);
    assert.equal(storeInput!.destinationLocationId, "position-2");
    assert.match(elements.get("#operation-result-status")!.textContent, /La posición fue ocupada antes de confirmar/);
    assert.doesNotMatch(elements.get("#operation-result-status")!.textContent, /confirmada por Rust/);
    assert.equal(elements.get("#operation-retry")!.hidden, true);
    assert.equal(elements.get("#store-unit")!.children.some((item) => item.value === receivedUnit.id), true);
    assert.equal(elements.get("#store-destination")!.value, "");
    assert.equal(elements.get("#store-destination")!.children.some((item) => item.value === "position-2"), false);
    assert.equal(elements.get("#store-destination")!.children.some((item) => item.value === "position-1"), true);
    assert.equal(historyReads, 0, "un rechazo de dominio no debe presentar un movimiento aceptado");
  });
});

test("el despacho cancelado no escribe y el confirmado espera la respuesta de Rust", async () => {
  const storedUnit: HandlingUnitDto = {
    id: "unit-stored-1",
    label: "PAL-DISPATCH-1",
    status: "stored",
    currentLocationId: "position-1",
    lines: [{ sku: "SKU-1", quantity: 3, lotCode: "LOT-1" }],
  };
  const dispatchedUnit: HandlingUnitDto = {
    ...storedUnit,
    status: "dispatched",
    currentLocationId: null,
  };
  const locations: LocationDto[] = [
    { id: "zone-1", code: "Z-1", name: "Zona 1", kind: "zone", parentId: null, active: true },
    { id: "aisle-1", code: "A-1", name: "Pasillo 1", kind: "aisle", parentId: "zone-1", active: true },
    { id: "rack-1", code: "R-1", name: "Rack 1", kind: "rack", parentId: "aisle-1", active: true },
    { id: "position-1", code: "P-01", name: "Posición 1", kind: "position", parentId: "rack-1", active: true },
  ];
  let shouldConfirm = false;
  let confirmationCount = 0;
  let dispatchRequested = false;
  let dispatchInput: Parameters<typeof inventoryIpc.dispatchUnit>[0] | null = null;
  let resolveDispatch: ((write: MovementWriteDto) => void) | undefined;

  await withOperationsView({
    listProducts: async () => products,
    listHandlingUnits: async () => dispatchRequested ? [dispatchedUnit] : [storedUnit],
    listLocations: async () => locations,
    dispatchUnit: async (input) => {
      dispatchInput = structuredClone(input);
      dispatchRequested = true;
      return await new Promise<MovementWriteDto>((resolve) => {
        resolveDispatch = resolve;
      });
    },
    getMovementHistory: async (unitId) => {
      assert.equal(unitId, storedUnit.id);
      assert.ok(dispatchInput);
      return [{
        recordId: "record-dispatch-1",
        eventId: dispatchInput.eventId,
        handlingUnitId: unitId,
        kind: "dispatched",
        originLocationId: "position-1",
        destinationLocationId: null,
        occurredAtMs: dispatchInput.occurredAtMs,
        recordedAtMs: dispatchInput.occurredAtMs,
        source: "manual",
        result: "applied",
        rejectionReason: null,
        correctsEventId: null,
        correctionReason: null,
      }];
    },
  }, async (elements) => {
    elements.get("tab:dispatch")!.dispatch("click");
    elements.get("#dispatch-unit")!.value = storedUnit.id;
    const dispatchForm = elements.get("#dispatch-operation-form")!;
    dispatchForm.dispatch("submit");

    assert.equal(confirmationCount, 1);
    assert.equal(dispatchRequested, false, "cancelar la confirmación no debe llamar a Rust");
    assert.doesNotMatch(elements.get("#operation-result-status")!.textContent, /Despacho confirmada/);

    shouldConfirm = true;
    dispatchForm.dispatch("submit");
    await waitFor(() => dispatchInput !== null, "el envío del despacho a Rust");
    assert.equal(confirmationCount, 2);
    assert.equal(elements.get("#operation-result-status")!.dataset.kind, "neutral");
    assert.equal(elements.get("#operation-result-status")!.textContent, "Esperando confirmación de Rust…");
    assert.equal(dispatchInput!.handlingUnitId, storedUnit.id);
    assert.equal(dispatchInput!.originLocationId, storedUnit.currentLocationId);
    assert.ok(dispatchInput!.eventId);

    resolveDispatch?.({ eventId: dispatchInput!.eventId, outcome: "applied" });
    await waitFor(
      () => elements.get("#operation-result-status")!.dataset.kind === "success"
        && !elements.get("#dispatch-unit")!.children.some((option) => option.value === storedUnit.id),
      "la confirmación del despacho",
    );
    assert.match(elements.get("#operation-result-status")!.textContent, /Despacho confirmada por Rust/);
    assert.match(elements.get("#operation-history-details")!.textContent, /Despacho/);
    assert.match(elements.get("#operation-history-details")!.textContent, /P-01/);
    assert.match(elements.get("#operation-history-details")!.textContent, /Evento/);
    assert.equal(elements.get("#dispatch-submit")!.disabled, true, "sin unidades almacenadas no debe poder iniciarse otro despacho");
  }, () => {
    confirmationCount += 1;
    return shouldConfirm;
  });
});

test("un despacho rechazado conserva la unidad y no anuncia éxito", async () => {
  const storedUnit: HandlingUnitDto = {
    id: "unit-stored-rejected",
    label: "PAL-DISPATCH-REJECTED",
    status: "stored",
    currentLocationId: "position-1",
    lines: [{ sku: "SKU-1", quantity: 2, lotCode: null }],
  };
  const locations: LocationDto[] = [
    { id: "zone-1", code: "Z-1", name: "Zona 1", kind: "zone", parentId: null, active: true },
    { id: "aisle-1", code: "A-1", name: "Pasillo 1", kind: "aisle", parentId: "zone-1", active: true },
    { id: "rack-1", code: "R-1", name: "Rack 1", kind: "rack", parentId: "aisle-1", active: true },
    { id: "position-1", code: "P-01", name: "Posición 1", kind: "position", parentId: "rack-1", active: true },
  ];
  let dispatchInput: Parameters<typeof inventoryIpc.dispatchUnit>[0] | null = null;
  let historyReads = 0;

  await withOperationsView({
    listProducts: async () => products,
    listHandlingUnits: async () => [storedUnit],
    listLocations: async () => locations,
    dispatchUnit: async (input) => {
      dispatchInput = structuredClone(input);
      throw new AriadnaIpcError({
        code: "invalid_transition",
        message: "La unidad ya no está almacenada.",
      });
    },
    getMovementHistory: async () => {
      historyReads += 1;
      return [];
    },
  }, async (elements) => {
    elements.get("tab:dispatch")!.dispatch("click");
    elements.get("#dispatch-unit")!.value = storedUnit.id;
    assert.equal(elements.get("#dispatch-operation-fields")!.disabled, false);
    assert.equal(elements.get("#dispatch-unit")!.value, storedUnit.id);
    elements.get("#dispatch-operation-form")!.dispatch("submit");

    await waitFor(() => dispatchInput !== null, "la solicitud de despacho a Rust");
    await waitFor(
      () => elements.get("#operation-result-status")!.dataset.kind === "error"
        && !elements.get("#dispatch-submit")!.disabled,
      "el rechazo de Rust y la reactivación del formulario",
    );

    assert.ok(dispatchInput);
    assert.equal(dispatchInput!.handlingUnitId, storedUnit.id);
    assert.equal(dispatchInput!.originLocationId, storedUnit.currentLocationId);
    assert.match(elements.get("#operation-result-status")!.textContent, /La unidad ya no está almacenada/);
    assert.doesNotMatch(elements.get("#operation-result-status")!.textContent, /Despacho confirmado/);
    assert.equal(elements.get("#operation-retry")!.hidden, true, "un rechazo de dominio no debe ofrecer reintento incierto");
    assert.equal(elements.get("#dispatch-operation-fields")!.disabled, false);
    assert.equal(elements.get("#dispatch-unit")!.children.some((option) => option.value === storedUnit.id), true);
    assert.equal(focusedElement, elements.get("#dispatch-submit"), "el foco debe volver al botón tras el rechazo");
    assert.equal(historyReads, 0, "un despacho rechazado no debe mostrarse como movimiento confirmado");
  });
});
