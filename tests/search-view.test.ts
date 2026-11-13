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

  removeAttribute(name: string): void {
    this.attributes.delete(name);
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

function findAllByClass(element: FakeElement, className: string): FakeElement[] {
  const found = element.className === className ? [element] : [];
  for (const child of element.children) found.push(...findAllByClass(child, className));
  return found;
}

type SearchElements = Record<string, FakeElement>;

async function withSearchDom(run: (elements: SearchElements) => Promise<void>): Promise<void> {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const originalSearchInventory = inventoryIpc.searchInventory;
  const originalGetProduct = inventoryIpc.getProduct;
  const originalGetLocation = inventoryIpc.getLocation;
  const elements: SearchElements = {
    "#inventory-search-input": new FakeElement("input"),
    "#inventory-search-clear": new FakeElement("button"),
    "#inventory-search-status": new FakeElement("p"),
    "#inventory-search-results": new FakeElement("div"),
    "#inventory-search-detail": new FakeElement("aside"),
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
  inventoryIpc.searchInventory = async () => [];
  inventoryIpc.getProduct = async () => null;
  inventoryIpc.getLocation = async () => null;

  try {
    await run(elements);
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

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
} {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
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
    assert.equal(resultButton.attributes.has("aria-label"), false, "el nombre accesible debe incluir el contenido visible del botón");
    assert.ok(resultButton.textContent.includes(result.title));
    assert.match(resultButton.textContent, /Registro sintético/);
    assert.match(resultButton.textContent, /Activo/);
    assert.equal(elements["#inventory-search-results"].attributes.has("aria-busy"), false);
    resultButton.dispatch("click");
    await new Promise((resolve) => setTimeout(resolve, 0));

    const expectedTitle = kind === "product" ? "Producto no disponible" : "Ubicación no disponible";
    const expectedStatus = kind === "product" ? /el producto ya no está disponible/i : /la ubicación ya no está disponible/i;
    assert.match(elements["#inventory-search-detail"].textContent, new RegExp(expectedTitle));
    assert.equal(elements["#inventory-search-status"].dataset.kind, "error");
    assert.match(elements["#inventory-search-status"].textContent, expectedStatus);
    assert.match(elements["#inventory-search-status"].textContent, /actualiza la búsqueda/i);
    assert.equal(elements["#inventory-search-detail"].attributes.has("aria-busy"), false);
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

test("anuncia la carga durante la espera de búsqueda y la limpia al vaciar el término", async () => {
  await withSearchDom(async (elements) => {
    const input = elements["#inventory-search-input"];
    const results = elements["#inventory-search-results"];
    const detail = elements["#inventory-search-detail"];
    const view = mountSearchView();

    input.value = "SKU-DEMO";
    input.dispatch("input");
    assert.equal(results.attributes.get("aria-busy"), "true");
    assert.equal(detail.attributes.get("aria-busy"), "true");
    assert.match(results.textContent, /Actualizando resultados/);

    input.value = "";
    await view.refresh();
    assert.equal(results.attributes.has("aria-busy"), false);
    assert.equal(detail.attributes.has("aria-busy"), false);
  });
});

test("mantiene aria-busy durante la búsqueda vigente y descarta resultados obsoletos", async () => {
  await withSearchDom(async (elements) => {
    const pending: Array<ReturnType<typeof deferred<InventorySearchResultDto[]>>> = [];
    inventoryIpc.searchInventory = async () => {
      const request = deferred<InventorySearchResultDto[]>();
      pending.push(request);
      return request.promise;
    };
    const input = elements["#inventory-search-input"];
    const results = elements["#inventory-search-results"];
    const detail = elements["#inventory-search-detail"];
    const view = mountSearchView();

    input.value = "ANTERIOR";
    const previousRequest = view.refresh();
    assert.equal(results.attributes.get("aria-busy"), "true");
    assert.equal(detail.attributes.get("aria-busy"), "true");

    input.value = "VIGENTE";
    const currentRequest = view.refresh();
    pending[0].resolve([{
      kind: "product", id: "old", title: "ANTERIOR", subtitle: "Obsoleto", status: "active",
    }]);
    await previousRequest;
    assert.equal(results.attributes.get("aria-busy"), "true");
    assert.equal(detail.attributes.get("aria-busy"), "true");
    assert.doesNotMatch(results.textContent, /ANTERIOR/);

    pending[1].resolve([{
      kind: "product", id: "current", title: "VIGENTE", subtitle: "Resultado actual", status: "active",
    }]);
    await currentRequest;
    assert.equal(results.attributes.has("aria-busy"), false);
    assert.equal(detail.attributes.has("aria-busy"), false);
    assert.match(results.textContent, /VIGENTE/);
  });
});

test("limpia aria-busy cuando falla la búsqueda", async () => {
  await withSearchDom(async (elements) => {
    inventoryIpc.searchInventory = async () => {
      throw new Error("No se pudo consultar SQLite.");
    };
    elements["#inventory-search-input"].value = "SKU-ERROR";
    const view = mountSearchView();

    await view.refresh();

    assert.equal(elements["#inventory-search-results"].attributes.has("aria-busy"), false);
    assert.equal(elements["#inventory-search-detail"].attributes.has("aria-busy"), false);
    assert.equal(elements["#inventory-search-status"].dataset.kind, "error");
  });
});

test("no libera aria-busy por un detalle obsoleto", async () => {
  await withSearchDom(async (elements) => {
    const firstDetail = deferred<Awaited<ReturnType<typeof inventoryIpc.getProduct>>>();
    const secondDetail = deferred<Awaited<ReturnType<typeof inventoryIpc.getProduct>>>();
    const requests = [firstDetail, secondDetail];
    inventoryIpc.searchInventory = async () => [
      { kind: "product", id: "one", title: "SKU-1", subtitle: "Primero", status: "active" },
      { kind: "product", id: "two", title: "SKU-2", subtitle: "Segundo", status: "active" },
    ];
    inventoryIpc.getProduct = async () => {
      const request = requests.shift();
      if (!request) throw new Error("No hay respuesta de detalle preparada.");
      return request.promise;
    };
    elements["#inventory-search-input"].value = "SKU";
    const view = mountSearchView();
    await view.refresh();
    const buttons = findAllByClass(elements["#inventory-search-results"], "search-result-button");
    assert.equal(buttons.length, 2);

    buttons[0].dispatch("click");
    buttons[1].dispatch("click");
    const detail = elements["#inventory-search-detail"];
    assert.equal(detail.attributes.get("aria-busy"), "true");

    firstDetail.resolve({ sku: "SKU-1", description: "Detalle obsoleto", unitOfMeasure: "unidad", active: true });
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(detail.attributes.get("aria-busy"), "true");
    assert.doesNotMatch(detail.textContent, /Detalle obsoleto/);

    secondDetail.resolve({ sku: "SKU-2", description: "Detalle vigente", unitOfMeasure: "unidad", active: true });
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(detail.attributes.has("aria-busy"), false);
    assert.match(detail.textContent, /Detalle vigente/);
  });
});
