import assert from "node:assert/strict";
import test from "node:test";

import { createAlertsRefreshGuard } from "../src/lib/alerts-refresh-guard.ts";
import { createAlertsViewState } from "../src/lib/alerts-view-state.ts";
import type { AlertDto } from "../src/lib/ipc.ts";

function alert(status: AlertDto["status"]): AlertDto {
  return {
    id: "alert-1",
    recordId: "event-1",
    movementEventId: "event-1",
    movementAttemptId: null,
    targetKind: "movement_event",
    eventId: "event-1",
    kind: "position_occupied",
    status,
    createdAtMs: 1,
    handlingUnitId: "unit-1",
    handlingUnitLabel: "PAL-1",
    eventKind: "stored",
    eventResult: "accepted",
    eventRejectionReason: null,
    occurredAtMs: 1,
    originLocationCode: null,
    destinationLocationCode: "P-01",
    source: "simulator",
  };
}

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

test("una respuesta antigua no revierte una alerta marcada como revisada", () => {
  const state = createAlertsViewState();
  const initial = state.beginRefresh();
  assert.equal(state.applyRefreshSuccess(initial, [alert("open")]), true);

  const pending = state.beginRefresh();
  state.applyReviewed(alert("reviewed"));

  assert.equal(state.applyRefreshSuccess(pending, [alert("open")]), false);
  assert.deepEqual(state.snapshot().alerts.map((item) => item.status), ["reviewed"]);
});

test("una recarga fallida conserva las alertas revisadas y expone el estado de error", () => {
  const state = createAlertsViewState();
  const initial = state.beginRefresh();
  assert.equal(state.applyRefreshSuccess(initial, [alert("open")]), true);
  state.applyReviewed(alert("reviewed"));

  const retry = state.beginRefresh();
  assert.equal(state.applyRefreshFailure(retry), true);
  assert.deepEqual(state.snapshot(), { alerts: [alert("reviewed")], loadFailed: true });
});
