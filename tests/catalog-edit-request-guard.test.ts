import assert from "node:assert/strict";
import test from "node:test";

import { inventoryIpc } from "../src/lib/ipc.ts";
import { mountLocationsView } from "../src/views/locations-view.ts";
import { mountProductsView } from "../src/views/products-view.ts";

class FakeElement {
  tagName: string;
  children: FakeElement[] = [];
  dataset: Record<string, string> = {};
  value = "";
  checked = false;
  hidden = false;
  disabled = false;
  required = false;
  colSpan = 0;
  className = "";
  type = "";
  private ownText = "";
  private listeners = new Map<string, Array<(event: { preventDefault(): void }) => void>>();

  constructor(tagName = "div") {
    this.tagName = tagName;
  }

  get textContent(): string {
    return this.ownText + this.children.map((child) => child.textContent).join("");
  }

  set textContent(value: string) {
    this.ownText = value;
    this.children = [];
  }

  append(...children: FakeElement[]): void {
    this.children.push(...children);
  }

  replaceChildren(...children: FakeElement[]): void {
    this.ownText = "";
    this.children = [...children];
  }

  addEventListener(name: string, listener: (event: { preventDefault(): void }) => void): void {
    const listeners = this.listeners.get(name) ?? [];
    listeners.push(listener);
    this.listeners.set(name, listeners);
  }

  dispatch(name: string): void {
    for (const listener of this.listeners.get(name) ?? []) {
      listener({ preventDefault() {} });
    }
  }

  focus(): void {}

  reset(): void {}

  reportValidity(): boolean {
    return true;
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function selectorsFor(kind: "products" | "locations"): string[] {
  return kind === "products"
    ? [
        "#products-table-body", "#products-status", "#products-filter", "#products-include-inactive",
        "#product-form", "#product-form-title", "#product-form-message", "#product-form-submit",
        "#product-sku", "#product-description", "#product-unit", "#product-new", "#product-cancel",
        "#products-refresh",
      ]
    : [
        "#locations-table-body", "#locations-status", "#locations-filter", "#locations-include-inactive",
        "#location-form", "#location-form-title", "#location-form-message", "#location-form-submit",
        "#location-code", "#location-name", "#location-kind", "#location-parent", "#location-parent-field",
        "#location-parent-hint", "#location-new", "#location-cancel", "#locations-refresh",
      ];
}

async function withMountedView(
  kind: "products" | "locations",
  overrides: Record<string, unknown>,
  run: (elements: Map<string, FakeElement>, view: { refresh: () => Promise<unknown> }) => Promise<void>,
): Promise<void> {
  const elements = new Map(selectorsFor(kind).map((selector) => [selector, new FakeElement()]));
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousIpc = new Map(
    Object.keys(overrides).map((key) => [key, Object.getOwnPropertyDescriptor(inventoryIpc, key)]),
  );
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: {
      querySelector: (selector: string) => elements.get(selector) ?? null,
      createElement: (tagName: string) => new FakeElement(tagName),
    },
  });
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { confirm: () => true },
  });
  Object.assign(inventoryIpc, overrides);

  try {
    const view = kind === "products" ? mountProductsView() : mountLocationsView();
    await run(elements, view);
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

const products = [
  { sku: "SKU-A", description: "Producto A", unitOfMeasure: "unidad", active: true },
  { sku: "SKU-B", description: "Producto B", unitOfMeasure: "caja", active: true },
];

const locations = [
  { id: "location-A", code: "ZONE-A", name: "Zona A", kind: "zone" as const, parentId: null, active: true },
  { id: "location-B", code: "ZONE-B", name: "Zona B", kind: "zone" as const, parentId: null, active: true },
];

function renderedEditButtons(elements: Map<string, FakeElement>): FakeElement[] {
  const rows = elements.get("#products-table-body") ?? elements.get("#locations-table-body");
  assert.ok(rows);
  return rows.children.map((row) => row.children[4].children[0]);
}

function renderedDeactivationButtons(elements: Map<string, FakeElement>): FakeElement[] {
  const rows = elements.get("#products-table-body") ?? elements.get("#locations-table-body");
  assert.ok(rows);
  return rows.children.map((row) => row.children[4].children[1]);
}

async function flushPromises(): Promise<void> {
  await new Promise<void>((resolve) => setImmediate(resolve));
  await Promise.resolve();
}

for (const kind of ["products", "locations"] as const) {
  const isProducts = kind === "products";
  const records = isProducts ? products : locations;
  const listMethod = isProducts ? "listProducts" : "listLocations";
  const getMethod = isProducts ? "getProduct" : "getLocation";
  const keyFor = (record: (typeof records)[number]) => isProducts ? record.sku : record.id;
  const titleFor = (record: (typeof records)[number]) => isProducts ? record.sku : record.code;
  const formTitleSelector = isProducts ? "#product-form-title" : "#location-form-title";
  const formSelector = isProducts ? "#product-form" : "#location-form";
  const formSubmitSelector = isProducts ? "#product-form-submit" : "#location-form-submit";
  const formMessageSelector = isProducts ? "#product-form-message" : "#location-form-message";
  const rowsSelector = isProducts ? "#products-table-body" : "#locations-table-body";
  const newButtonSelector = isProducts ? "#product-new" : "#location-new";
  const cancelButtonSelector = isProducts ? "#product-cancel" : "#location-cancel";
  const statusSelector = isProducts ? "#products-status" : "#locations-status";
  const refreshButtonSelector = isProducts ? "#products-refresh" : "#locations-refresh";

  test(kind + ": una respuesta de edición antigua no reemplaza la selección más reciente", async () => {
    const requests = new Map(records.map((record) => [keyFor(record), deferred<typeof record>()]));
    await withMountedView(kind, {
      [listMethod]: async () => records,
      [getMethod]: (key: string) => requests.get(key)!.promise,
    }, async (elements, view) => {
      await view.refresh();
      const [editA, editB] = renderedEditButtons(elements);
      editA.dispatch("click");
      editB.dispatch("click");

      requests.get(keyFor(records[1]))!.resolve(records[1]);
      await flushPromises();
      assert.equal(elements.get(formTitleSelector)!.textContent, "Editar " + titleFor(records[1]));

      requests.get(keyFor(records[0]))!.resolve(records[0]);
      await flushPromises();
      assert.equal(elements.get(formTitleSelector)!.textContent, "Editar " + titleFor(records[1]));
      assert.equal(elements.get(statusSelector)!.textContent, "Consultando " + titleFor(records[1]) + "…");
    });
  });

  test(kind + ": Nuevo y Cancelar invalidan la consulta pendiente", async () => {
    for (const action of ["new", "cancel"] as const) {
      const pending = deferred<(typeof records)[number]>();
      await withMountedView(kind, {
        [listMethod]: async () => records,
        [getMethod]: () => pending.promise,
      }, async (elements, view) => {
        await view.refresh();
        renderedEditButtons(elements)[0].dispatch("click");

        if (action === "new") elements.get(newButtonSelector)!.dispatch("click");
        else elements.get(cancelButtonSelector)!.dispatch("click");

        pending.reject(new Error("respuesta de edición obsoleta"));
        await flushPromises();
        if (action === "new") {
          assert.equal(elements.get(formSelector)!.hidden, false);
          assert.equal(elements.get(formTitleSelector)!.textContent, isProducts ? "Nuevo producto" : "Nueva ubicación");
        } else {
          assert.equal(elements.get(formSelector)!.hidden, true);
          assert.equal(elements.get(statusSelector)!.textContent, isProducts
            ? "Formulario de producto cerrado."
            : "Formulario de ubicación cerrado.");
        }
        assert.doesNotMatch(elements.get(statusSelector)!.textContent, /respuesta de edición obsoleta/);
      });
    }
  });

  test(kind + ": un guardado tardío no cierra un formulario Nuevo posterior", async () => {
    const pendingSave = deferred<(typeof records)[number]>();
    await withMountedView(kind, {
      [listMethod]: async () => records,
      [getMethod]: async (key: string) => records.find((record) => keyFor(record) === key) ?? null,
      [isProducts ? "createProduct" : "createLocation"]: () => pendingSave.promise,
    }, async (elements, view) => {
      await view.refresh();
      elements.get(newButtonSelector)!.dispatch("click");
      elements.get(formSelector)!.dispatch("submit");
      elements.get(newButtonSelector)!.dispatch("click");

      pendingSave.resolve(records[0]);
      await flushPromises();
      await flushPromises();
      assert.equal(elements.get(formSelector)!.hidden, false);
      assert.equal(elements.get(formTitleSelector)!.textContent, isProducts ? "Nuevo producto" : "Nueva ubicación");
      assert.equal(elements.get(statusSelector)!.textContent, isProducts
        ? "Formulario de nuevo producto listo."
        : "Formulario de nueva ubicación listo.");
    });
  });

  test(kind + ": una recarga manual no invalida el resultado del guardado vigente", async () => {
    const pendingSave = deferred<(typeof records)[number]>();
    await withMountedView(kind, {
      [listMethod]: async () => records,
      [getMethod]: async (key: string) => records.find((record) => keyFor(record) === key) ?? null,
      [isProducts ? "createProduct" : "createLocation"]: () => pendingSave.promise,
    }, async (elements, view) => {
      await view.refresh();
      elements.get(newButtonSelector)!.dispatch("click");
      elements.get(formSelector)!.dispatch("submit");
      elements.get(refreshButtonSelector)!.dispatch("click");
      await flushPromises();

      pendingSave.resolve(records[0]);
      await flushPromises();
      await flushPromises();
      assert.equal(elements.get(formSelector)!.hidden, true);
      assert.equal(elements.get(statusSelector)!.textContent, isProducts
        ? "Producto SKU-A guardado."
        : "Ubicación ZONE-A guardada.");
    });
  });

  test(kind + ": una recarga posterior no deja un guardado confirmado en estado pendiente", async () => {
    const pendingSave = deferred<(typeof records)[number]>();
    const pendingWriteRefresh = deferred<(typeof records)[number][]>();
    let listCalls = 0;
    await withMountedView(kind, {
      [listMethod]: () => {
        listCalls += 1;
        if (listCalls === 2) return pendingWriteRefresh.promise;
        return Promise.resolve(records);
      },
      [getMethod]: async (key: string) => records.find((record) => keyFor(record) === key) ?? null,
      [isProducts ? "createProduct" : "createLocation"]: () => pendingSave.promise,
    }, async (elements, view) => {
      await view.refresh();
      elements.get(newButtonSelector)!.dispatch("click");
      elements.get(formSelector)!.dispatch("submit");

      pendingSave.resolve(records[0]);
      await flushPromises();
      assert.equal(listCalls, 2);
      elements.get(refreshButtonSelector)!.dispatch("click");
      await flushPromises();
      assert.equal(listCalls, 3);

      pendingWriteRefresh.resolve(records);
      await flushPromises();
      await flushPromises();
      assert.equal(elements.get(formSelector)!.hidden, true);
      assert.equal(elements.get(statusSelector)!.textContent, isProducts
        ? "Producto SKU-A guardado."
        : "Ubicación ZONE-A guardada.");
    });
  });

  test(kind + ": al cargar otra edición no se puede enviar el formulario anterior", async () => {
    const pendingB = deferred<(typeof records)[number]>();
    let writes = 0;
    await withMountedView(kind, {
      [listMethod]: async () => records,
      [getMethod]: (key: string) => key === keyFor(records[0])
        ? Promise.resolve(records[0])
        : pendingB.promise,
      [isProducts ? "updateProduct" : "updateLocation"]: () => {
        writes += 1;
        return Promise.resolve(records[0]);
      },
    }, async (elements, view) => {
      await view.refresh();
      const [editA, editB] = renderedEditButtons(elements);
      editA.dispatch("click");
      await flushPromises();
      assert.equal(elements.get(formTitleSelector)!.textContent, "Editar " + titleFor(records[0]));

      editB.dispatch("click");
      assert.equal(elements.get(formSelector)!.hidden, true);
      assert.equal(elements.get(formSubmitSelector)!.disabled, true);
      elements.get(formSelector)!.dispatch("submit");
      assert.equal(writes, 0);

      pendingB.resolve(records[1]);
      await flushPromises();
      assert.equal(elements.get(formTitleSelector)!.textContent, "Editar " + titleFor(records[1]));
      assert.equal(elements.get(formSubmitSelector)!.disabled, false);
    });
  });

  test(kind + ": un guardado anterior no habilita el botón de un guardado posterior", async () => {
    const firstSave = deferred<(typeof records)[number]>();
    const secondSave = deferred<(typeof records)[number]>();
    let saveNumber = 0;
    await withMountedView(kind, {
      [listMethod]: async () => records,
      [getMethod]: async (key: string) => records.find((record) => keyFor(record) === key) ?? null,
      [isProducts ? "createProduct" : "createLocation"]: () => {
        saveNumber += 1;
        return saveNumber === 1 ? firstSave.promise : secondSave.promise;
      },
    }, async (elements, view) => {
      await view.refresh();
      elements.get(newButtonSelector)!.dispatch("click");
      elements.get(formSelector)!.dispatch("submit");
      assert.equal(elements.get(formSubmitSelector)!.disabled, true);

      elements.get(newButtonSelector)!.dispatch("click");
      assert.equal(elements.get(formSubmitSelector)!.disabled, false);
      elements.get(formSelector)!.dispatch("submit");
      assert.equal(saveNumber, 2);
      assert.equal(elements.get(formSubmitSelector)!.disabled, true);

      firstSave.resolve(records[0]);
      await flushPromises();
      await flushPromises();
      assert.equal(elements.get(formSubmitSelector)!.disabled, true);

      secondSave.resolve(records[1]);
      await flushPromises();
      await flushPromises();
      assert.equal(elements.get(formSubmitSelector)!.disabled, false);
      assert.equal(elements.get(formSelector)!.hidden, true);
    });
  });

  test(kind + ": una desactivación vigente informa éxito si otra recarga la desplaza", async () => {
    const deactivateA = deferred<(typeof records)[number]>();
    const deactivateB = deferred<(typeof records)[number]>();
    const pendingRefresh = deferred<(typeof records)[number][]>();
    const deactivateMethod = isProducts ? "deactivateProduct" : "deactivateLocation";
    let listCalls = 0;
    await withMountedView(kind, {
      [listMethod]: () => {
        listCalls += 1;
        if (listCalls === 1 || listCalls === 3) return Promise.resolve(records);
        return pendingRefresh.promise;
      },
      [deactivateMethod]: (key: string) => key === keyFor(records[0])
        ? deactivateA.promise
        : deactivateB.promise,
    }, async (elements, view) => {
      await view.refresh();
      const [deactivateButtonA, deactivateButtonB] = renderedDeactivationButtons(elements);
      deactivateButtonA.dispatch("click");
      deactivateButtonB.dispatch("click");

      deactivateB.resolve(records[1]);
      await flushPromises();
      assert.equal(listCalls, 2);

      deactivateA.resolve(records[0]);
      await flushPromises();
      await flushPromises();
      assert.equal(listCalls, 3);
      pendingRefresh.resolve(records);
      await flushPromises();
      await flushPromises();
      assert.equal(elements.get(statusSelector)!.textContent, isProducts
        ? "Producto SKU-B desactivado."
        : "Ubicación ZONE-B desactivada.");
    });
  });

  test(kind + ": presenta los estados vacío y error de la lista", async () => {
    const emptyMessage = isProducts
      ? "Aún no hay productos. Crea un producto para empezar el catálogo."
      : "Aún no hay ubicaciones. Crea una zona para empezar la jerarquía.";
    await withMountedView(kind, {
      [listMethod]: async () => [],
    }, async (elements, view) => {
      await view.refresh();
      const rows = elements.get(rowsSelector)!;
      assert.equal(rows.children[0].children[0].textContent, emptyMessage);
      assert.equal(elements.get(statusSelector)!.textContent, isProducts ? "0 productos cargados." : "0 ubicaciones cargadas.");
    });

    const listError = "El catálogo no está disponible.";
    await withMountedView(kind, {
      [listMethod]: async () => { throw new Error(listError); },
    }, async (elements, view) => {
      const result = await view.refresh();
      assert.deepEqual(result, { kind: "failed", message: listError });
      assert.equal(elements.get(rowsSelector)!.children[0].children[0].textContent, isProducts
        ? "No se pudo cargar el catálogo de productos."
        : "No se pudo cargar la lista de ubicaciones.");
      assert.equal(elements.get(statusSelector)!.textContent, listError);
      assert.equal(elements.get(statusSelector)!.dataset.kind, "error");
      assert.equal(elements.get(refreshButtonSelector)!.disabled, false);
    });
  });

  test(kind + ": el error IPC de guardado aparece en el formulario y el estado", async () => {
    const saveError = isProducts ? "El SKU ya existe." : "La jerarquía de la ubicación no es válida.";
    await withMountedView(kind, {
      [listMethod]: async () => records,
      [isProducts ? "createProduct" : "createLocation"]: async () => { throw new Error(saveError); },
    }, async (elements, view) => {
      await view.refresh();
      elements.get(newButtonSelector)!.dispatch("click");
      elements.get(formSelector)!.dispatch("submit");
      await flushPromises();

      assert.equal(elements.get(formSelector)!.hidden, false);
      assert.equal(elements.get(formMessageSelector)!.textContent, saveError);
      assert.equal(elements.get(formMessageSelector)!.dataset.kind, "error");
      assert.equal(elements.get(statusSelector)!.textContent, saveError);
      assert.equal(elements.get(statusSelector)!.dataset.kind, "error");
      assert.equal(elements.get(formSubmitSelector)!.disabled, false);
    });
  });

  test(kind + ": un error de guardado antiguo no reemplaza el formulario Nuevo", async () => {
    const pendingSave = deferred<(typeof records)[number]>();
    await withMountedView(kind, {
      [listMethod]: async () => records,
      [isProducts ? "createProduct" : "createLocation"]: () => pendingSave.promise,
    }, async (elements, view) => {
      await view.refresh();
      elements.get(newButtonSelector)!.dispatch("click");
      elements.get(formSelector)!.dispatch("submit");
      elements.get(newButtonSelector)!.dispatch("click");

      pendingSave.reject(new Error("fallo del guardado anterior"));
      await flushPromises();
      assert.equal(elements.get(formSelector)!.hidden, false);
      assert.equal(elements.get(formTitleSelector)!.textContent, isProducts ? "Nuevo producto" : "Nueva ubicación");
      assert.doesNotMatch(elements.get(formMessageSelector)!.textContent, /fallo del guardado anterior/);
      assert.doesNotMatch(elements.get(statusSelector)!.textContent, /fallo del guardado anterior/);
      assert.equal(elements.get(formSubmitSelector)!.disabled, false);
    });
  });
}
