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
