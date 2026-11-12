import assert from "node:assert/strict";
import test from "node:test";

import { AriadnaIpcError, inventoryIpc } from "../src/lib/ipc.ts";

test("muestra el identificador que permite encontrar el diagnóstico local", () => {
  const error = new AriadnaIpcError({
    code: "storage",
    message: "No se pudo completar la operación.",
    operationId: "op-abc-1",
  });

  assert.equal(error.code, "storage");
  assert.equal(error.operationId, "op-abc-1");
  assert.match(error.message, /ID de operación: op-abc-1/);
});

test("mantiene los errores sin identificador en forma compatible", () => {
  const error = new AriadnaIpcError({
    code: "validation",
    message: "Revisa los datos ingresados.",
  });

  assert.equal(error.message, "Revisa los datos ingresados.");
  assert.equal(error.operationId, undefined);
});

test("convierte un rechazo IPC del backend y conserva el código y el identificador", async () => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      __TAURI_INTERNALS__: {
        invoke: async () => {
          throw {
            code: "invalid_location_hierarchy",
            message: "No se guardaron cambios por una jerarquía inválida.",
            operationId: "op-hierarchy-42",
          };
        },
      },
    },
  });

  try {
    await assert.rejects(inventoryIpc.listProducts(), (error: unknown) => {
      assert.ok(error instanceof AriadnaIpcError);
      assert.equal(error.code, "invalid_location_hierarchy");
      assert.equal(error.operationId, "op-hierarchy-42");
      assert.match(error.message, /ID de operación: op-hierarchy-42/);
      return true;
    });
  } finally {
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else delete (globalThis as typeof globalThis & { window?: unknown }).window;
  }
});

test("notifica cambios de inventario solo después de una escritura IPC confirmada", async () => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const events: string[] = [];
  const response = { result: "applied", eventId: "event-confirmed" };
  let writeConfirmed = false;
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      __TAURI_INTERNALS__: {
        invoke: async (command: string) => {
          assert.equal(command, "receive_unit");
          writeConfirmed = true;
          return response;
        },
      },
      dispatchEvent: (event: Event) => {
        assert.equal(writeConfirmed, true);
        events.push(event.type);
        return true;
      },
    },
  });

  try {
    const result = await inventoryIpc.receiveUnit({
      id: "unit-1",
      label: "UNIT-TEST-1",
      lines: [{ sku: "SKU-1", quantity: 1 }],
      eventId: "event-1",
      occurredAtMs: 1_800_000_000_000,
    });

    assert.deepEqual(result, response);
    assert.deepEqual(events, ["ariadna:inventory-changed"]);
  } finally {
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else delete (globalThis as typeof globalThis & { window?: unknown }).window;
  }
});

test("no notifica cambios de inventario cuando falla la escritura IPC", async () => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const events: string[] = [];
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      __TAURI_INTERNALS__: {
        invoke: async () => {
          throw {
            code: "conflict",
            message: "No se guardaron cambios por un conflicto.",
            operationId: "op-conflict-9",
          };
        },
      },
      dispatchEvent: (event: Event) => {
        events.push(event.type);
        return true;
      },
    },
  });

  try {
    await assert.rejects(inventoryIpc.receiveUnit({
      id: "unit-1",
      label: "UNIT-TEST-1",
      lines: [{ sku: "SKU-1", quantity: 1 }],
      eventId: "event-1",
      occurredAtMs: 1_800_000_000_000,
    }), (error: unknown) => {
      assert.ok(error instanceof AriadnaIpcError);
      assert.equal(error.code, "conflict");
      assert.equal(error.operationId, "op-conflict-9");
      return true;
    });

    assert.deepEqual(events, []);
  } finally {
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else delete (globalThis as typeof globalThis & { window?: unknown }).window;
  }
});
