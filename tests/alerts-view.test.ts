import assert from "node:assert/strict";
import test from "node:test";

import { inventoryIpc, type AlertDto } from "../src/lib/ipc.ts";
import { mountAlertsView } from "../src/views/alerts-view.ts";

class FakeElement {
  readonly children: FakeElement[] = [];
  readonly dataset: Record<string, string> = {};
  readonly attributes = new Map<string, string>();
  disabled = false;
  className = "";
  type = "";
  tabIndex = 0;
  private ownText = "";
  private readonly listeners = new Map<string, Array<() => void>>();

  readonly tagName: string;

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

  addEventListener(name: string, listener: () => void): void {
    const listeners = this.listeners.get(name) ?? [];
    listeners.push(listener);
    this.listeners.set(name, listeners);
  }

  dispatch(name: string): void {
    for (const listener of this.listeners.get(name) ?? []) listener();
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
  const match = selector.match(/^\[data-([a-z-]+)(?:=['"]([^'"]+)['"])?\]$/);
  if (!match) return false;
  const key = match[1].replace(/-([a-z])/g, (_whole, letter: string) => letter.toUpperCase());
  return key in element.dataset && (match[2] === undefined || element.dataset[key] === match[2]);
}

function makeAlert(overrides: Partial<AlertDto> = {}): AlertDto {
  return {
    id: "alert-occupied-1",
    recordId: "event-occupied-1",
    movementEventId: "event-occupied-1",
    movementAttemptId: null,
    targetKind: "movement_event",
    eventId: "event-occupied-1",
    kind: "position_occupied",
    status: "open",
    createdAtMs: 1_800_000_000_000,
    handlingUnitId: "unit-1",
    handlingUnitLabel: "PAL-1",
    eventKind: "moved",
    eventResult: "accepted",
    eventRejectionReason: null,
    occurredAtMs: 1_800_000_000_000,
    originLocationCode: "A-01",
    destinationLocationCode: "A-02",
    source: "simulator",
    ...overrides,
  };
}

type AlertElements = Record<string, FakeElement>;

async function withAlertsView(
  run: (elements: AlertElements, openedUnits: unknown[], view: ReturnType<typeof mountAlertsView>) => Promise<void>,
): Promise<void> {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  const previousCustomEvent = Object.getOwnPropertyDescriptor(globalThis, "CustomEvent");
  const originalListAlerts = inventoryIpc.listAlerts;
  const originalMarkAlertReviewed = inventoryIpc.markAlertReviewed;
  focusedElement = null;
  const elements: AlertElements = {
    "#alerts-status": new FakeElement("p"),
    "#alerts-list": new FakeElement("div"),
    "#alerts-refresh": new FakeElement("button"),
    "#alerts-title": new FakeElement("h1"),
  };
  const openedUnits: unknown[] = [];
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
    readonly init: { detail: unknown };

    constructor(type: string, init: { detail: unknown }) {
      this.type = type;
      this.init = init;
    }
    get detail(): unknown { return this.init.detail; }
  }

  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { dispatchEvent: (event: { type: string; detail: unknown }) => { openedUnits.push({ type: event.type, detail: event.detail }); return true; } },
  });
  Object.defineProperty(globalThis, "document", { configurable: true, value: fakeDocument });
  Object.defineProperty(globalThis, "CustomEvent", { configurable: true, value: FakeCustomEvent });
  inventoryIpc.listAlerts = async () => [];

  try {
    await run(elements, openedUnits, mountAlertsView());
  } finally {
    inventoryIpc.listAlerts = originalListAlerts;
    inventoryIpc.markAlertReviewed = originalMarkAlertReviewed;
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

function openButton(list: FakeElement): FakeElement {
  const button = list.querySelector<FakeElement>("[data-alert-action='open-event']");
  assert.ok(button, "la alerta debe ofrecer abrir su registro");
  return button;
}

test("una alerta de movimiento abre la unidad con la referencia exacta del evento", async () => {
  await withAlertsView(async (elements, openedUnits, view) => {
    const alert = makeAlert();
    inventoryIpc.listAlerts = async () => [alert];
    await view.refresh();

    openButton(elements["#alerts-list"]).dispatch("click");

    assert.deepEqual(openedUnits, [{
      type: "ariadna:open-unit",
      detail: {
        handlingUnitId: "unit-1",
        recordId: "event-occupied-1",
        movementEventId: "event-occupied-1",
      },
    }]);
  });
});

test("una alerta de intento rechazado abre la unidad por el recordId del intento", async () => {
  await withAlertsView(async (elements, openedUnits, view) => {
    const alert = makeAlert({
      id: "alert-attempt-42",
      recordId: "attempt:42",
      movementEventId: null,
      movementAttemptId: 42,
      targetKind: "movement_attempt",
      eventId: "command-event-42",
      eventResult: "rejected",
      eventRejectionReason: "position_occupied",
    });
    inventoryIpc.listAlerts = async () => [alert];
    await view.refresh();

    openButton(elements["#alerts-list"]).dispatch("click");

    assert.deepEqual(openedUnits, [{
      type: "ariadna:open-unit",
      detail: {
        handlingUnitId: "unit-1",
        recordId: "attempt:42",
        movementEventId: undefined,
      },
    }]);
  });
});

test("una anomalía sin ficha conserva visible la referencia del intento exacto", async () => {
  await withAlertsView(async (elements, openedUnits, view) => {
    const alert = makeAlert({
      id: "alert-unknown-label",
      recordId: "attempt:57",
      movementEventId: null,
      movementAttemptId: 57,
      targetKind: "movement_attempt",
      eventId: "unknown-label-event",
      handlingUnitId: "",
      handlingUnitLabel: null,
      eventResult: "rejected",
      eventRejectionReason: "handling_unit_not_found",
    });
    inventoryIpc.listAlerts = async () => [alert];
    await view.refresh();

    openButton(elements["#alerts-list"]).dispatch("click");

    const preview = elements["#alerts-list"].querySelector<FakeElement>("[data-alert-record-preview]");
    assert.ok(preview);
    assert.match(preview.textContent, /Identificador: attempt:57/);
    assert.match(preview.textContent, /Intento 57 · evento unknown-label-event/);
    assert.deepEqual(openedUnits, []);
  });
});

test("la vista anuncia un error de carga sin dejar controles ocupados", async () => {
  await withAlertsView(async (elements, _openedUnits, view) => {
    inventoryIpc.listAlerts = async () => { throw new Error("SQLite no disponible"); };
    await view.refresh();

    assert.equal(elements["#alerts-status"].dataset.kind, "error");
    assert.equal(elements["#alerts-status"].textContent, "SQLite no disponible");
    assert.equal(elements["#alerts-refresh"].disabled, false);
    assert.equal(elements["#alerts-refresh"].attributes.has("aria-busy"), false);
    assert.equal(elements["#alerts-list"].attributes.has("aria-busy"), false);
    assert.match(elements["#alerts-list"].textContent, /No se pudieron cargar las alertas/);
  });
});

test("marcar una alerta como revisada actualiza la tarjeta y mueve el foco al registro", async () => {
  await withAlertsView(async (elements, _openedUnits, view) => {
    const alert = makeAlert();
    let reviewedAlertId = "";
    inventoryIpc.listAlerts = async () => [alert];
    inventoryIpc.markAlertReviewed = async (alertId) => {
      reviewedAlertId = alertId;
      return { ...alert, status: "reviewed" };
    };
    await view.refresh();

    const reviewButton = elements["#alerts-list"].querySelector<FakeElement>("[data-alert-action='review']");
    assert.ok(reviewButton);
    reviewButton.dispatch("click");

    await waitFor(
      () => elements["#alerts-status"].dataset.kind === "success",
      "la confirmación de la revisión",
    );
    const card = elements["#alerts-list"].querySelector<FakeElement>("[data-alert-id]");
    assert.ok(card);
    const openMovementButton = card.querySelector<FakeElement>("[data-alert-action='open-event']");
    assert.ok(openMovementButton);
    assert.equal(reviewedAlertId, alert.id);
    assert.match(card.textContent, /Revisada/);
    assert.equal(card.querySelector("[data-alert-action='review']"), null);
    assert.equal(focusedElement, openMovementButton);
    assert.equal(elements["#alerts-status"].textContent, "Alerta “position_occupied” marcada como revisada.");
  });
});

test("si falla la revisión, la alerta sigue abierta y el foco vuelve al botón", async () => {
  await withAlertsView(async (elements, _openedUnits, view) => {
    const alert = makeAlert();
    inventoryIpc.listAlerts = async () => [alert];
    inventoryIpc.markAlertReviewed = async () => {
      throw new Error("SQLite no pudo actualizar la alerta");
    };
    await view.refresh();

    const reviewButton = elements["#alerts-list"].querySelector<FakeElement>("[data-alert-action='review']");
    assert.ok(reviewButton);
    reviewButton.dispatch("click");

    await waitFor(
      () => elements["#alerts-status"].dataset.kind === "error",
      "el error de revisión",
    );
    const card = elements["#alerts-list"].querySelector<FakeElement>("[data-alert-id]");
    assert.ok(card);
    assert.match(card.textContent, /Abierta/);
    assert.equal(reviewButton.disabled, false);
    assert.equal(card.querySelector("[data-alert-action='review']"), reviewButton);
    assert.equal(focusedElement, reviewButton);
    assert.equal(elements["#alerts-status"].textContent, "SQLite no pudo actualizar la alerta");
  });
});
