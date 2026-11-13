import {
  AriadnaIpcError,
  inventoryIpc,
  newEventMetadata,
  type HandlingUnitDto,
  type LocationDto,
  type MovementHistoryEntryDto,
  type MovementWriteDto,
  type ProductDto,
  type ReceiveLineInput,
} from "../lib/ipc.ts";
import {
  activePositions,
  getLocationPath,
  storedUnits,
  unitsAwaitingStorage,
} from "../lib/catalog-model.ts";
import {
  createLatestRefreshCoordinator,
  type LatestRefreshResult,
} from "../lib/latest-refresh-coordinator.ts";

const MAX_RECEIVE_LINES = 100;
const MAX_LOT_CODE_LENGTH = 120;

type OperationMode = "receive" | "store" | "move" | "dispatch";
type RefreshResult = LatestRefreshResult;

type PendingIntent = {
  fingerprint: string;
  eventId: string;
  occurredAtMs: number;
  handlingUnitId?: string;
};

type RetryRequest = {
  mode: OperationMode;
  fingerprint: string;
  unitId: string;
  actionLabel: string;
  send: (intent: PendingIntent) => Promise<MovementWriteDto>;
};

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`No se encontró el elemento requerido ${selector}.`);
  return element;
}

function messageFor(error: unknown): string {
  if (error instanceof AriadnaIpcError) return error.message;
  return error instanceof Error ? error.message : "No se pudo completar la operación.";
}

function option(value: string, label: string): HTMLOptionElement {
  const item = document.createElement("option");
  item.value = value;
  item.textContent = label;
  return item;
}

function replaceOptions(
  select: HTMLSelectElement,
  placeholder: string,
  options: Array<{ value: string; label: string }>,
): void {
  const selected = select.value;
  select.replaceChildren(option("", placeholder));
  for (const item of options) select.append(option(item.value, item.label));
  select.value = options.some((item) => item.value === selected) ? selected : "";
}

function movementKindLabel(kind: string): string {
  const labels: Record<string, string> = {
    received: "Recepción",
    stored: "Asignación de posición",
    moved: "Traslado",
    dispatched: "Despacho",
    correction: "Corrección",
  };
  return labels[kind] ?? "Movimiento";
}

function movementResultLabel(result: string): string {
  const labels: Record<string, string> = {
    accepted: "Aceptado",
    duplicate: "Duplicado, sin repetir efectos",
    rejected: "Rechazado",
  };
  return labels[result] ?? result;
}

export function mountOperationsView(): { refresh: () => Promise<void> } {
  const status = requiredElement<HTMLElement>("#operations-status");
  const refreshButton = requiredElement<HTMLButtonElement>("#operations-refresh");
  const tabs = [...document.querySelectorAll<HTMLButtonElement>("[data-operation-tab]")];
  const panels = [...document.querySelectorAll<HTMLElement>("[data-operation-panel]")];
  const resultStatus = requiredElement<HTMLElement>("#operation-result-status");
  const availabilityNote = requiredElement<HTMLElement>("#operation-availability");
  const retryButton = requiredElement<HTMLButtonElement>("#operation-retry");
  const historyDetails = requiredElement<HTMLDListElement>("#operation-history-details");
  const historyNote = requiredElement<HTMLElement>("#operation-history-note");

  const receiveForm = requiredElement<HTMLFormElement>("#receive-operation-form");
  const receiveFields = requiredElement<HTMLFieldSetElement>("#receive-operation-fields");
  const receiveLabel = requiredElement<HTMLInputElement>("#receive-label");
  const receiveLines = requiredElement<HTMLDivElement>("#receive-lines");
  const addLineButton = requiredElement<HTMLButtonElement>("#receive-line-add");
  const receiveSubmit = requiredElement<HTMLButtonElement>("#receive-submit");

  const storeForm = requiredElement<HTMLFormElement>("#store-operation-form");
  const storeFields = requiredElement<HTMLFieldSetElement>("#store-operation-fields");
  const storeUnitSelect = requiredElement<HTMLSelectElement>("#store-unit");
  const storeUnitSummary = requiredElement<HTMLElement>("#store-unit-summary");
  const storeDestination = requiredElement<HTMLSelectElement>("#store-destination");
  const storeSubmit = requiredElement<HTMLButtonElement>("#store-submit");

  const moveForm = requiredElement<HTMLFormElement>("#move-operation-form");
  const moveFields = requiredElement<HTMLFieldSetElement>("#move-operation-fields");
  const moveUnitSelect = requiredElement<HTMLSelectElement>("#move-unit");
  const moveOriginSummary = requiredElement<HTMLElement>("#move-origin-summary");
  const moveDestination = requiredElement<HTMLSelectElement>("#move-destination");
  const moveSubmit = requiredElement<HTMLButtonElement>("#move-submit");

  const dispatchForm = requiredElement<HTMLFormElement>("#dispatch-operation-form");
  const dispatchFields = requiredElement<HTMLFieldSetElement>("#dispatch-operation-fields");
  const dispatchUnitSelect = requiredElement<HTMLSelectElement>("#dispatch-unit");
  const dispatchOriginSummary = requiredElement<HTMLElement>("#dispatch-origin-summary");
  const dispatchSubmit = requiredElement<HTMLButtonElement>("#dispatch-submit");

  const forms: Record<OperationMode, HTMLFormElement> = {
    receive: receiveForm,
    store: storeForm,
    move: moveForm,
    dispatch: dispatchForm,
  };
  const fieldsets: Record<OperationMode, HTMLFieldSetElement> = {
    receive: receiveFields,
    store: storeFields,
    move: moveFields,
    dispatch: dispatchFields,
  };
  const submits: Record<OperationMode, HTMLButtonElement> = {
    receive: receiveSubmit,
    store: storeSubmit,
    move: moveSubmit,
    dispatch: dispatchSubmit,
  };

  let products: ProductDto[] = [];
  let locations: LocationDto[] = [];
  let units: HandlingUnitDto[] = [];
  let loadFailed = false;
  let busy = false;
  let uncertainRetry: RetryRequest | null = null;
  let activeMode: OperationMode = "receive";
  const refreshCoordinator = createLatestRefreshCoordinator();
  const pendingIntents = new Map<OperationMode, PendingIntent>();

  function setStatus(text: string, kind: "success" | "error" | "neutral" = "neutral"): void {
    status.textContent = text;
    status.dataset.kind = kind;
  }

  function setResult(text: string, kind: "success" | "error" | "neutral"): void {
    resultStatus.textContent = text;
    resultStatus.dataset.kind = kind;
    historyDetails.replaceChildren();
    historyDetails.hidden = true;
    retryButton.hidden = true;
    retryButton.disabled = true;
    historyNote.textContent = "El resumen se consulta por IPC después de confirmar. La cronología completa se añadirá en la vista de historial.";
  }

  function setMode(mode: OperationMode): void {
    activeMode = mode;
    for (const tab of tabs) {
      const selected = tab.dataset.operationTab === mode;
      tab.classList.toggle("is-current", selected);
      tab.setAttribute("aria-pressed", String(selected));
      tab.disabled = busy;
    }
    for (const panel of panels) panel.hidden = panel.dataset.operationPanel !== mode;
    updateAvailability();
  }

  function unitLabel(unit: HandlingUnitDto, showLocation = false): string {
    const location = unit.currentLocationId
      ? getLocationPath(locations.find((item) => item.id === unit.currentLocationId), locations)
      : "Sin ubicación";
    const lineCount = `${unit.lines.length} ${unit.lines.length === 1 ? "línea" : "líneas"}`;
    return `${unit.label} · ${lineCount}${showLocation ? ` · ${location}` : ""}`;
  }

  function occupiedPositionIds(): Set<string> {
    return new Set(storedUnits(units).flatMap((unit) =>
      unit.currentLocationId ? [unit.currentLocationId] : [],
    ));
  }

  function appendReceiveLine(): void {
    if (receiveLines.children.length >= MAX_RECEIVE_LINES) return;
    const row = document.createElement("div");
    row.className = "receive-line-row";

    const productLabel = document.createElement("label");
    productLabel.className = "form-field";
    productLabel.textContent = "Producto activo";
    const productSelect = document.createElement("select");
    productSelect.required = true;
    productSelect.dataset.receiveSku = "true";
    productLabel.append(productSelect);

    const quantityLabel = document.createElement("label");
    quantityLabel.className = "form-field";
    quantityLabel.textContent = "Cantidad";
    const quantityInput = document.createElement("input");
    quantityInput.type = "number";
    quantityInput.min = "1";
    quantityInput.max = "4294967295";
    quantityInput.step = "1";
    quantityInput.value = "1";
    quantityInput.required = true;
    quantityInput.dataset.receiveQuantity = "true";
    quantityLabel.append(quantityInput);

    const lotLabel = document.createElement("label");
    lotLabel.className = "form-field";
    lotLabel.textContent = "Lote, opcional";
    const lotInput = document.createElement("input");
    lotInput.type = "text";
    lotInput.maxLength = MAX_LOT_CODE_LENGTH;
    lotInput.autocomplete = "off";
    lotInput.dataset.receiveLot = "true";
    lotLabel.append(lotInput);

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "receive-line-remove";
    remove.textContent = "Quitar";
    remove.setAttribute("aria-label", "Quitar línea de producto");
    remove.addEventListener("click", () => {
      if (receiveLines.children.length <= 1) return;
      row.remove();
      updateAvailability();
    });

    row.append(productLabel, quantityLabel, lotLabel, remove);
    receiveLines.append(row);
    refreshProductOptions();
    updateAvailability();
  }

  function refreshProductOptions(): void {
    for (const select of receiveLines.querySelectorAll<HTMLSelectElement>("[data-receive-sku]")) {
      const productOptions = products.map((product) => ({
        value: product.sku,
        label: `${product.sku} · ${product.description}`,
      }));
      replaceOptions(select, "Selecciona un producto", productOptions);
    }
  }

  function selectedUnit(select: HTMLSelectElement): HandlingUnitDto | undefined {
    return units.find((unit) => unit.id === select.value);
  }

  function activeFreePositions(excludeLocationId?: string | null): LocationDto[] {
    const occupied = occupiedPositionIds();
    return activePositions(locations).filter((location) =>
      !occupied.has(location.id) && location.id !== excludeLocationId,
    );
  }

  function refreshSelectors(): void {
    const received = unitsAwaitingStorage(units);
    const stored = storedUnits(units);
    replaceOptions(
      storeUnitSelect,
      "Selecciona una unidad recibida",
      received.map((unit) => ({ value: unit.id, label: unitLabel(unit) })),
    );
    replaceOptions(
      moveUnitSelect,
      "Selecciona una unidad almacenada",
      stored.map((unit) => ({ value: unit.id, label: unitLabel(unit, true) })),
    );
    replaceOptions(
      dispatchUnitSelect,
      "Selecciona una unidad almacenada",
      stored.map((unit) => ({ value: unit.id, label: unitLabel(unit, true) })),
    );
    const storePositions = activeFreePositions();
    replaceOptions(
      storeDestination,
      "Selecciona una posición libre",
      storePositions.map((location) => ({
        value: location.id,
        label: `${getLocationPath(location, locations)} · ${location.name}`,
      })),
    );
    const movingUnit = selectedUnit(moveUnitSelect);
    const movePositions = activeFreePositions(movingUnit?.currentLocationId);
    replaceOptions(
      moveDestination,
      "Selecciona una posición libre distinta al origen",
      movePositions.map((location) => ({
        value: location.id,
        label: `${getLocationPath(location, locations)} · ${location.name}`,
      })),
    );
    refreshProductOptions();

    const storingUnit = selectedUnit(storeUnitSelect);
    storeUnitSummary.textContent = storingUnit
      ? `${storingUnit.label}: ${storingUnit.lines.map((line) => `${line.sku} × ${line.quantity}`).join(", ")}. La asignación registra el primer destino.`
      : received.length
        ? `${received.length} ${received.length === 1 ? "unidad recibida" : "unidades recibidas"} pendiente${received.length === 1 ? "" : "s"} de ubicación.`
        : "No hay unidades recibidas pendientes de ubicación.";

    moveOriginSummary.textContent = movingUnit?.currentLocationId
      ? `Origen registrado: ${getLocationPath(locations.find((item) => item.id === movingUnit.currentLocationId), locations)}. El origen se valida de nuevo en Rust.`
      : stored.length
        ? "Selecciona una unidad para consultar su origen guardado."
        : "No hay unidades almacenadas para trasladar.";
    const dispatchingUnit = selectedUnit(dispatchUnitSelect);
    dispatchOriginSummary.textContent = dispatchingUnit?.currentLocationId
      ? `Se despachará ${dispatchingUnit.label} desde ${getLocationPath(locations.find((item) => item.id === dispatchingUnit.currentLocationId), locations)}.`
      : stored.length
        ? "Selecciona una unidad para consultar su ubicación guardada."
        : "No hay unidades almacenadas para despachar.";
    updateAvailability();
  }

  function updateAvailability(): void {
    const received = unitsAwaitingStorage(units);
    const stored = storedUnits(units);
    const freePositions = activeFreePositions();
    const movingUnit = selectedUnit(moveUnitSelect);
    const moveTargets = activeFreePositions(movingUnit?.currentLocationId);
    const canReceive = !loadFailed && products.length > 0;
    fieldsets.receive.disabled = busy || uncertainRetry !== null || !canReceive;
    fieldsets.store.disabled = busy || uncertainRetry !== null || loadFailed || received.length === 0 || freePositions.length === 0;
    fieldsets.move.disabled = busy || uncertainRetry !== null || loadFailed || stored.length === 0 || moveTargets.length === 0;
    fieldsets.dispatch.disabled = busy || uncertainRetry !== null || loadFailed || stored.length === 0;
    refreshButton.disabled = busy;
    const lineRows = [...receiveLines.querySelectorAll<HTMLElement>(".receive-line-row")];
    addLineButton.disabled = busy || products.length === 0 || lineRows.length >= MAX_RECEIVE_LINES;
    addLineButton.title = lineRows.length >= MAX_RECEIVE_LINES
      ? "Una unidad puede contener hasta 100 líneas de producto."
      : "Agregar otra línea de producto";
    for (const row of lineRows) {
      const remove = row.querySelector<HTMLButtonElement>(".receive-line-remove");
      if (remove) remove.disabled = busy || lineRows.length <= 1;
    }
    for (const tab of tabs) tab.disabled = busy;
    retryButton.hidden = uncertainRetry === null;
    retryButton.disabled = busy || uncertainRetry === null;

    if (busy) {
      availabilityNote.textContent = "La operación está en curso. Espera la respuesta de Rust antes de cambiar de flujo.";
    } else if (uncertainRetry) {
      availabilityNote.textContent = "Hay una operación cuyo resultado no se pudo confirmar. Reintenta la misma solicitud con el botón del panel de resultado; sus datos e identificador se conservan para evitar duplicarla.";
    } else if (loadFailed) {
      availabilityNote.textContent = "No se pudieron cargar los datos del inventario. Actualiza antes de iniciar otra operación.";
    } else if (activeMode === "receive" && products.length === 0) {
      availabilityNote.textContent = "No hay productos activos para recibir. Crea o activa un producto en la sección Productos.";
    } else if (activeMode === "store" && received.length === 0) {
      availabilityNote.textContent = "No hay unidades recibidas pendientes de ubicación. Registra una recepción primero.";
    } else if (activeMode === "store" && freePositions.length === 0) {
      availabilityNote.textContent = "No hay posiciones activas libres. Configura una posición nueva o libera una mediante un traslado o despacho.";
    } else if (activeMode === "move" && stored.length === 0) {
      availabilityNote.textContent = "No hay unidades almacenadas para trasladar.";
    } else if (activeMode === "move" && moveTargets.length === 0) {
      availabilityNote.textContent = "No hay otra posición activa libre para la unidad seleccionada.";
    } else if (activeMode === "dispatch" && stored.length === 0) {
      availabilityNote.textContent = "No hay unidades almacenadas para despachar.";
    } else {
      availabilityNote.textContent = "Los selectores se alimentan de datos guardados. Rust vuelve a validar cada operación antes de persistir.";
    }
  }

  function renderHistoryEntry(entry: MovementHistoryEntryDto, write: MovementWriteDto): void {
    historyDetails.replaceChildren();
    const origin = entry.originLocationId
      ? getLocationPath(locations.find((item) => item.id === entry.originLocationId), locations)
      : "Sin ubicación anterior";
    const destination = entry.destinationLocationId
      ? getLocationPath(locations.find((item) => item.id === entry.destinationLocationId), locations)
      : "Sin ubicación de destino";
    const recordedAt = new Date(entry.occurredAtMs).toLocaleString("es-CL");
    const values: Array<[string, string]> = [
      ["Movimiento", movementKindLabel(entry.kind)],
      ["Resultado", movementResultLabel(entry.result)],
      ["Origen", origin],
      ["Destino", destination],
      ["Fecha", Number.isNaN(entry.occurredAtMs) ? "No disponible" : recordedAt],
      ["Evento", write.eventId],
    ];
    for (const [label, value] of values) {
      const row = document.createElement("div");
      row.className = "detail-row";
      const term = document.createElement("dt");
      term.className = "detail-label";
      term.textContent = label;
      const description = document.createElement("dd");
      description.className = "detail-value";
      description.textContent = value;
      row.append(term, description);
      historyDetails.append(row);
    }
    historyDetails.hidden = false;
    historyNote.textContent = "Resumen del registro consultado por IPC. La cronología completa permanece en ARD-14.";
  }

  function intentFor(mode: OperationMode, fingerprint: string, isReceipt = false): PendingIntent {
    const existing = pendingIntents.get(mode);
    if (existing?.fingerprint === fingerprint) return existing;
    const metadata = newEventMetadata();
    const intent: PendingIntent = {
      fingerprint,
      eventId: metadata.eventId,
      occurredAtMs: metadata.occurredAtMs,
      ...(isReceipt ? { handlingUnitId: globalThis.crypto.randomUUID() } : {}),
    };
    pendingIntents.set(mode, intent);
    return intent;
  }

  function shouldKeepIntent(error: unknown): boolean {
    return !(error instanceof AriadnaIpcError) || error.code === "storage";
  }

  async function refreshData(announce = true): Promise<RefreshResult> {
    refreshButton.disabled = true;
    if (announce) setStatus("Consultando productos, unidades y ubicaciones…");
    return refreshCoordinator.run(
      () => Promise.all([
        inventoryIpc.listProducts(false),
        inventoryIpc.listHandlingUnits(true),
        inventoryIpc.listLocations(true),
      ]),
      {
        onLoaded: ([nextProducts, nextUnits, nextLocations]) => {
          products = nextProducts.filter((product) => product.active);
          units = nextUnits;
          locations = nextLocations;
          loadFailed = false;
          refreshSelectors();
          if (announce) {
            setStatus(`${products.length} productos activos, ${units.length} unidades y ${locations.length} ubicaciones consultados.`);
          }
        },
        onError: (error) => {
          loadFailed = true;
          updateAvailability();
          const message = messageFor(error);
          if (announce) setStatus(message, "error");
          return message;
        },
        onSettled: () => {
          refreshButton.disabled = busy;
        },
      },
    );
  }

  async function presentConfirmedMovement(
    actionLabel: string,
    unitId: string,
    write: MovementWriteDto,
    refreshResult: RefreshResult,
  ): Promise<void> {
    const outcomeLabel = write.outcome === "applied"
      ? `${actionLabel} confirmada por Rust.`
      : `Este reintento de ${actionLabel.toLocaleLowerCase("es")} ya estaba aplicado; no se repitieron sus efectos.`;
    let historyMessage = "";
    try {
      const entries = await inventoryIpc.getMovementHistory(unitId);
      const matching = entries
        .filter((entry) => entry.eventId === write.eventId)
        .sort((left, right) => left.recordedAtMs - right.recordedAtMs);
      const latest = matching[matching.length - 1];
      if (latest) renderHistoryEntry(latest, write);
      else historyMessage = " El comando confirmó la operación, pero todavía no aparece su registro de historial.";
    } catch {
      historyMessage = " El comando confirmó la operación, pero no se pudo consultar su resumen de historial.";
    }

    const refreshMessage = refreshResult.kind === "failed"
      ? ` No se pudieron actualizar los selectores: ${refreshResult.message}`
      : "";
    const completeMessage = `${outcomeLabel}${historyMessage}${refreshMessage}`;
    setStatus(completeMessage, historyMessage || refreshMessage ? "error" : "success");
    resultStatus.textContent = completeMessage;
    resultStatus.dataset.kind = historyMessage || refreshMessage ? "error" : "success";
  }

  async function executeMovement(
    mode: OperationMode,
    fingerprint: string,
    unitId: string,
    actionLabel: string,
    send: (intent: PendingIntent) => Promise<MovementWriteDto>,
  ): Promise<void> {
    if (busy || (loadFailed && !pendingIntents.has(mode))) return;
    const intent = intentFor(mode, fingerprint, mode === "receive");
    const retryRequest: RetryRequest = { mode, fingerprint, unitId, actionLabel, send };
    busy = true;
    setMode(activeMode);
    for (const fieldset of Object.values(fieldsets)) fieldset.disabled = true;
    for (const tab of tabs) tab.disabled = true;
    submits[mode].disabled = true;
    setStatus(`${actionLabel} en curso…`);
    setResult("Esperando confirmación de Rust…", "neutral");
    let operationSucceeded = false;
    let operationErrorWasUncertain = false;
    try {
      const write = await send(intent);
      pendingIntents.delete(mode);
      uncertainRetry = null;
      operationSucceeded = true;
      const refreshResult = await refreshData(false);
      await presentConfirmedMovement(actionLabel, unitId, write, refreshResult);
      if (mode === "receive") {
        receiveForm.reset();
        receiveLines.replaceChildren();
        appendReceiveLine();
      }
    } catch (error: unknown) {
      operationErrorWasUncertain = shouldKeepIntent(error);
      if (operationErrorWasUncertain) {
        uncertainRetry = retryRequest;
      } else {
        pendingIntents.delete(mode);
        uncertainRetry = null;
      }
      const operationError = messageFor(error);
      const refreshResult = await refreshData(false);
      const refreshMessage = refreshResult.kind === "failed"
        ? ` Tampoco se pudo actualizar la lista: ${refreshResult.message}`
        : "";
      const retryMessage = operationErrorWasUncertain
        ? " El resultado de escritura no está confirmado. El reintento conservará exactamente los mismos datos e identificador."
        : "";
      setStatus(`${operationError}${retryMessage}${refreshMessage}`, "error");
      setResult(`${operationError}${retryMessage}`, "error");
    } finally {
      busy = false;
      refreshSelectors();
      setMode(activeMode);
      if (operationErrorWasUncertain && uncertainRetry) retryButton.focus();
      else if (activeMode !== mode) tabs.find((tab) => tab.dataset.operationTab === activeMode)?.focus();
      else if (mode === "receive" && operationSucceeded && !receiveFields.disabled) receiveLabel.focus();
      else if (!operationSucceeded && !fieldsets[mode].disabled) submits[mode].focus();
      else if (mode === "store" && !fieldsets.store.disabled) storeUnitSelect.focus();
      else if (mode === "move" && !fieldsets.move.disabled) moveUnitSelect.focus();
      else if (mode === "dispatch" && !fieldsets.dispatch.disabled) dispatchUnitSelect.focus();
      else tabs.find((tab) => tab.dataset.operationTab === mode)?.focus();
    }
  }

  function readReceiptLines(): ReceiveLineInput[] | null {
    const rows = [...receiveLines.querySelectorAll<HTMLElement>(".receive-line-row")];
    const lines: ReceiveLineInput[] = [];
    const uniqueLines = new Set<string>();
    for (const row of rows) {
      const sku = row.querySelector<HTMLSelectElement>("[data-receive-sku]")?.value.trim() ?? "";
      const quantityInput = row.querySelector<HTMLInputElement>("[data-receive-quantity]");
      const quantity = Number(quantityInput?.value);
      const lotCode = row.querySelector<HTMLInputElement>("[data-receive-lot]")?.value.trim() ?? "";
      if (!sku || !Number.isSafeInteger(quantity) || quantity < 1 || quantity > 4_294_967_295) return null;
      const fingerprint = `${sku}\u0000${lotCode}`;
      if (uniqueLines.has(fingerprint)) {
        setStatus("Cada combinación de SKU y lote debe aparecer una sola vez; ajusta las líneas antes de recibir.", "error");
        return null;
      }
      uniqueLines.add(fingerprint);
      lines.push({ sku, quantity, lotCode: lotCode || null });
    }
    return lines.length ? lines : null;
  }

  receiveForm.addEventListener("submit", (event) => {
    event.preventDefault();
    if (!receiveForm.reportValidity() || busy) return;
    const label = receiveLabel.value.trim();
    const lines = readReceiptLines();
    if (!lines) {
      if (status.dataset.kind !== "error") setStatus("Añade al menos una línea válida con SKU y cantidad positiva.", "error");
      return;
    }
    const fingerprint = JSON.stringify({ label, lines });
    const intent = intentFor("receive", fingerprint, true);
    void executeMovement("receive", fingerprint, intent.handlingUnitId!, "Recepción", (request) =>
      inventoryIpc.receiveUnit({
        id: request.handlingUnitId!,
        label,
        lines,
        eventId: request.eventId,
        occurredAtMs: request.occurredAtMs,
      }),
    );
  });

  storeForm.addEventListener("submit", (event) => {
    event.preventDefault();
    if (!storeForm.reportValidity() || busy) return;
    const unit = selectedUnit(storeUnitSelect);
    if (!unit || !storeDestination.value) return;
    const destinationLocationId = storeDestination.value;
    const fingerprint = JSON.stringify({ handlingUnitId: unit.id, destinationLocationId });
    void executeMovement("store", fingerprint, unit.id, "Asignación de posición", (intent) =>
      inventoryIpc.storeUnit({
        eventId: intent.eventId,
        handlingUnitId: unit.id,
        destinationLocationId,
        occurredAtMs: intent.occurredAtMs,
      }),
    );
  });

  moveForm.addEventListener("submit", (event) => {
    event.preventDefault();
    if (!moveForm.reportValidity() || busy) return;
    const unit = selectedUnit(moveUnitSelect);
    if (!unit?.currentLocationId || !moveDestination.value) return;
    const originLocationId = unit.currentLocationId;
    const destinationLocationId = moveDestination.value;
    const fingerprint = JSON.stringify({ handlingUnitId: unit.id, originLocationId, destinationLocationId });
    void executeMovement("move", fingerprint, unit.id, "Traslado", (intent) =>
      inventoryIpc.moveUnit({
        eventId: intent.eventId,
        handlingUnitId: unit.id,
        originLocationId,
        destinationLocationId,
        occurredAtMs: intent.occurredAtMs,
      }),
    );
  });

  dispatchForm.addEventListener("submit", (event) => {
    event.preventDefault();
    if (!dispatchForm.reportValidity() || busy) return;
    const unit = selectedUnit(dispatchUnitSelect);
    if (!unit?.currentLocationId) return;
    if (!window.confirm(`¿Confirmas el despacho de ${unit.label} desde ${getLocationPath(locations.find((item) => item.id === unit.currentLocationId), locations)}?`)) return;
    const originLocationId = unit.currentLocationId;
    const fingerprint = JSON.stringify({ handlingUnitId: unit.id, originLocationId });
    void executeMovement("dispatch", fingerprint, unit.id, "Despacho", (intent) =>
      inventoryIpc.dispatchUnit({
        eventId: intent.eventId,
        handlingUnitId: unit.id,
        originLocationId,
        occurredAtMs: intent.occurredAtMs,
      }),
    );
  });

  for (const tab of tabs) {
    tab.addEventListener("click", () => {
      const mode = tab.dataset.operationTab as OperationMode | undefined;
      if (mode && mode in forms) setMode(mode);
    });
  }
  addLineButton.addEventListener("click", appendReceiveLine);
  storeUnitSelect.addEventListener("change", refreshSelectors);
  moveUnitSelect.addEventListener("change", refreshSelectors);
  dispatchUnitSelect.addEventListener("change", refreshSelectors);
  refreshButton.addEventListener("click", () => void refreshData());
  retryButton.addEventListener("click", () => {
    const retry = uncertainRetry;
    if (!retry || busy) return;
    void executeMovement(retry.mode, retry.fingerprint, retry.unitId, retry.actionLabel, retry.send);
  });

  appendReceiveLine();
  setMode("receive");
  void refreshData();

  return { refresh: async () => { await refreshData(); } };
}
