import { AriadnaIpcError, inventoryIpc, type AlertDto } from "../lib/ipc";
import { isSimulatedSource } from "../lib/simulator-model";

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

export function mountAlertsView(): {
  refresh: () => Promise<void>;
  focusAlert: (alertId: string) => void;
} {
  const status = requiredElement<HTMLElement>("#alerts-status");
  const list = requiredElement<HTMLElement>("#alerts-list");
  const refreshButton = requiredElement<HTMLButtonElement>("#alerts-refresh");
  let alerts: AlertDto[] = [];
  let requestNumber = 0;
  let loadFailed = false;
  let expandedRecordAlertId: string | null = null;

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
        : "Cuando una anomalía requiera revisión, aparecerá aquí. El Simulador permite crear escenarios sintéticos para demostración.";
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
      if (isSimulatedSource(alert.source)) {
        const simulated = document.createElement("span");
        simulated.className = "simulation-source-badge";
        simulated.textContent = "Dato sintético";
        heading.append(simulated);
      }

      const metadata = document.createElement("dl");
      metadata.className = "detail-list alert-metadata";
      appendDetailRow(metadata, "Unidad", alert.handlingUnitLabel
        ? `${alert.handlingUnitLabel} · ${alert.handlingUnitId}`
        : `Sin ficha registrada · ${alert.handlingUnitId ?? "sin identificador"}`);
      appendDetailRow(metadata, "Movimiento", `${eventKindLabel(alert.eventKind)} · ${dateLabel(alert.occurredAtMs)}`);
      appendDetailRow(metadata, "Registro exacto", alert.recordId);
      appendDetailRow(metadata, "Evento", alert.movementEventId ?? alert.eventId);
      if (alert.movementAttemptId !== undefined && alert.movementAttemptId !== null) {
        appendDetailRow(metadata, "Intento", String(alert.movementAttemptId));
      }
      if (alert.targetKind) appendDetailRow(metadata, "Tipo de registro", alert.targetKind === "movement_attempt" ? "Intento de movimiento" : alert.targetKind === "movement_event" ? "Movimiento aceptado" : alert.targetKind);
      appendDetailRow(metadata, "Resultado", alert.eventResult);
      if (alert.eventRejectionReason) appendDetailRow(metadata, "Motivo", alert.eventRejectionReason);
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
        `Abrir movimiento ${eventKindLabel(alert.eventKind)} asociado a la alerta ${alert.kind} de ${alert.handlingUnitLabel ?? "un intento sin unidad registrada"}`,
      );
      openButton.addEventListener("click", () => {
        if (!alert.handlingUnitId) {
          expandedRecordAlertId = expandedRecordAlertId === alert.id ? null : alert.id;
          render();
          const preview = [...list.querySelectorAll<HTMLElement>("[data-alert-record-preview]")]
            .find((item) => item.dataset.alertRecordPreview === alert.id)
            ?.querySelector<HTMLElement>("h3");
          preview?.focus();
          return;
        }
        window.dispatchEvent(new CustomEvent("ariadna:open-unit", {
          detail: {
            handlingUnitId: alert.handlingUnitId,
            recordId: alert.recordId || alert.movementEventId || undefined,
            movementEventId: alert.movementEventId ?? undefined,
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
      if (expandedRecordAlertId === alert.id) {
        const preview = document.createElement("section");
        preview.className = "alert-record-preview";
        preview.dataset.alertRecordPreview = alert.id;
        preview.setAttribute("aria-labelledby", `alert-record-title-${alert.id}`);
        const previewTitle = document.createElement("h3");
        previewTitle.id = `alert-record-title-${alert.id}`;
        previewTitle.tabIndex = -1;
        previewTitle.textContent = "Registro exacto sin ficha de unidad";
        const record = document.createElement("p");
        record.textContent = `Identificador: ${alert.recordId}`;
        const target = document.createElement("p");
        target.textContent = alert.movementAttemptId !== undefined && alert.movementAttemptId !== null
          ? `Intento ${alert.movementAttemptId} · evento ${alert.eventId} · ${alert.targetKind ?? "anomalía"}`
          : `Evento ${alert.movementEventId ?? alert.eventId} · ${alert.targetKind ?? "registro"}`;
        const result = document.createElement("p");
        result.textContent = `${alert.eventResult}${alert.eventRejectionReason ? ` · ${alert.eventRejectionReason}` : ""}`;
        preview.append(previewTitle, record, target, result);
        card.append(preview);
      }
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

  function focusAlert(alertId: string): void {
    const card = [...list.querySelectorAll<HTMLElement>("[data-alert-id]")]
      .find((item) => item.dataset.alertId === alertId);
    if (!card) {
      setStatus("La alerta solicitada ya no está en la lista. Actualiza las alertas.", "error");
      requiredElement<HTMLElement>("#alerts-title").focus();
      return;
    }
    const target = card.querySelector<HTMLButtonElement>("[data-alert-action='open-event']")
      ?? card.querySelector<HTMLButtonElement>("[data-alert-action='review']");
    target?.focus();
    card.scrollIntoView({ block: "nearest" });
  }

  refreshButton.addEventListener("click", () => void refresh());
  return { refresh, focusAlert };
}
