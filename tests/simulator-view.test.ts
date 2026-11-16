import assert from "node:assert/strict";
import test from "node:test";

import {
  inventoryIpc,
  type SimulationRunDto,
  type SimulationRunInput,
} from "../src/lib/ipc.ts";
import { mountSimulatorView } from "../src/views/simulator-view.ts";

type Listener = (event: { preventDefault(): void; target: FakeElement }) => void;

class FakeElement {
  readonly tagName: string;
  readonly children: FakeElement[] = [];
  readonly dataset: Record<string, string> = {};
  readonly attributes = new Map<string, string>();
  private ownText = "";
  private readonly listeners = new Map<string, Listener[]>();

  disabled = false;
  value = "";
  className = "";
  type = "";
  tabIndex = 0;

  constructor(tagName = "div") {
    this.tagName = tagName;
  }

  get textContent(): string {
    return this.ownText + this.children.map((child) => child.textContent).join("");
  }

  set textContent(value: string) {
    this.ownText = value;
    this.children.splice(0);
  }

  append(...children: FakeElement[]): void {
    this.children.push(...children);
  }

  replaceChildren(...children: FakeElement[]): void {
    this.ownText = "";
    this.children.splice(0, this.children.length, ...children);
  }

  addEventListener(name: string, listener: Listener): void {
    const listeners = this.listeners.get(name) ?? [];
    listeners.push(listener);
    this.listeners.set(name, listeners);
  }

  dispatch(name: string): void {
    for (const listener of this.listeners.get(name) ?? []) {
      listener({ preventDefault() {}, target: this });
    }
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  focus(): void {
    focusedElement = this;
  }

  reportValidity(): boolean {
    return true;
  }

  querySelector<T extends Element>(selector: string): T | null {
    return this.querySelectorAll<T>(selector)[0] ?? null;
  }

  querySelectorAll<T extends Element>(selector: string): T[] {
    const matches: FakeElement[] = [];
    const visit = (node: FakeElement) => {
      for (const child of node.children) {
        if (matchesSelector(child, selector)) matches.push(child);
        visit(child);
      }
    };
    visit(this);
    return matches as unknown as T[];
  }
}

let focusedElement: FakeElement | null = null;

function matchesSelector(element: FakeElement, selector: string): boolean {
  if (selector === "button") return element.tagName === "button";
  const attribute = selector.match(/^\[data-([a-z-]+)\]$/);
  if (!attribute) return false;
  const key = attribute[1].replace(/-([a-z])/g, (_match, letter: string) => letter.toUpperCase());
  return key in element.dataset;
}

type SimulatorElements = Record<string, FakeElement>;

async function withSimulatorView(
  run: (elements: SimulatorElements, events: Array<{ type: string; detail: unknown }>) => Promise<void>,
): Promise<void> {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const previousCustomEvent = Object.getOwnPropertyDescriptor(globalThis, "CustomEvent");
  const originalRunSimulation = inventoryIpc.runSimulation;
  const elements: SimulatorElements = {
    "#simulator-form": new FakeElement("form"),
    "#simulator-seed": new FakeElement("input"),
    "#simulator-scenario": new FakeElement("select"),
    "#simulator-run": new FakeElement("button"),
    "#simulator-status": new FakeElement("p"),
    "#simulator-result-panel": new FakeElement("section"),
    "#simulator-steps": new FakeElement("ol"),
    "#simulator-record-detail": new FakeElement("aside"),
  };
  const events: Array<{ type: string; detail: unknown }> = [];
  focusedElement = null;

  const fakeDocument = {
    querySelector<T extends Element>(selector: string): T | null {
      return (elements[selector] as unknown as T | undefined) ?? null;
    },
    createElement<T extends HTMLElement>(tagName: string): T {
      return new FakeElement(tagName) as unknown as T;
    },
  };
  class FakeCustomEvent {
    readonly type: string;
    readonly detail: unknown;

    constructor(type: string, init: { detail: unknown }) {
      this.type = type;
      this.detail = init.detail;
    }
  }

  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      dispatchEvent: (event: { type: string; detail: unknown }) => {
        events.push({ type: event.type, detail: event.detail });
        return true;
      },
    },
  });
  Object.defineProperty(globalThis, "document", { configurable: true, value: fakeDocument });
  Object.defineProperty(globalThis, "CustomEvent", { configurable: true, value: FakeCustomEvent });

  try {
    mountSimulatorView();
    await run(elements, events);
  } finally {
    inventoryIpc.runSimulation = originalRunSimulation;
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else delete (globalThis as typeof globalThis & { window?: unknown }).window;
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else delete (globalThis as typeof globalThis & { document?: unknown }).document;
    if (previousCustomEvent) Object.defineProperty(globalThis, "CustomEvent", previousCustomEvent);
    else delete (globalThis as typeof globalThis & { CustomEvent?: unknown }).CustomEvent;
  }
}

async function waitFor(predicate: () => boolean, label: string): Promise<void> {
  const deadline = Date.now() + 1_000;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  assert.fail("Tiempo agotado esperando " + label);
}

test("la vista envía semilla/escenario y enlaza registros y alertas sintéticos exactos", async () => {
  await withSimulatorView(async (elements, events) => {
    const input: SimulationRunInput = { seed: "SEED-ARIADNA-2026", scenario: "unknown_label" };
    const output: SimulationRunDto = {
      seed: input.seed,
      scenario: input.scenario,
      summary: "2 pasos procesados.",
      steps: [
        {
          id: "step-1",
          label: "Recepción aplicada",
          result: "applied",
          message: "La unidad quedó registrada.",
          recordId: "record-exact-1",
          eventId: "event-command-1",
          handlingUnitId: "unit-1",
        },
        {
          id: "step-2",
          label: "Etiqueta desconocida",
          result: "rejected",
          message: "La etiqueta no existe.",
          recordId: "attempt:42",
          eventId: "event-command-42",
          handlingUnitId: null,
          alertId: "alert-exact-42",
        },
      ],
    };
    let submitted: SimulationRunInput | null = null;
    let resolveRun: ((result: SimulationRunDto) => void) | undefined;
    inventoryIpc.runSimulation = async (runInput) => {
      submitted = structuredClone(runInput);
      return await new Promise((resolve) => {
        resolveRun = resolve;
      });
    };
    elements["#simulator-seed"].value = input.seed;
    elements["#simulator-scenario"].value = input.scenario;
    elements["#simulator-form"].dispatch("submit");

    await waitFor(() => submitted !== null, "la solicitud IPC de simulación");
    assert.deepEqual(submitted, input);
    assert.equal(elements["#simulator-status"].dataset.kind, "neutral");
    assert.match(elements["#simulator-status"].textContent, /Ejecutando etiqueta desconocida/);
    assert.equal(elements["#simulator-seed"].disabled, true);
    assert.equal(elements["#simulator-scenario"].disabled, true);
    assert.equal(elements["#simulator-result-panel"].attributes.get("aria-busy"), "true");

    resolveRun?.(output);
    await waitFor(
      () => elements["#simulator-status"].dataset.kind === "success",
      "el resultado de la simulación",
    );
    assert.equal(elements["#simulator-result-panel"].attributes.has("aria-busy"), false);
    assert.equal(elements["#simulator-seed"].disabled, false);
    assert.equal(elements["#simulator-scenario"].disabled, false);
    assert.equal(elements["#simulator-run"].attributes.has("aria-disabled"), false);
    assert.match(elements["#simulator-status"].textContent, /Todos los resultados están identificados como datos sintéticos/);

    const steps = elements["#simulator-steps"].querySelectorAll<FakeElement>("[data-simulation-step-id]");
    assert.equal(steps.length, 2);
    assert.match(steps[0].textContent, /Dato sintético/);
    assert.match(steps[0].textContent, /Registro: record-exact-1/);
    assert.match(steps[1].textContent, /Dato sintético/);
    assert.match(steps[1].textContent, /Rechazado/);

    const acceptedRecordButton = steps[0].querySelector<FakeElement>("button");
    assert.ok(acceptedRecordButton);
    assert.equal(
      acceptedRecordButton.attributes.get("aria-label"),
      "Abrir el registro exacto record-exact-1: Recepción aplicada",
    );
    acceptedRecordButton.dispatch("click");
    assert.deepEqual(events[0], {
      type: "ariadna:open-unit",
      detail: {
        handlingUnitId: "unit-1",
        recordId: "record-exact-1",
        movementEventId: "event-command-1",
      },
    });

    const rejectedButtons = steps[1].querySelectorAll<FakeElement>("button");
    assert.equal(rejectedButtons.length, 2);
    const openAlertButton = rejectedButtons.find((button) => button.textContent === "Abrir alerta");
    assert.ok(openAlertButton);
    assert.equal(openAlertButton.attributes.get("aria-label"), "Abrir la alerta alert-exact-42 del paso 2");
    openAlertButton.dispatch("click");
    assert.deepEqual(events[1], {
      type: "ariadna:open-alert",
      detail: { alertId: "alert-exact-42" },
    });
    rejectedButtons.find((button) => button.textContent === "Mostrar registro exacto")!.dispatch("click");
    assert.match(elements["#simulator-record-detail"].textContent, /REGISTRO SINTÉTICO/);
    assert.match(elements["#simulator-record-detail"].textContent, /attempt:42/);
    assert.equal(focusedElement?.textContent, "Etiqueta desconocida");
  });
});

test("la vista comunica el error de ejecución y libera los controles ocupados", async () => {
  await withSimulatorView(async (elements) => {
    inventoryIpc.runSimulation = async () => {
      throw new Error("SQLite no disponible durante la simulación");
    };
    elements["#simulator-seed"].value = "SEED-ARIADNA-2026";
    elements["#simulator-scenario"].value = "normal";
    elements["#simulator-form"].dispatch("submit");

    await waitFor(
      () => elements["#simulator-status"].dataset.kind === "error",
      "el error de la simulación",
    );
    assert.equal(elements["#simulator-status"].textContent, "SQLite no disponible durante la simulación");
    assert.match(elements["#simulator-steps"].textContent, /Reintenta con la misma semilla/);
    assert.equal(elements["#simulator-result-panel"].attributes.has("aria-busy"), false);
    assert.equal(elements["#simulator-seed"].disabled, false);
    assert.equal(elements["#simulator-scenario"].disabled, false);
    assert.equal(elements["#simulator-run"].attributes.has("aria-disabled"), false);
    assert.equal(elements["#simulator-run"].textContent, "Ejecutar simulación");
  });
});
