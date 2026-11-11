import { AriadnaIpcError, inventoryIpc, type AlertDto } from "../lib/ipc";

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`No se encontró el elemento requerido ${selector}.`);
  return element;
}

function messageFor(error: unknown): string {
  if (error instanceof AriadnaIpcError) return error.message;
  return error instanceof Error ? error.message : "No se pudo completar la consulta.";
}

function eventKindLabel(kind: string): string {
  const labels: Record<string, string> = {
    received: "Recepción",
    stored: "Asignación de posición",
    moved: "Traslado",
    dispatched: "Despacho",
  };
  return labels[kind] ?? kind;
}

function alertStatusLabel(status: AlertDto["status"]): string {
  const labels: Record<AlertDto["status"], string> = {
    open: "Abierta",
    reviewed: "Revisada",
    resolved: "Resuelta",
  };
  return labels[status];
}

function dateLabel(timestamp: number): string {
  return new Intl.DateTimeFormat("es-CL", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(timestamp);
}

export function mountAlertsView(): { refresh: () => Promise<void> } {
  const status = requiredElement<HTMLElement>("#alerts-status");
  const list = requiredElement<HTMLElement>("#alerts-list");
  const refreshButton = requiredElement<HTMLButtonElement>("#alerts-refresh");
  let alerts: AlertDto[] = [];
  let requestNumber = 0;
  let loadFailed = false;

  function setStatus(message: string, kind: "neutral" | "success" | "error" = "neutral"): void {
    status.textContent = message;
    status.dataset.kind = kind;
  }

  function render(): void {
    list.replaceChildren();
    if (alerts.length === 0) {
      const empty = document.createElement("div");
      empty.className = "alert-empty";
      const title = document.createElement("h2");
      title.textContent = loadFailed ? "No se pudieron cargar las alertas" : "No hay alertas guardadas";
      const copy = document.createElement("p");
      copy.textContent = loadFailed
        ? "Revisa el mensaje de estado e inténtalo de nuevo."
        : "Cuando existan alertas asociadas a movimientos, aparecerán aquí. La generación de escenarios de demostración se implementa en la siguiente etapa.";
      empty.append(title, copy);
      list.append(empty);
      return;
    }

    for (const alert of alerts) {
      const card = document.createElement("article");
      card.className = "alert-card";
      card.dataset.alertId = alert.id;

      const heading = document.createElement("div");
      heading.className = "alert-card-heading";
      const identity = document.createElement("div");
      const title = document.createElement("h2");
      title.textContent = alert.kind;
      const subtitle = document.createElement("p");
      subtitle.className = "detail-muted";
      subtitle.textContent = `Creada ${dateLabel(alert.createdAtMs)}`;
      identity.append(title, subtitle);
      const badge = document.createElement("span");
      badge.className = `record-badge alert-status-${alert.status}`;
      badge.textContent = alertStatusLabel(alert.status);
      heading.append(identity, badge);

      const metadata = document.createElement("dl");
      metadata.className = "detail-list alert-metadata";
      appendDetailRow(metadata, "Unidad", `${alert.handlingUnitLabel} · ${alert.handlingUnitId}`);
      appendDetailRow(metadata, "Movimiento", `${eventKindLabel(alert.eventKind)} · ${dateLabel(alert.occurredAtMs)}`);
      appendDetailRow(metadata, "Evento", alert.movementEventId);
      if (alert.originLocationCode) appendDetailRow(metadata, "Origen", alert.originLocationCode);
      if (alert.destinationLocationCode) appendDetailRow(metadata, "Destino", alert.destinationLocationCode);

      const actions = document.createElement("div");
      actions.className = "alert-card-actions";
      const openButton = document.createElement("button");
      openButton.type = "button";
      openButton.className = "secondary-button";
      openButton.dataset.alertAction = "open-event";
      openButton.textContent = "Abrir movimiento y cronología";
      openButton.setAttribute(
        "aria-label",
        `Abrir movimiento ${eventKindLabel(alert.eventKind)} asociado a la alerta ${alert.kind} de la unidad ${alert.handlingUnitLabel}`,
      );
      openButton.addEventListener("click", () => {
        window.dispatchEvent(new CustomEvent("ariadna:open-unit", {
          detail: {
            handlingUnitId: alert.handlingUnitId,
            movementEventId: alert.movementEventId,
          },
        }));
      });
      actions.append(openButton);

      if (alert.status === "open") {
        const reviewButton = document.createElement("button");
        reviewButton.type = "button";
        reviewButton.className = "primary-button";
        reviewButton.dataset.alertAction = "review";
        reviewButton.textContent = "Marcar revisada";
        reviewButton.addEventListener("click", () => void markReviewed(alert, reviewButton));
        actions.append(reviewButton);
      }

      card.append(heading, metadata, actions);
      list.append(card);
    }
  }

  function appendDetailRow(container: HTMLDListElement, label: string, value: string): void {
    const row = document.createElement("div");
    row.className = "detail-row";
    const term = document.createElement("dt");
    term.className = "detail-label";
    term.textContent = label;
    const description = document.createElement("dd");
    description.className = "detail-value";
    description.textContent = value;
    row.append(term, description);
    container.append(row);
  }

  async function markReviewed(alert: AlertDto, button: HTMLButtonElement): Promise<void> {
    button.disabled = true;
    setStatus(`Marcando como revisada la alerta “${alert.kind}”…`);
    try {
      const updated = await inventoryIpc.markAlertReviewed(alert.id);
      alerts = alerts.map((item) => item.id === updated.id ? updated : item);
      render();
      setStatus(`Alerta “${updated.kind}” marcada como revisada.`, "success");
      const card = [...list.querySelectorAll<HTMLElement>("[data-alert-id]")]
        .find((item) => item.dataset.alertId === alert.id);
      const focusTarget = card?.querySelector<HTMLButtonElement>("[data-alert-action='open-event']")
        ?? refreshButton;
      focusTarget.focus();
    } catch (error: unknown) {
      setStatus(messageFor(error), "error");
      button.disabled = false;
      button.focus();
    }
  }

  async function refresh(): Promise<void> {
    const currentRequest = ++requestNumber;
    refreshButton.disabled = true;
    setStatus("Consultando alertas…");
    try {
      const nextAlerts = await inventoryIpc.listAlerts();
      if (currentRequest !== requestNumber) return;
      alerts = nextAlerts;
      loadFailed = false;
      render();
      setStatus(alerts.length
        ? `${alerts.length} ${alerts.length === 1 ? "alerta cargada" : "alertas cargadas"}.`
        : "No hay alertas persistidas para mostrar.");
    } catch (error: unknown) {
      if (currentRequest !== requestNumber) return;
      alerts = [];
      loadFailed = true;
      render();
      setStatus(messageFor(error), "error");
    } finally {
      if (currentRequest === requestNumber) refreshButton.disabled = false;
    }
  }

  refreshButton.addEventListener("click", () => void refresh());
  return { refresh };
}
