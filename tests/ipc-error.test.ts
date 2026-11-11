import assert from "node:assert/strict";
import test from "node:test";

import { AriadnaIpcError } from "../src/lib/ipc.ts";

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
