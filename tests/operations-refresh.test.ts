import assert from "node:assert/strict";
import test from "node:test";

import { createLatestRefreshCoordinator } from "../src/lib/latest-refresh-coordinator.ts";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function handlers<T>(
  label: string,
  applied: T[],
  errors: unknown[],
  settled: string[],
) {
  return {
    onLoaded: (value: T) => applied.push(value),
    onError: (error: unknown) => {
      errors.push(error);
      return `${label} failed`;
    },
    onSettled: () => settled.push(label),
  };
}

test("una recarga obsoleta no reemplaza los datos de la más reciente", async () => {
  const coordinator = createLatestRefreshCoordinator();
  const older = deferred<string[]>();
  const latest = deferred<string[]>();
  const applied: string[][] = [];
  const errors: unknown[] = [];
  const settled: string[] = [];

  const olderRun = coordinator.run(
    () => older.promise,
    handlers("older", applied, errors, settled),
  );
  const latestRun = coordinator.run(
    () => latest.promise,
    handlers("latest", applied, errors, settled),
  );

  latest.resolve(["datos actuales"]);
  assert.deepEqual(await latestRun, { kind: "loaded" });
  older.resolve(["datos obsoletos"]);
  assert.deepEqual(await olderRun, { kind: "superseded" });

  assert.deepEqual(applied, [["datos actuales"]]);
  assert.deepEqual(errors, []);
  assert.deepEqual(settled, ["latest"]);
});

test("el error y el cierre de una recarga obsoleta no pisan el estado actual", async () => {
  const coordinator = createLatestRefreshCoordinator();
  const older = deferred<string[]>();
  const latest = deferred<string[]>();
  const applied: string[][] = [];
  const errors: unknown[] = [];
  const settled: string[] = [];

  const olderRun = coordinator.run(
    () => older.promise,
    handlers("older", applied, errors, settled),
  );
  const latestRun = coordinator.run(
    () => latest.promise,
    handlers("latest", applied, errors, settled),
  );

  latest.resolve(["datos actuales"]);
  assert.deepEqual(await latestRun, { kind: "loaded" });
  const staleError = new Error("respuesta antigua fallida");
  older.reject(staleError);
  assert.deepEqual(await olderRun, { kind: "superseded" });

  assert.deepEqual(applied, [["datos actuales"]]);
  assert.deepEqual(errors, []);
  assert.deepEqual(settled, ["latest"]);
});

test("un error de la recarga vigente se comunica y finaliza su estado", async () => {
  const coordinator = createLatestRefreshCoordinator();
  const current = deferred<string[]>();
  const applied: string[][] = [];
  const errors: unknown[] = [];
  const settled: string[] = [];
  const failure = new Error("falló la consulta actual");

  const currentRun = coordinator.run(
    () => current.promise,
    handlers("current", applied, errors, settled),
  );

  current.reject(failure);
  assert.deepEqual(await currentRun, { kind: "failed", message: "current failed" });

  assert.deepEqual(applied, []);
  assert.deepEqual(errors, [failure]);
  assert.deepEqual(settled, ["current"]);
});
