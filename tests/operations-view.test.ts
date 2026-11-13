import assert from "node:assert/strict";
import test from "node:test";

import { AriadnaIpcError, inventoryIpc } from "../src/lib/ipc.ts";
import { mountOperationsView } from "../src/views/operations-view.ts";

type Listener = (event: { preventDefault(): void }) => void;

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

  focus(): void {}

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
): Promise<void> {
  const elements = new Map(operationSelectors().map((selector) => [selector, new FakeElement()]));
  const tabs = ["receive", "store", "move", "dispatch"].map((mode) => {
    const tab = new FakeElement("button");
    tab.dataset.operationTab = mode;
    return tab;
  });
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
    value: { confirm: () => true },
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

async function flushPromises(): Promise<void> {
  await new Promise<void>((resolve) => setImmediate(resolve));
  await Promise.resolve();
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
    await flushPromises();

    const retry = elements.get("#operation-retry")!;
    assert.equal(retry.hidden, false, "un fallo de almacenamiento debe ofrecer un reintento explícito");
    assert.equal(elements.get("#receive-operation-fields")!.disabled, true);
    assert.match(elements.get("#operation-availability")!.textContent, /datos e identificador se conservan/);

    retry.dispatch("click");
    await flushPromises();
    await flushPromises();

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
    await flushPromises();
    await flushPromises();

    assert.match(elements.get("#operation-result-status")!.textContent, /Recepción confirmada por Rust/);
    assert.match(elements.get("#operation-result-status")!.textContent, /no se pudieron actualizar los selectores/i);
    assert.equal(elements.get("#receive-operation-fields")!.disabled, true, "los formularios dependientes deben bloquearse con datos obsoletos");
    assert.equal(elements.get("#operation-retry")!.hidden, true, "una escritura confirmada no debe ofrecerse para reintento incierto");
  });
});
