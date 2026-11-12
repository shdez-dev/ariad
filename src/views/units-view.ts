import {
  AriadnaIpcError,
  inventoryIpc,
  type HandlingUnitDto,
  type LocationDto,
  type MovementHistoryEntryDto,
} from "../lib/ipc";
import { getLocationPath } from "../lib/catalog-model";
import { findHistoryEntry } from "../lib/simulator-model";

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`No se encontró el elemento requerido ${selector}.`);
  return element;
}

function messageFor(error: unknown): string {
  if (error instanceof AriadnaIpcError) return error.message;
  return error instanceof Error ? error.message : "No se pudo completar la consulta.";
}

function unitStatusLabel(status: HandlingUnitDto["status"]): string {
  const labels: Record<HandlingUnitDto["status"], string> = {
    received: "Recibido",
    stored: "Almacenado",
    dispatched: "Despachado",
  };
  return labels[status];
}

function movementKindLabel(kind: string): string {
  const labels: Record<string, string> = {
    received: "Recepción",
    stored: "Asignación de posición",
    moved: "Traslado",
    dispatched: "Despacho",
    correction: "Corrección",
  };
  return labels[kind] ?? kind;
}

function movementResultLabel(result: string): string {
  const labels: Record<string, string> = {
    accepted: "Aceptado",
    duplicate: "Duplicado",
    rejected: "Rechazado",
  };
  return labels[result] ?? result;
}

function dateLabel(timestamp: number): string {
  return new Intl.DateTimeFormat("es-CL", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(timestamp);
}

export function mountUnitsView(): {
  refresh: () => Promise<void>;
  openByIdentifier: (identifier: string, recordId?: string) => Promise<void>;
} {
  const rows = requiredElement<HTMLTableSectionElement>("#units-table-body");
  const status = requiredElement<HTMLElement>("#units-status");
  const filter = requiredElement<HTMLInputElement>("#units-filter");
  const refreshButton = requiredElement<HTMLButtonElement>("#units-refresh");
  const detail = requiredElement<HTMLElement>("#unit-details");
  const detailAnnouncement = requiredElement<HTMLElement>("#unit-detail-announcement");

  let units: HandlingUnitDto[] = [];
  let locations: LocationDto[] = [];
  let selectedUnitId: string | null = null;
  let selectedRecordId: string | null = null;
  let requestNumber = 0;
  let refreshRequestNumber = 0;
  let loadFailed = false;

  function setStatus(text: string, kind: "success" | "error" | "neutral" = "neutral"): void {
    status.textContent = text;
    status.dataset.kind = kind;
  }

  function matchesFilter(unit: HandlingUnitDto, query: string): boolean {
    const term = query.trim().toLocaleLowerCase("es");
    if (!term) return true;
    const location = locations.find((item) => item.id === unit.currentLocationId);
    const searchable = [
      unit.id,
      unit.label,
      unitStatusLabel(unit.status),
      location?.code ?? "",
      location?.name ?? "",
      ...unit.lines.map((line) => line.sku),
    ].join(" ").toLocaleLowerCase("es");
    return searchable.includes(term);
  }

  function renderRows(): void {
    rows.replaceChildren();
    const filtered = units.filter((unit) => matchesFilter(unit, filter.value));
    if (!filtered.length) {
      const row = document.createElement("tr");
      const cell = document.createElement("td");
      cell.colSpan = 5;
      cell.className = "table-empty";
      cell.textContent = loadFailed
        ? "No se pudo cargar la lista de unidades."
        : units.length === 0
          ? "Aún no hay unidades. Se crearán al registrar una recepción en el siguiente flujo operativo."
          : "No hay unidades que coincidan con el filtro.";
      row.append(cell);
      rows.append(row);
      return;
    }

    for (const unit of filtered) {
      const row = document.createElement("tr");
      const label = document.createElement("td");
      const identity = document.createElement("div");
      identity.className = "location-identity";
      const strong = document.createElement("strong");
      strong.className = "table-primary-text";
      strong.textContent = unit.label;
      const id = document.createElement("span");
      id.className = "table-secondary-text";
      id.textContent = unit.id;
      identity.append(strong, id);
      label.append(identity);

      const unitState = document.createElement("td");
      const badge = document.createElement("span");
      badge.className = `record-badge unit-${unit.status}`;
      badge.textContent = unitStatusLabel(unit.status);
      unitState.append(badge);

      const locationCell = document.createElement("td");
      const location = locations.find((item) => item.id === unit.currentLocationId);
      locationCell.textContent = unit.currentLocationId
        ? location
          ? getLocationPath(location, locations)
          : "Ubicación no disponible"
        : "Sin ubicación";

      const content = document.createElement("td");
      content.textContent = `${unit.lines.length} ${unit.lines.length === 1 ? "línea" : "líneas"}`;

      const actions = document.createElement("td");
      actions.className = "table-actions";
      const open = document.createElement("button");
      open.type = "button";
      open.className = "text-button";
      open.textContent = "Ver detalle";
      open.setAttribute("aria-label", `Ver detalle del pallet ${unit.label}`);
      open.addEventListener("click", () => void openDetails(unit));
      actions.append(open);
      row.append(label, unitState, locationCell, content, actions);
      rows.append(row);
    }
  }

  function renderDetail(
    unit: HandlingUnitDto,
    history: MovementHistoryEntryDto[] | null,
    historyError?: string,
  ): void {
    detail.replaceChildren();
    const header = document.createElement("div");
    header.className = "detail-header";
    const kicker = document.createElement("p");
    kicker.className = "detail-kicker";
    kicker.textContent = "DETALLE DE UNIDAD";
    const title = document.createElement("h2");
    title.className = "detail-title";
    title.tabIndex = -1;
    title.textContent = unit.label;
    const identity = document.createElement("p");
    identity.className = "detail-name";
    identity.textContent = unit.id;
    header.append(kicker, title, identity);
    detail.append(header);

    const metadata = document.createElement("dl");
    metadata.className = "detail-list";
    const entries: Array<[string, string]> = [
      ["Estado", unitStatusLabel(unit.status)],
      ["Ubicación", unit.currentLocationId
        ? getLocationPath(locations.find((item) => item.id === unit.currentLocationId), locations)
        : "Sin ubicación"],
    ];
    for (const [label, value] of entries) {
      const row = document.createElement("div");
      row.className = "detail-row";
      const term = document.createElement("dt");
      term.className = "detail-label";
      term.textContent = label;
      const description = document.createElement("dd");
      description.className = "detail-value";
      description.textContent = value;
      row.append(term, description);
      metadata.append(row);
    }
    detail.append(metadata);

    const linesTitle = document.createElement("h3");
    linesTitle.className = "detail-section-title unit-lines-title";
    linesTitle.textContent = "Productos del pallet";
    detail.append(linesTitle);
    if (unit.lines.length === 0) {
      const empty = document.createElement("p");
      empty.className = "detail-muted";
      empty.textContent = "Esta unidad no contiene líneas de producto.";
      detail.append(empty);
    } else {
      const list = document.createElement("ul");
      list.className = "product-line-list";
      for (const line of unit.lines) {
        const item = document.createElement("li");
        item.className = "product-line";
        const copy = document.createElement("span");
        copy.className = "product-line-copy";
        const sku = document.createElement("strong");
        sku.className = "product-sku";
        sku.textContent = line.sku;
        const lot = document.createElement("span");
        lot.className = "product-lot";
        lot.textContent = line.lotCode ? `Lote ${line.lotCode}` : "Sin lote";
        const quantity = document.createElement("span");
        quantity.className = "product-quantity";
        quantity.textContent = `× ${line.quantity}`;
        copy.append(sku, lot);
        item.append(copy, quantity);
        list.append(item);
      }
      detail.append(list);
    }

    const historyTitle = document.createElement("h3");
    historyTitle.className = "detail-section-title movement-history-title";
    historyTitle.textContent = "Cronología de movimientos";
    detail.append(historyTitle);
    if (history === null) {
      const loadingText = document.createElement("p");
      loadingText.className = "detail-muted";
      loadingText.textContent = "Consultando historial persistido…";
      detail.append(loadingText);
    } else if (historyError) {
      const errorText = document.createElement("p");
      errorText.className = "detail-warning";
      errorText.textContent = historyError;
      detail.append(errorText);
    } else if (history.length === 0) {
      const empty = document.createElement("p");
      empty.className = "detail-muted";
      empty.textContent = "Esta unidad aún no tiene movimientos registrados.";
      detail.append(empty);
    } else {
      const timeline = document.createElement("ol");
      timeline.className = "movement-history-list";
      for (const entry of history) {
        const item = document.createElement("li");
        item.className = "movement-history-entry";
        item.dataset.historyEventId = entry.eventId;
        item.dataset.historyRecordId = entry.recordId;
        item.tabIndex = -1;

        const header = document.createElement("div");
        header.className = "movement-history-heading";
        const kind = document.createElement("strong");
        kind.textContent = movementKindLabel(entry.kind);
        const outcome = document.createElement("span");
        outcome.className = `record-badge history-result-${entry.result}`;
        outcome.textContent = movementResultLabel(entry.result);
        header.append(kind, outcome);

        const date = document.createElement("p");
        date.className = "detail-muted";
        date.textContent = `Ocurrido ${dateLabel(entry.occurredAtMs)} · registrado ${dateLabel(entry.recordedAtMs)}`;
        item.append(header, date);

        const movementLocations = document.createElement("p");
        movementLocations.className = "movement-history-locations";
        const origin = entry.originLocationId
          ? locations.find((location) => location.id === entry.originLocationId)
          : undefined;
        const destination = entry.destinationLocationId
          ? locations.find((location) => location.id === entry.destinationLocationId)
          : undefined;
        movementLocations.textContent = entry.originLocationId || entry.destinationLocationId
          ? `${origin ? getLocationPath(origin, locations) : entry.originLocationId ?? "Origen sin ubicación"} → ${destination ? getLocationPath(destination, locations) : entry.destinationLocationId ?? "Sin destino"}`
          : "Sin cambio de ubicación";
        item.append(movementLocations);

        if (entry.rejectionReason) {
          const reason = document.createElement("p");
          reason.className = "detail-warning history-reason";
          reason.textContent = `Motivo: ${entry.rejectionReason}`;
          item.append(reason);
        }
        if (entry.correctsEventId || entry.correctionReason) {
          const correction = document.createElement("p");
          correction.className = "detail-note history-correction";
          correction.textContent = `Corrige ${entry.correctsEventId ?? "un evento"}${entry.correctionReason ? ` · ${entry.correctionReason}` : ""}`;
          item.append(correction);
        }
        const source = document.createElement("p");
        source.className = "movement-history-source";
        source.textContent = entry.source === "simulator"
          ? `Evento ${entry.eventId} · Dato sintético del simulador`
          : `Evento ${entry.eventId} · ${entry.source}`;
        item.append(source);
        timeline.append(item);
      }
      detail.append(timeline);
    }
  }

  async function openDetails(
    unit: HandlingUnitDto,
    focusRecordId?: string,
    transferFocus = false,
  ): Promise<void> {
    selectedUnitId = unit.id;
    selectedRecordId = focusRecordId ?? null;
    const currentRequest = ++requestNumber;
    renderDetail(unit, null);
    detailAnnouncement.textContent = `Consultando detalle del pallet ${unit.label}.`;
    const [latestResult, historyResult] = await Promise.allSettled([
      inventoryIpc.getHandlingUnit(unit.id),
      inventoryIpc.getMovementHistory(unit.id),
    ]);
    if (currentRequest !== requestNumber || selectedUnitId !== unit.id) return;
    if (latestResult.status === "fulfilled" && !latestResult.value) {
      detail.replaceChildren();
      const notFound = document.createElement("p");
      notFound.className = "detail-warning";
      notFound.textContent = `No se encontró la unidad ${unit.label}. Actualiza la lista.`;
      detail.append(notFound);
      setStatus(`No se encontró la unidad ${unit.label}. Actualiza la lista.`, "error");
      detailAnnouncement.textContent = `No se encontró el pallet ${unit.label}.`;
      return;
    }
    const latest = latestResult.status === "fulfilled" ? latestResult.value ?? unit : unit;
    const movementHistory = historyResult.status === "fulfilled" ? historyResult.value : null;
    const historyError = historyResult.status === "rejected" ? messageFor(historyResult.reason) : undefined;
    renderDetail(latest, movementHistory, historyError);
    if (latestResult.status === "rejected") {
      setStatus(messageFor(latestResult.reason), "error");
      detailAnnouncement.textContent = `No se pudo actualizar el detalle del pallet ${unit.label}.`;
    } else if (historyError) {
      setStatus(`No se pudo cargar la cronología: ${historyError}`, "error");
      detailAnnouncement.textContent = `Detalle del pallet ${latest.label} cargado, pero falló el historial.`;
    } else {
      setStatus(`Detalle y cronología de ${latest.label} cargados.`);
      detailAnnouncement.textContent = `Detalle e historial del pallet ${latest.label} cargados.`;
    }
    if (transferFocus) {
      const exactEntry = findHistoryEntry(movementHistory ?? [], focusRecordId);
      const eventTarget = exactEntry
        ? [...detail.querySelectorAll<HTMLElement>("[data-history-record-id]")]
          .find((item) => item.dataset.historyRecordId === exactEntry.recordId)
        : undefined;
      if (focusRecordId && movementHistory && !eventTarget) {
        setStatus(`El registro ${focusRecordId} no aparece en la cronología actual.`, "error");
      }
      const focusTarget = eventTarget ?? detail.querySelector<HTMLElement>(".detail-title");
      focusTarget?.focus();
      eventTarget?.scrollIntoView({ block: "nearest" });
    }
  }

  function renderHistoryOnly(identifier: string, history: MovementHistoryEntryDto[], recordId?: string): void {
    selectedUnitId = null;
    selectedRecordId = recordId ?? null;
    detail.replaceChildren();
    const header = document.createElement("div");
    header.className = "detail-header";
    const kicker = document.createElement("p");
    kicker.className = "detail-kicker";
    kicker.textContent = "REGISTRO SINTÉTICO";
    const title = document.createElement("h2");
    title.className = "detail-title";
    title.tabIndex = -1;
    title.textContent = "Intento sin unidad registrada";
    const identity = document.createElement("p");
    identity.className = "detail-name";
    identity.textContent = identifier;
    header.append(kicker, title, identity);
    const explanation = document.createElement("p");
    explanation.className = "detail-muted";
    explanation.textContent = "No existe una ficha de unidad para esta referencia. Se muestra el historial persistido y se enfoca el registro exacto solicitado.";
    detail.append(header, explanation);

    const exactEntry = findHistoryEntry(history, recordId);
    const historyTitle = document.createElement("h3");
    historyTitle.className = "detail-section-title movement-history-title";
    historyTitle.textContent = exactEntry ? "Registro exacto" : "Historial disponible";
    detail.append(historyTitle);
    if (history.length === 0) {
      const empty = document.createElement("p");
      empty.className = "detail-warning";
      empty.textContent = `No hay registros asociados a ${identifier}.`;
      detail.append(empty);
      title.focus();
      return;
    }

    const timeline = document.createElement("ol");
    timeline.className = "movement-history-list";
    const displayedEntries = exactEntry ? [exactEntry] : history;
    for (const entry of displayedEntries) {
      const item = document.createElement("li");
      item.className = "movement-history-entry";
      item.dataset.historyEventId = entry.eventId;
      item.dataset.historyRecordId = entry.recordId;
      item.tabIndex = -1;
      const heading = document.createElement("div");
      heading.className = "movement-history-heading";
      const kind = document.createElement("strong");
      kind.textContent = movementKindLabel(entry.kind);
      const outcome = document.createElement("span");
      outcome.className = `record-badge history-result-${entry.result}`;
      outcome.textContent = movementResultLabel(entry.result);
      heading.append(kind, outcome);
      const date = document.createElement("p");
      date.className = "detail-muted";
      date.textContent = `Ocurrido ${dateLabel(entry.occurredAtMs)} · registrado ${dateLabel(entry.recordedAtMs)}`;
      const reference = document.createElement("p");
      reference.className = "movement-history-source";
      reference.textContent = `Registro ${entry.recordId} · evento ${entry.eventId} · ${entry.source === "simulator" ? "Dato sintético del simulador" : entry.source}`;
      item.append(heading, date, reference);
      if (entry.rejectionReason) {
        const reason = document.createElement("p");
        reason.className = "detail-warning history-reason";
        reason.textContent = `Motivo: ${entry.rejectionReason}`;
        item.append(reason);
      }
      timeline.append(item);
    }
    detail.append(timeline);
    const focusTarget = exactEntry
      ? [...timeline.querySelectorAll<HTMLElement>("[data-history-record-id]")]
        .find((item) => item.dataset.historyRecordId === exactEntry.recordId)
      : title;
    focusTarget?.focus();
    if (exactEntry) focusTarget?.scrollIntoView({ block: "nearest" });
  }

  async function openByIdentifier(identifier: string, recordId?: string): Promise<void> {
    const currentRequest = ++requestNumber;
    selectedUnitId = null;
    selectedRecordId = recordId ?? null;
    detail.replaceChildren();
    detailAnnouncement.textContent = `Buscando la unidad y el registro ${recordId ?? identifier}.`;
    try {
      const unit = await inventoryIpc.getHandlingUnit(identifier);
      if (currentRequest !== requestNumber) return;
      if (!unit) {
        const history = await inventoryIpc.getMovementHistory(identifier);
        if (currentRequest !== requestNumber) return;
        renderHistoryOnly(identifier, history, recordId);
        if (history.length === 0) {
          setStatus(`No hay historial asociado a ${identifier}.`, "error");
          detailAnnouncement.textContent = `No hay historial asociado a ${identifier}.`;
        } else if (recordId && !findHistoryEntry(history, recordId)) {
          setStatus(`No se encontró el registro ${recordId}; se muestra el historial disponible.`, "error");
          detailAnnouncement.textContent = `No se encontró el registro ${recordId}.`;
        } else {
          setStatus("Unidad no registrada; se muestra el historial persistido.");
          detailAnnouncement.textContent = "Se abrió el registro de historial sin ficha de unidad.";
        }
        return;
      }
      if (!units.some((item) => item.id === unit.id)) units.push(unit);
      renderRows();
      await openDetails(unit, recordId, true);
    } catch (error: unknown) {
      if (currentRequest !== requestNumber) return;
      setStatus(messageFor(error), "error");
      detailAnnouncement.textContent = `No se pudo abrir el registro ${recordId ?? identifier}.`;
      detail.replaceChildren();
      const failure = document.createElement("p");
      failure.className = "detail-warning";
      failure.textContent = messageFor(error);
      detail.append(failure);
    }
  }

  async function refresh(): Promise<void> {
    const currentRefresh = ++refreshRequestNumber;
    requestNumber += 1;
    refreshButton.disabled = true;
    refreshButton.setAttribute("aria-busy", "true");
    detail.setAttribute("aria-busy", "true");
    detail.replaceChildren();
    const loading = document.createElement("p");
    loading.className = "detail-muted";
    loading.textContent = "Actualizando la lista de unidades…";
    detail.append(loading);
    detailAnnouncement.textContent = "Actualizando unidades. El detalle se mostrará cuando termine la consulta.";
    setStatus("Consultando unidades…");
    try {
      const [nextUnits, nextLocations] = await Promise.all([
        inventoryIpc.listHandlingUnits(true),
        inventoryIpc.listLocations(true),
      ]);
      if (currentRefresh !== refreshRequestNumber) return;
      units = nextUnits;
      locations = nextLocations;
      loadFailed = false;
      renderRows();
      setStatus(`${units.length} ${units.length === 1 ? "unidad cargada" : "unidades cargadas"}.`);
      const selected = selectedUnitId ? units.find((item) => item.id === selectedUnitId) : undefined;
      if (selected) await openDetails(selected, selectedRecordId ?? undefined);
      else if (selectedUnitId) {
        selectedUnitId = null;
        detail.replaceChildren();
        const message = "La unidad seleccionada ya no está disponible en la lista.";
        const unavailable = document.createElement("p");
        unavailable.className = "detail-warning";
        unavailable.textContent = message;
        detail.append(unavailable);
        setStatus(message, "error");
        detailAnnouncement.textContent = message;
      } else {
        const empty = document.createElement("p");
        empty.className = "detail-muted";
        empty.textContent = "Selecciona una unidad para consultar su ubicación e historial.";
        detail.replaceChildren(empty);
        detailAnnouncement.textContent = "Lista de unidades actualizada.";
      }
    } catch (error: unknown) {
      if (currentRefresh !== refreshRequestNumber) return;
      units = [];
      locations = [];
      loadFailed = true;
      renderRows();
      detail.replaceChildren();
      const message = messageFor(error);
      const failure = document.createElement("p");
      failure.className = "detail-warning";
      failure.textContent = `No se pudo actualizar el detalle: ${message}`;
      detail.append(failure);
      setStatus(message, "error");
      detailAnnouncement.textContent = `No se pudo cargar la lista de unidades: ${message}`;
    } finally {
      if (currentRefresh === refreshRequestNumber) {
        refreshButton.disabled = false;
        refreshButton.removeAttribute("aria-busy");
        detail.removeAttribute("aria-busy");
      }
    }
  }

  filter.addEventListener("input", renderRows);
  refreshButton.addEventListener("click", () => void refresh());
  return { refresh, openByIdentifier };
}

