import type { MovementHistoryEntryDto, SimulationScenario } from "./ipc";

export const simulationScenarios: readonly SimulationScenario[] = [
  "normal",
  "duplicate",
  "unknown_label",
  "invalid_location",
  "occupied_destination",
];

const scenarioLabels: Record<SimulationScenario, string> = {
  normal: "Recorrido normal",
  duplicate: "Evento duplicado",
  unknown_label: "Etiqueta desconocida",
  invalid_location: "Posición inválida",
  occupied_destination: "Destino ocupado",
};

export function isSimulationScenario(value: string): value is SimulationScenario {
  return simulationScenarios.some((scenario) => scenario === value);
}

export function simulationScenarioLabel(scenario: SimulationScenario): string {
  return scenarioLabels[scenario];
}

export function isSimulatedSource(source: string | null | undefined): boolean {
  return source === "simulator";
}

export function simulationResultLabel(result: string): string {
  const labels: Record<string, string> = {
    applied: "Aplicado",
    accepted: "Aceptado",
    duplicate: "Duplicado",
    rejected: "Rechazado",
    alert_created: "Alerta creada",
    skipped: "Omitido",
  };
  return labels[result.toLocaleLowerCase("en-US")] ?? result;
}

export function simulationResultTone(result: string): "success" | "warning" | "error" | "neutral" {
  switch (result.toLocaleLowerCase("en-US")) {
    case "applied":
    case "accepted":
    case "alert_created":
      return "success";
    case "duplicate":
    case "skipped":
      return "warning";
    case "rejected":
      return "error";
    default:
      return "neutral";
  }
}

export function validateSimulationSeed(seed: string): string | null {
  if (!seed.trim()) return "Escribe una semilla para poder repetir el recorrido.";
  if (seed.trim().length > 80) return "La semilla no puede superar 80 caracteres.";
  return null;
}

export function findHistoryEntry(
  entries: readonly MovementHistoryEntryDto[],
  reference: string | null | undefined,
): MovementHistoryEntryDto | undefined {
  if (!reference) return undefined;
  return entries.find((entry) => entry.recordId === reference)
    ?? entries.find((entry) => entry.eventId === reference);
}
