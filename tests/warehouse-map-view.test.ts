import assert from "node:assert/strict";
import test from "node:test";

import { inventoryIpc, type HandlingUnitDto, type LocationDto } from "../src/lib/ipc.ts";

class FakeElement {
  readonly tagName: string;
  readonly children: FakeElement[] = [];
  readonly dataset: Record<string, string> = {};
  readonly attributes = new Map<string, string>();
  readonly classList = {
    add: (name: string) => {
      const classes = new Set(this.className.split(/\s+/).filter(Boolean));
      classes.add(name);
      this.className = [...classes].join(" ");
    },
  };
  className = "";
  disabled = false;
  open = false;
  tabIndex = 0;
  private ownText = "";
  private parent: FakeElement | null = null;
  private readonly listeners = new Map<string, Array<() => void>>();

  constructor(tagName: string) {
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

  addEventListener(name: string, listener: () => void): void {
    const listeners = this.listeners.get(name) ?? [];
    listeners.push(listener);
    this.listeners.set(name, listeners);
  }

  dispatch(name: string): void {
    for (const listener of this.listeners.get(name) ?? []) listener();
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  focus(): void {
    const document = globalThis.document as unknown as { activeElement: FakeElement | null };
    document.activeElement = this;
  }

  contains(candidate: FakeElement): boolean {
    return this === candidate || this.children.some((child) => child.contains(candidate));
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
  if (selector === "[data-position-id]") return "positionId" in element.dataset;
  if (selector === "[data-map-focus-id]") return "mapFocusId" in element.dataset;
  return false;
}

function location(id: string, kind: LocationDto["kind"], parentId: string | null): LocationDto {
  return { id, code: id.toUpperCase(), name: "Nombre " + id, kind, parentId, active: true };
}

async function waitFor(predicate: () => boolean, label: string): Promise<void> {
  const deadline = Date.now() + 1_000;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  assert.fail("Tiempo agotado esperando " + label);
}

test("el refresco restaura el foco y la selección consulta el pallet asociado", async () => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const previousHTMLElement = Object.getOwnPropertyDescriptor(globalThis, "HTMLElement");
  const originalListLocations = inventoryIpc.listLocations;
  const originalListUnits = inventoryIpc.listHandlingUnits;
  const originalGetHandlingUnit = inventoryIpc.getHandlingUnit;
  const storedUnit: HandlingUnitDto = {
    id: "unit-1",
    label: "PALLET-001",
    status: "stored",
    currentLocationId: "position-1",
    lines: [{ sku: "SKU-TEST", quantity: 3, lotCode: "LOT-42" }],
  };
  const secondStoredUnit: HandlingUnitDto = {
    id: "unit-2",
    label: "PALLET-002",
    status: "stored",
    currentLocationId: "position-2",
    lines: [{ sku: "SKU-SECOND", quantity: 7, lotCode: "LOT-84" }],
  };
  const locations = [
    location("zone-1", "zone", null),
    location("aisle-1", "aisle", "zone-1"),
    location("rack-1", "rack", "aisle-1"),
    location("position-1", "position", "rack-1"),
    location("position-2", "position", "rack-1"),
  ];
  const elements: Record<string, FakeElement> = {
    "#warehouse-map": new FakeElement("div"),
    "#map-panel": new FakeElement("section"),
    "#map-status": new FakeElement("p"),
    "#warehouse-summary": new FakeElement("div"),
    "#position-details": new FakeElement("aside"),
    "#refresh-map": new FakeElement("button"),
    "#selection-announcement": new FakeElement("p"),
  };
  const fakeDocument = {
    documentElement: new FakeElement("html"),
    activeElement: null as FakeElement | null,
    querySelector<T extends Element>(selector: string): T | null {
      return (elements[selector] as unknown as T | undefined) ?? null;
    },
    createElement<T extends HTMLElement>(tagName: string): T {
      return new FakeElement(tagName) as unknown as T;
    },
    createTextNode(text: string): FakeElement {
      const node = new FakeElement("#text");
      node.textContent = text;
      return node;
    },
  };
  const windowListeners = new Map<string, Array<(event: { type: string }) => void>>();
  const fakeWindow = {
    addEventListener(name: string, listener: (event: { type: string }) => void): void {
      const listeners = windowListeners.get(name) ?? [];
      listeners.push(listener);
      windowListeners.set(name, listeners);
    },
    dispatchEvent(event: { type: string }): boolean {
      for (const listener of windowListeners.get(event.type) ?? []) listener(event);
      return true;
    },
  };

  Object.defineProperty(globalThis, "window", { configurable: true, value: fakeWindow });
  Object.defineProperty(globalThis, "document", { configurable: true, value: fakeDocument });
  Object.defineProperty(globalThis, "HTMLElement", { configurable: true, value: FakeElement });
  inventoryIpc.listLocations = async () => locations;
  inventoryIpc.listHandlingUnits = async (): Promise<HandlingUnitDto[]> => [storedUnit, secondStoredUnit];

  try {
    await import("../src/views/warehouse-map-view.ts?ard11-focus-test");
    const map = elements["#warehouse-map"];
    await waitFor(() => map.querySelectorAll("[data-position-id]").length === 2, "el mapa inicial");
    const originalPosition = map
      .querySelectorAll<FakeElement>("[data-position-id]")
      .find((position) => position.dataset.positionId === "position-1");
    assert.ok(originalPosition);
    originalPosition.focus();

    fakeWindow.dispatchEvent({ type: "ariadna:inventory-changed" });

    await waitFor(() => {
      const refreshedPosition = map
        .querySelectorAll<FakeElement>("[data-position-id]")
        .find((position) => position.dataset.positionId === "position-1");
      return refreshedPosition !== originalPosition && fakeDocument.activeElement === refreshedPosition;
    }, "la restauración del foco tras el refresco");
    assert.equal(fakeDocument.activeElement?.dataset.mapFocusId, "position-1");
    assert.equal(elements["#refresh-map"].disabled, false);
    assert.equal(elements["#warehouse-map"].attributes.has("aria-busy"), false);

    const detailResolvers = new Map<string, (unit: HandlingUnitDto | null) => void>();
    const requestedUnitIds: string[] = [];
    let failNextDetailRequest = false;
    inventoryIpc.getHandlingUnit = async (id) => {
      requestedUnitIds.push(id);
      if (failNextDetailRequest) {
        failNextDetailRequest = false;
        throw new Error("IPC no disponible");
      }
      return await new Promise((resolve) => {
        detailResolvers.set(id, resolve);
      });
    };
    const refreshedPositions = map.querySelectorAll<FakeElement>("[data-position-id]");
    const refreshedPosition = refreshedPositions.find(
      (position) => position.dataset.positionId === "position-1",
    );
    const secondPosition = refreshedPositions.find(
      (position) => position.dataset.positionId === "position-2",
    );
    assert.ok(refreshedPosition);
    assert.ok(secondPosition);
    assert.ok(refreshedPosition.textContent.includes("Ocupada"));
    assert.equal(
      refreshedPosition.attributes.get("aria-label"),
      "POSITION-1, Nombre position-1, Ocupada. Consultar detalle de la posición.",
    );
    refreshedPosition.dispatch("click");
    assert.equal(refreshedPosition.attributes.get("aria-pressed"), "true");
    assert.equal(elements["#position-details"].attributes.get("aria-busy"), "true");
    assert.ok(elements["#position-details"].textContent.includes("Consultando el pallet"));
    secondPosition.dispatch("click");
    assert.equal(refreshedPosition.attributes.get("aria-pressed"), "false");
    assert.equal(secondPosition.attributes.get("aria-pressed"), "true");
    assert.deepEqual(requestedUnitIds, [storedUnit.id, secondStoredUnit.id]);
    detailResolvers.get(storedUnit.id)?.(storedUnit);
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(elements["#position-details"].textContent.includes("PALLET-001"), false);
    assert.equal(elements["#position-details"].attributes.get("aria-busy"), "true");
    detailResolvers.get(secondStoredUnit.id)?.(secondStoredUnit);

    await waitFor(
      () => elements["#position-details"].textContent.includes("SKU-SECOND"),
      "el detalle de la segunda posición",
    );
    const detailsText = elements["#position-details"].textContent;
    assert.ok(detailsText.includes("PALLET-002"));
    assert.ok(detailsText.includes("unit-2"));
    assert.ok(detailsText.includes("Almacenado"));
    assert.ok(detailsText.includes("SKU-SECOND"));
    assert.ok(detailsText.includes("LOT-84"));
    assert.ok(detailsText.includes("× 7"));
    assert.equal(detailsText.includes("PALLET-001"), false);
    assert.equal(elements["#position-details"].attributes.has("aria-busy"), false);

    failNextDetailRequest = true;
    secondPosition.dispatch("click");
    await waitFor(
      () => elements["#position-details"].textContent.includes("IPC no disponible"),
      "el error de consulta del pallet",
    );
    assert.equal(elements["#position-details"].attributes.has("aria-busy"), false);
    assert.equal(
      elements["#selection-announcement"].textContent,
      "No se pudo consultar el pallet de la posición POSITION-2.",
    );
  } finally {
    inventoryIpc.listLocations = originalListLocations;
    inventoryIpc.listHandlingUnits = originalListUnits;
    inventoryIpc.getHandlingUnit = originalGetHandlingUnit;
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else delete (globalThis as typeof globalThis & { window?: unknown }).window;
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else delete (globalThis as typeof globalThis & { document?: unknown }).document;
    if (previousHTMLElement) Object.defineProperty(globalThis, "HTMLElement", previousHTMLElement);
    else delete (globalThis as typeof globalThis & { HTMLElement?: unknown }).HTMLElement;
  }
});
