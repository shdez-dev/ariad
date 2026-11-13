import assert from "node:assert/strict";
import test from "node:test";

import { inventoryIpc, type InventorySearchResultDto } from "../src/lib/ipc.ts";
import { mountSearchView } from "../src/views/search-view.ts";

class FakeElement {
  readonly tagName: string;
  readonly children: FakeElement[] = [];
  readonly dataset: Record<string, string> = {};
  readonly attributes = new Map<string, string>();
  value = "";
  className = "";
  type = "";
  private ownText = "";
  private readonly listeners = new Map<string, Array<() => void>>();

  constructor(tagName: string) {
    this.tagName = tagName;
  }

  get textContent(): string {
    return this.ownText + this.children.map((child) => child.textContent).join("");
  }

  set textContent(value: string) {
    this.ownText = value;
    this.children.splice(0);
  }

  append(...children: FakeElement[]): void {
    this.children.push(...children);
  }

  replaceChildren(...children: FakeElement[]): void {
    this.ownText = "";
    this.children.splice(0, this.children.length, ...children);
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

  focus(): void {}
}

function findByClass(element: FakeElement, className: string): FakeElement | undefined {
  if (element.className === className) return element;
  for (const child of element.children) {
    const found = findByClass(child, className);
    if (found) return found;
  }
  return undefined;
}

async function assertMissingSearchDetail(kind: "product" | "location"): Promise<void> {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const originalSearchInventory = inventoryIpc.searchInventory;
  const originalGetProduct = inventoryIpc.getProduct;
  const originalGetLocation = inventoryIpc.getLocation;
  const elements: Record<string, FakeElement> = {
    "#inventory-search-input": new FakeElement("input"),
    "#inventory-search-clear": new FakeElement("button"),
    "#inventory-search-status": new FakeElement("p"),
    "#inventory-search-results": new FakeElement("div"),
    "#inventory-search-detail": new FakeElement("aside"),
  };
  elements["#inventory-search-input"].value = "DEMO-404";
  const result: InventorySearchResultDto = {
    kind,
    id: kind === "product" ? "SKU-404" : "location-404",
    title: kind === "product" ? "DEMO-404" : "Z-404",
    subtitle: "Registro sintético",
    status: "active",
  };
  const fakeDocument = {
    querySelector<T extends Element>(selector: string): T | null {
      return (elements[selector] as unknown as T | undefined) ?? null;
    },
    createElement<T extends HTMLElement>(tagName: string): T {
      return new FakeElement(tagName) as unknown as T;
    },
  };

  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { clearTimeout: () => 0, setTimeout: () => 1, dispatchEvent: () => true },
  });
  Object.defineProperty(globalThis, "document", { configurable: true, value: fakeDocument });
  inventoryIpc.searchInventory = async (query) => {
    assert.equal(query, "DEMO-404");
    return [result];
  };
  inventoryIpc.getProduct = async () => null;
  inventoryIpc.getLocation = async () => null;

  try {
    const view = mountSearchView();
    await view.refresh();
    assert.match(elements["#inventory-search-status"].textContent, /1 resultado para/);

    const resultButton = findByClass(elements["#inventory-search-results"], "search-result-button");
    assert.ok(resultButton, "se debe poder abrir el resultado de búsqueda");
    resultButton.dispatch("click");
    await new Promise((resolve) => setTimeout(resolve, 0));

    const expectedTitle = kind === "product" ? "Producto no disponible" : "Ubicación no disponible";
    const expectedStatus = kind === "product" ? /el producto ya no está disponible/i : /la ubicación ya no está disponible/i;
    assert.match(elements["#inventory-search-detail"].textContent, new RegExp(expectedTitle));
    assert.equal(elements["#inventory-search-status"].dataset.kind, "error");
    assert.match(elements["#inventory-search-status"].textContent, expectedStatus);
    assert.match(elements["#inventory-search-status"].textContent, /actualiza la búsqueda/i);
  } finally {
    inventoryIpc.searchInventory = originalSearchInventory;
    inventoryIpc.getProduct = originalGetProduct;
    inventoryIpc.getLocation = originalGetLocation;
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else delete (globalThis as typeof globalThis & { window?: unknown }).window;
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else delete (globalThis as typeof globalThis & { document?: unknown }).document;
  }
}

test("anuncia por estado accesible cuando un producto buscado ya no existe", async () => {
  await assertMissingSearchDetail("product");
});

test("anuncia por estado accesible cuando una ubicación buscada ya no existe", async () => {
  await assertMissingSearchDetail("location");
});
