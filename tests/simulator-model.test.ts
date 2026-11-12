import assert from "node:assert/strict";
import test from "node:test";

import type { MovementHistoryEntryDto } from "../src/lib/ipc.ts";
import {
  findHistoryEntry,
  isSimulatedSource,
  isSimulationScenario,
  movementOccurredLabel,
  simulationResultLabel,
  simulationResultTone,
  simulationScenarioLabel,
  validateSimulationSeed,
} from "../src/lib/simulator-model.ts";

const historyEntry = (recordId: string, eventId: string): MovementHistoryEntryDto => ({
  recordId,
  eventId,
  handlingUnitId: "unit-1",
  kind: "moved",
  originLocationId: null,
  destinationLocationId: null,
  occurredAtMs: 1_800_000_000_000,
  recordedAtMs: 1_800_000_000_001,
  source: "deterministic_simulator",
  result: "rejected",
  rejectionReason: "destino ocupado",
  correctsEventId: null,
  correctionReason: null,
});

test("acepta solo los escenarios de simulación declarados y los etiqueta en español", () => {
  assert.equal(isSimulationScenario("normal"), true);
  assert.equal(isSimulationScenario("occupied_destination"), true);
  assert.equal(isSimulationScenario("unlisted"), false);
  assert.equal(simulationScenarioLabel("unknown_label"), "Etiqueta desconocida");
});

test("marca como sintética solo la fuente exacta del simulador", () => {
  assert.equal(isSimulatedSource("simulator"), true);
  assert.equal(isSimulatedSource("desktop_ui"), false);
  assert.equal(isSimulatedSource(null), false);
  assert.equal(isSimulatedSource(undefined), false);
});

test("distingue una fecha de ocurrencia simulada de la fecha de registro", () => {
  assert.equal(movementOccurredLabel("simulator"), "Fecha simulada");
  assert.equal(movementOccurredLabel("desktop_ui"), "Ocurrido");
  assert.equal(movementOccurredLabel(null), "Ocurrido");
});

test("valida la semilla sin cambiar la cadena que se usará en el recorrido", () => {
  assert.match(validateSimulationSeed("   ") ?? "", /semilla/i);
  assert.equal(validateSimulationSeed(` ${"s".repeat(81)} `), "La semilla no puede superar 80 caracteres.");
  assert.equal(validateSimulationSeed(" lote-12 "), null);
});

test("etiqueta resultados conocidos y conserva los resultados nuevos", () => {
  assert.equal(simulationResultLabel("duplicate"), "Duplicado");
  assert.equal(simulationResultLabel("rejected"), "Rechazado");
  assert.equal(simulationResultLabel("custom"), "custom");
  assert.equal(simulationResultTone("duplicate"), "warning");
  assert.equal(simulationResultTone("rejected"), "error");
});

test("resuelve primero el recordId exacto si dos filas comparten eventId", () => {
  const accepted = historyEntry("accepted-17", "event-17");
  const rejectedAttempt = historyEntry("attempt:44", "event-17");
  const entries = [accepted, rejectedAttempt];

  assert.equal(findHistoryEntry(entries, "attempt:44"), rejectedAttempt);
  assert.equal(findHistoryEntry(entries, "event-17"), accepted);
  assert.equal(findHistoryEntry(entries, "missing"), undefined);
});
