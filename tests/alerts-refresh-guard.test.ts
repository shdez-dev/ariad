import assert from "node:assert/strict";
import test from "node:test";

import { createAlertsRefreshGuard } from "../src/lib/alerts-refresh-guard.ts";

test("una revisión de alerta invalida una respuesta de lista anterior", () => {
  const guard = createAlertsRefreshGuard();
  const pending = guard.begin();

  guard.invalidate();

  assert.equal(guard.canApply(pending), false);
  assert.equal(guard.isLatest(pending), true);
});

test("solo la petición de refresco más reciente puede aplicarse", () => {
  const guard = createAlertsRefreshGuard();
  const older = guard.begin();
  const latest = guard.begin();

  assert.equal(guard.canApply(older), false);
  assert.equal(guard.isLatest(older), false);
  assert.equal(guard.canApply(latest), true);
  assert.equal(guard.isLatest(latest), true);
});
