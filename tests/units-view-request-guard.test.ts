import assert from "node:assert/strict";
import test from "node:test";

import { createUnitsViewRequestGuard } from "../src/lib/units-view-request-guard.ts";

test("una navegación de detalle posterior invalida el resultado de una recarga anterior", () => {
  const guard = createUnitsViewRequestGuard();
  const refresh = guard.beginRefresh();
  assert.equal(guard.canApplyRefreshResult(refresh), true);

  const navigation = guard.beginDetailRequest();

  assert.equal(guard.isLatestRefresh(refresh), true);
  assert.equal(guard.canApplyRefreshResult(refresh), false);
  assert.equal(guard.isCurrentDetailRequest(navigation), true);
  assert.equal(guard.isCurrentDetailRequest(refresh.detailRequestNumber), false);
});

test("una recarga posterior invalida la limpieza de carga de un detalle anterior", () => {
  const guard = createUnitsViewRequestGuard();
  const detail = guard.beginDetailRequest();
  const refresh = guard.beginRefresh();

  assert.equal(guard.isCurrentDetailRequest(detail), false);
  assert.equal(guard.isLatestRefresh(refresh), true);
  assert.equal(guard.canApplyRefreshResult(refresh), true);
});

test("solo la recarga más reciente conserva autoridad sobre su lista", () => {
  const guard = createUnitsViewRequestGuard();
  const older = guard.beginRefresh();
  const latest = guard.beginRefresh();

  assert.equal(guard.isLatestRefresh(older), false);
  assert.equal(guard.canApplyRefreshResult(older), false);
  assert.equal(guard.isLatestRefresh(latest), true);
  assert.equal(guard.canApplyRefreshResult(latest), true);
});
