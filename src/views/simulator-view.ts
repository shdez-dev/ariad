import { AriadnaIpcError, inventoryIpc, type SimulationStepDto } from "../lib/ipc";
import {
  isSimulationScenario,
  simulationResultLabel,
  simulationResultTone,
  simulationScenarioLabel,
  validateSimulationSeed,
} from "../lib/simulator-model";

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`No se encontró el elemento requerido ${selector}.`);
  return element;
}

function messageFor(error: unknown): string {
  if (error instanceof AriadnaIpcError) return error.message;
  return error instanceof Error ? error.message : "No se pudo ejecutar la simulación.";
}

export function mountSimulatorView(): { refresh: () => Promise<void> } {
  const form = requiredElement<HTMLFormElement>("#simulator-form");
  const seedInput = requiredElement<HTMLInputElement>("#simulator-seed");
  const scenarioInput = requiredElement<HTMLSelectElement>("#simulator-scenario");
  const runButton = requiredElement<HTMLButtonElement>("#simulator-run");
  const status = requiredElement<HTMLElement>("#simulator-status");
  const resultPanel = requiredElement<HTMLElement>("#simulator-result-panel");
  const results = requiredElement<HTMLOListElement>("#simulator-steps");
  const recordDetail = requiredElement<HTMLElement>("#simulator-record-detail");
  let running = false;

  function setStatus(message: string, kind: "neutral" | "success" | "error" = "neutral"): void {
    status.textContent = message;
    status.dataset.kind = kind;
  }

  function renderRecordDetail(step: SimulationStepDto): void {
    recordDetail.replaceChildren();
    const header = document.createElement("div");
    header.className = "detail-header";
    const kicker = document.createElement("p");
    kicker.className = "detail-kicker";
    kicker.textContent = "REGISTRO SINTÉTICO";
    const title = document.createElement("h2");
    title.className = "detail-title";
    title.tabIndex = -1;
    title.textContent = step.label;
    const identity = document.createElement("p");
    identity.className = "detail-name";
    identity.textContent = step.recordId ?? step.id;
    header.append(kicker, title, identity);

    const message = document.createElement("p");
    message.className = step.result.toLocaleLowerCase("en-US") === "rejected"
      ? "detail-warning"
      : "detail-muted";
    message.textContent = `${simulationResultLabel(step.result)}. ${step.message}`;

    const details = document.createElement("dl");
    details.className = "detail-list";
    appendDetailRow(details, "Evento", step.eventId ?? "No informado");
    appendDetailRow(details, "Unidad", step.handlingUnitId ?? "Sin unidad persistida");
    if (step.alertId) appendDetailRow(details, "Alerta relacionada", step.alertId);
    recordDetail.append(header, message, details);
    title.focus();
  }

  function appendDetailRow(list: HTMLDListElement, label: string, value: string): void {
    const row = document.createElement("div");
    row.className = "detail-row";
    const term = document.createElement("dt");
    term.className = "detail-label";
    term.textContent = label;
    const description = document.createElement("dd");
    description.className = "detail-value";
    description.textContent = value;
    row.append(term, description);
    list.append(row);
  }

  function openRecord(step: SimulationStepDto): void {
    const handlingUnitId = step.handlingUnitId?.trim();
    const recordId = step.recordId ?? step.eventId ?? step.id;
    if (!handlingUnitId) {
      renderRecordDetail(step);
      return;
    }
    window.dispatchEvent(new CustomEvent("ariadna:open-unit", {
      detail: { handlingUnitId, recordId, movementEventId: step.eventId ?? undefined },
    }));
  }

  function openAlert(alertId: string): void {
    window.dispatchEvent(new CustomEvent("ariadna:open-alert", { detail: { alertId } }));
  }

  function renderSteps(steps: SimulationStepDto[]): void {
    results.replaceChildren();
    if (steps.length === 0) {
      const empty = document.createElement("li");
      empty.className = "simulation-empty";
      empty.textContent = "La ejecución terminó sin pasos que mostrar.";
      results.append(empty);
      return;
    }

    steps.forEach((step, index) => {
      const item = document.createElement("li");
      item.className = "simulation-step";
      item.dataset.simulationStepId = step.id;

      const heading = document.createElement("div");
      heading.className = "simulation-step-heading";
      const title = document.createElement("h3");
      title.textContent = `${index + 1}. ${step.label}`;
      const outcome = document.createElement("span");
      outcome.className = `record-badge simulation-result-${simulationResultTone(step.result)}`;
      outcome.textContent = simulationResultLabel(step.result);
      heading.append(title, outcome);

      const message = document.createElement("p");
      message.className = step.result.toLocaleLowerCase("en-US") === "rejected"
        ? "detail-warning"
        : "detail-muted";
      message.textContent = step.message;

      const syntheticTag = document.createElement("span");
      syntheticTag.className = "simulation-source-badge";
      syntheticTag.textContent = "Dato sintético";

      const actions = document.createElement("div");
      actions.className = "simulation-step-actions";
      const recordId = step.recordId ?? step.eventId ?? step.id;
      if (step.recordId || step.eventId) {
        const recordButton = document.createElement("button");
        recordButton.type = "button";
        recordButton.className = "secondary-button";
        recordButton.textContent = step.handlingUnitId ? "Abrir registro" : "Mostrar registro exacto";
        recordButton.setAttribute("aria-label", `Abrir el registro exacto ${recordId}: ${step.label}`);
        recordButton.addEventListener("click", () => openRecord(step));
        actions.append(recordButton);
      }
      if (step.alertId) {
        const alertButton = document.createElement("button");
        alertButton.type = "button";
        alertButton.className = "primary-button";
        alertButton.textContent = "Abrir alerta";
        alertButton.setAttribute("aria-label", `Abrir la alerta ${step.alertId} del paso ${index + 1}`);
        alertButton.addEventListener("click", () => openAlert(step.alertId!));
        actions.append(alertButton);
      }

      item.append(heading, message, syntheticTag);
      if (step.recordId || step.eventId) {
        const ref = document.createElement("p");
        ref.className = "simulation-record-id";
        ref.textContent = `Registro: ${recordId}`;
        item.append(ref);
      }
      item.append(actions);
      results.append(item);
    });
  }

  async function execute(event?: SubmitEvent): Promise<void> {
    event?.preventDefault();
    if (running) return;
    if (!form.reportValidity()) return;
    const seed = seedInput.value.trim();
    const seedError = validateSimulationSeed(seed);
    if (seedError) {
      setStatus(seedError, "error");
      seedInput.focus();
      return;
    }
    const scenario = scenarioInput.value;
    if (!isSimulationScenario(scenario)) {
      setStatus("Selecciona un recorrido disponible.", "error");
      scenarioInput.focus();
      return;
    }

    running = true;
    seedInput.disabled = true;
    scenarioInput.disabled = true;
    runButton.setAttribute("aria-disabled", "true");
    runButton.textContent = "Ejecutando…";
    resultPanel.setAttribute("aria-busy", "true");
    results.replaceChildren();
    recordDetail.replaceChildren();
    setStatus(`Ejecutando ${simulationScenarioLabel(scenario).toLocaleLowerCase("es-CL")} con semilla “${seed}”…`);
    try {
      const run = await inventoryIpc.runSimulation({ seed, scenario });
      renderSteps(run.steps);
      const label = simulationScenarioLabel(run.scenario);
      setStatus(`${run.summary} Recorrido: ${label}. Semilla: ${run.seed}. Todos los resultados están identificados como datos sintéticos.`, "success");
    } catch (error: unknown) {
      results.replaceChildren();
      const warning = document.createElement("li");
      warning.className = "simulation-empty";
      warning.textContent = "La ejecución pudo guardar pasos anteriores al error. Reintenta con la misma semilla y el mismo recorrido para conservar la idempotencia.";
      results.append(warning);
      setStatus(messageFor(error), "error");
    } finally {
      running = false;
      seedInput.disabled = false;
      scenarioInput.disabled = false;
      runButton.removeAttribute("aria-disabled");
      runButton.textContent = "Ejecutar simulación";
      resultPanel.removeAttribute("aria-busy");
    }
  }

  form.addEventListener("submit", (event) => void execute(event));
  form.addEventListener("click", (event) => {
    if ((event.target as HTMLElement).closest("#simulator-run") && running) event.preventDefault();
  });

  return { refresh: async () => undefined };
}
