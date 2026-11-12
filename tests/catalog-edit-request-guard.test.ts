import assert from "node:assert/strict";
import test from "node:test";

import { AriadnaIpcError, inventoryIpc } from "../src/lib/ipc.ts";
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
  const filterSelector = isProducts ? "#products-filter" : "#locations-filter";
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

  test(kind + ": comunica una carga pendiente y reactiva la actualización al terminar", async () => {
    const pendingList = deferred<(typeof records)[number][]>();
    await withMountedView(kind, {
      [listMethod]: () => pendingList.promise,
    }, async (elements, view) => {
      const request = view.refresh();
      assert.equal(elements.get(refreshButtonSelector)!.disabled, true);
      assert.equal(elements.get(statusSelector)!.textContent, isProducts
        ? "Consultando catálogo…"
        : "Consultando ubicaciones…");
      assert.equal(elements.get(statusSelector)!.dataset.kind, "neutral");

      pendingList.resolve(records);
      await request;
      assert.equal(elements.get(refreshButtonSelector)!.disabled, false);
      assert.equal(elements.get(rowsSelector)!.children.length, records.length);
      assert.equal(elements.get(statusSelector)!.dataset.kind, "neutral");
    });
  });

  test(kind + ": diferencia un filtro sin coincidencias de una lista vacía", async () => {
    await withMountedView(kind, {
      [listMethod]: async () => records,
    }, async (elements, view) => {
      await view.refresh();
      const filter = elements.get(filterSelector)!;
      filter.value = "sin-coincidencias";
      filter.dispatch("input");
      assert.equal(elements.get(rowsSelector)!.children.length, 1);
      assert.equal(elements.get(rowsSelector)!.children[0].children[0].textContent, isProducts
        ? "No hay productos que coincidan con el filtro."
        : "No hay ubicaciones que coincidan con el filtro.");
      assert.equal(elements.get(statusSelector)!.textContent, isProducts
        ? "2 productos cargados."
        : "2 ubicaciones cargadas.");
      assert.equal(elements.get(statusSelector)!.dataset.kind, "neutral");

      filter.value = "";
      filter.dispatch("input");
      assert.equal(elements.get(rowsSelector)!.children.length, records.length);
    });
  });

  test(kind + ": muestra el error estructurado de una referencia sin anunciar éxito", async () => {
    const operationId = isProducts ? "op-product-reference-17" : "op-location-reference-17";
    const message = "No se encontró el registro relacionado; no se guardaron cambios.";
    await withMountedView(kind, {
      [listMethod]: async () => records,
      [getMethod]: async () => {
        throw new AriadnaIpcError({ code: "not_found", message, operationId });
      },
    }, async (elements, view) => {
      await view.refresh();
      renderedEditButtons(elements)[0].dispatch("click");
      await flushPromises();

      assert.equal(elements.get(formSelector)!.hidden, true);
      assert.equal(elements.get(statusSelector)!.dataset.kind, "error");
      assert.match(elements.get(statusSelector)!.textContent, /no se guardaron cambios/);
      assert.match(elements.get(statusSelector)!.textContent, new RegExp(operationId));
      assert.doesNotMatch(elements.get(statusSelector)!.textContent, /se guardó|actualizado/);
    });
  });

  test(kind + ": conserva el éxito de escritura cuando falla la recarga posterior", async () => {
    const operationId = isProducts ? "op-product-refresh-23" : "op-location-refresh-23";
    let listCalls = 0;
    await withMountedView(kind, {
      [listMethod]: () => {
        listCalls += 1;
        return listCalls === 1
          ? Promise.resolve(records)
          : Promise.reject(new AriadnaIpcError({
              code: "storage",
              message: "La base local no está disponible.",
              operationId,
            }));
      },
      [isProducts ? "createProduct" : "createLocation"]: async () => records[0],
    }, async (elements, view) => {
      await view.refresh();
      elements.get(newButtonSelector)!.dispatch("click");
      elements.get(formSelector)!.dispatch("submit");
      await flushPromises();
      await flushPromises();

      const status = elements.get(statusSelector)!;
      assert.equal(listCalls, 2);
      assert.equal(elements.get(formSelector)!.hidden, true);
      assert.equal(status.dataset.kind, "error");
      assert.match(status.textContent, /guardad[oa], pero no se pudo actualizar la lista/);
      assert.match(status.textContent, /La base local no está disponible/);
      assert.match(status.textContent, new RegExp(operationId));
      assert.doesNotMatch(status.textContent, /no se pudo guardar/i);
      assert.equal(elements.get(rowsSelector)!.children[0].children[0].textContent, isProducts
        ? "No se pudo cargar el catálogo de productos."
        : "No se pudo cargar la lista de ubicaciones.");
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
    const operationId = isProducts ? "op-product-conflict-11" : "op-location-hierarchy-11";
    const message = isProducts
      ? "El SKU ya existe; no se guardaron cambios."
      : "El nivel y la ubicación padre no forman una jerarquía válida; no se guardaron cambios.";
    const saveError = new AriadnaIpcError({
      code: isProducts ? "conflict" : "invalid_location_hierarchy",
      message,
      operationId,
    });
    await withMountedView(kind, {
      [listMethod]: async () => records,
      [isProducts ? "createProduct" : "createLocation"]: async () => { throw saveError; },
    }, async (elements, view) => {
      await view.refresh();
      elements.get(newButtonSelector)!.dispatch("click");
      elements.get(formSelector)!.dispatch("submit");
      await flushPromises();

      assert.equal(elements.get(formSelector)!.hidden, false);
      assert.equal(elements.get(formMessageSelector)!.textContent, saveError.message);
      assert.equal(elements.get(formMessageSelector)!.dataset.kind, "error");
      assert.equal(elements.get(statusSelector)!.textContent, saveError.message);
      assert.equal(elements.get(statusSelector)!.dataset.kind, "error");
      assert.equal(elements.get(formSubmitSelector)!.disabled, false);
      assert.match(elements.get(statusSelector)!.textContent, new RegExp(operationId));
      assert.doesNotMatch(elements.get(statusSelector)!.textContent, /guardado con éxito|guardada con éxito/i);
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

test("locations: exige padres activos compatibles y permite avanzar por la jerarquía", async () => {
  const hierarchy = [
    { id: "zone-a", code: "ZONE-A", name: "Zona A", kind: "zone" as const, parentId: null, active: true },
    { id: "zone-b", code: "ZONE-B", name: "Zona B", kind: "zone" as const, parentId: null, active: true },
    { id: "aisle-a", code: "AISLE-A", name: "Pasillo A", kind: "aisle" as const, parentId: "zone-a", active: true },
    { id: "aisle-inactive", code: "AISLE-X", name: "Pasillo inactivo", kind: "aisle" as const, parentId: "zone-b", active: false },
    { id: "rack-a", code: "RACK-A", name: "Rack A", kind: "rack" as const, parentId: "aisle-a", active: true },
    { id: "position-a", code: "POSITION-A", name: "Posición A", kind: "position" as const, parentId: "rack-a", active: true },
  ];
  await withMountedView("locations", {
    listLocations: async () => hierarchy,
  }, async (elements, view) => {
    await view.refresh();
    elements.get("#location-new")!.dispatch("click");
    const kind = elements.get("#location-kind")!;
    const parent = elements.get("#location-parent")!;
    const submit = elements.get("#location-form-submit")!;

    kind.value = "aisle";
    kind.dispatch("change");
    assert.deepEqual(parent.children.slice(1).map((option) => option.value), ["zone-a", "zone-b"]);
    assert.equal(submit.disabled, false);

    kind.value = "rack";
    kind.dispatch("change");
    assert.deepEqual(parent.children.slice(1).map((option) => option.value), ["aisle-a"]);
    assert.equal(submit.disabled, false);

    kind.value = "position";
    kind.dispatch("change");
    assert.deepEqual(parent.children.slice(1).map((option) => option.value), ["rack-a"]);
    assert.equal(submit.disabled, false);
  });

  await withMountedView("locations", {
    listLocations: async () => [],
  }, async (elements, view) => {
    await view.refresh();
    elements.get("#location-new")!.dispatch("click");
    const kind = elements.get("#location-kind")!;
    kind.value = "aisle";
    kind.dispatch("change");

    assert.match(elements.get("#location-parent-hint")!.textContent, /Primero crea una ubicación activa del nivel zona/);
    assert.equal(elements.get("#location-parent-field")!.hidden, false);
    assert.equal(elements.get("#location-parent")!.required, true);
    assert.equal(elements.get("#location-form-submit")!.disabled, true);
  });
});

test("locations: una ubicación en uso conserva la fila y muestra el error estructurado", async () => {
  const operationId = "op-location-in-use-29";
  let listCalls = 0;
  await withMountedView("locations", {
    listLocations: async () => {
      listCalls += 1;
      return locations;
    },
    deactivateLocation: async () => {
      throw new AriadnaIpcError({
        code: "in_use",
        message: "No se desactivó la ubicación porque tiene dependencias activas u ocupación.",
        operationId,
      });
    },
  }, async (elements, view) => {
    await view.refresh();
    renderedDeactivationButtons(elements)[0].dispatch("click");
    await flushPromises();

    const status = elements.get("#locations-status")!;
    assert.equal(listCalls, 1);
    assert.equal(elements.get("#locations-table-body")!.children.length, locations.length);
    assert.equal(status.dataset.kind, "error");
    assert.match(status.textContent, /No se desactivó la ubicación/);
    assert.match(status.textContent, new RegExp(operationId));
    assert.doesNotMatch(status.textContent, /Ubicación ZONE-A desactivada\./);
  });
});
