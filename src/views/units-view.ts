import { AriadnaIpcError, inventoryIpc, type HandlingUnitDto, type LocationDto } from "../lib/ipc";
import { getLocationPath } from "../lib/catalog-model";

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

export function mountUnitsView(): { refresh: () => Promise<void> } {
  const rows = requiredElement<HTMLTableSectionElement>("#units-table-body");
  const status = requiredElement<HTMLElement>("#units-status");
  const filter = requiredElement<HTMLInputElement>("#units-filter");
  const refreshButton = requiredElement<HTMLButtonElement>("#units-refresh");
  const detail = requiredElement<HTMLElement>("#unit-details");
  const detailAnnouncement = requiredElement<HTMLElement>("#unit-detail-announcement");

  let units: HandlingUnitDto[] = [];
  let locations: LocationDto[] = [];
  let selectedUnitId: string | null = null;
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

  function renderDetail(unit: HandlingUnitDto, loading: boolean, error?: string): void {
    detail.replaceChildren();
    const header = document.createElement("div");
    header.className = "detail-header";
    const kicker = document.createElement("p");
    kicker.className = "detail-kicker";
    kicker.textContent = "DETALLE DE UNIDAD";
    const title = document.createElement("h2");
    title.className = "detail-title";
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
    if (loading) {
      const loadingText = document.createElement("p");
      loadingText.className = "detail-muted";
      loadingText.textContent = "Consultando información actual…";
      detail.append(loadingText);
    } else if (error) {
      const errorText = document.createElement("p");
      errorText.className = "detail-warning";
      errorText.textContent = error;
      detail.append(errorText);
    } else if (unit.lines.length === 0) {
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
  }

  async function openDetails(unit: HandlingUnitDto): Promise<void> {
    selectedUnitId = unit.id;
    const currentRequest = ++requestNumber;
    renderDetail(unit, true);
    detailAnnouncement.textContent = `Consultando detalle del pallet ${unit.label}.`;
    try {
      const latest = await inventoryIpc.getHandlingUnit(unit.id);
      if (currentRequest !== requestNumber || selectedUnitId !== unit.id) return;
      if (!latest) {
        detail.replaceChildren();
        const notFound = document.createElement("p");
        notFound.className = "detail-warning";
        notFound.textContent = `No se encontró la unidad ${unit.label}. Actualiza la lista.`;
        detail.append(notFound);
        setStatus(`No se encontró la unidad ${unit.label}. Actualiza la lista.`, "error");
        detailAnnouncement.textContent = `No se encontró el pallet ${unit.label}.`;
        return;
      }
      renderDetail(latest, false);
      detailAnnouncement.textContent = `Detalle del pallet ${latest.label} cargado.`;
    } catch (error: unknown) {
      if (currentRequest !== requestNumber || selectedUnitId !== unit.id) return;
      renderDetail(unit, false, messageFor(error));
      detailAnnouncement.textContent = `No se pudo cargar el detalle del pallet ${unit.label}.`;
    }
  }

  async function refresh(): Promise<void> {
    const currentRefresh = ++refreshRequestNumber;
    requestNumber += 1;
    refreshButton.disabled = true;
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
      if (selected) await openDetails(selected);
      else if (selectedUnitId) {
        selectedUnitId = null;
        detail.replaceChildren();
        setStatus("La unidad seleccionada ya no está disponible en la lista.", "error");
      }
    } catch (error: unknown) {
      if (currentRefresh !== refreshRequestNumber) return;
      units = [];
      locations = [];
      loadFailed = true;
      renderRows();
      detail.replaceChildren();
      setStatus(messageFor(error), "error");
      detailAnnouncement.textContent = "No se pudo cargar la lista de unidades.";
    } finally {
      if (currentRefresh === refreshRequestNumber) refreshButton.disabled = false;
    }
  }

  filter.addEventListener("input", renderRows);
  refreshButton.addEventListener("click", () => void refresh());
  return { refresh };
}

