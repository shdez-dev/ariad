import { inventoryIpc, type HandlingUnitDto, type LocationDto } from "../lib/ipc";
import { locationKindLabels } from "../lib/catalog-model";
import { createWarehouseMapRefreshGuard } from "../lib/warehouse-map-refresh-guard";
import {
  buildWarehouseMap,
  formatPositionAccessibleLabel,
  getPositionState,
  type LocationTreeNode,
  type PositionState,
  type WarehouseMapModel,
} from "../lib/warehouse-map";

document.documentElement.dataset.app = "ariadna";

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`No se encontró el elemento requerido ${selector}.`);
  return element;
}

const mapElement = requiredElement<HTMLElement>("#warehouse-map");
const mapPanel = requiredElement<HTMLElement>("#map-panel");
const mapStatus = requiredElement<HTMLElement>("#map-status");
const summaryElement = requiredElement<HTMLElement>("#warehouse-summary");
const detailsElement = requiredElement<HTMLElement>("#position-details");
const refreshButton = requiredElement<HTMLButtonElement>("#refresh-map");
const selectionAnnouncement = requiredElement<HTMLElement>("#selection-announcement");

let mapModel: WarehouseMapModel | null = null;
let selectedPositionId: string | null = null;
const refreshGuard = createWarehouseMapRefreshGuard();
let mapHasRendered = false;
let pendingMapFocusId: string | null = null;
const expandedLocationIds = new Set<string>();

const stateLabels: Record<PositionState, string> = {
  free: "Libre",
  occupied: "Ocupada",
  inactive: "Inactiva",
  inconsistent: "Revisar",
};

function createElement<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (text !== undefined) element.textContent = text;
  return element;
}

function appendTextPair(parent: HTMLElement, label: string, value: string): void {
  const row = createElement("div", "detail-row");
  row.append(createElement("dt", "detail-label", label), createElement("dd", "detail-value", value));
  parent.append(row);
}

function stateFor(location: LocationDto): { state: PositionState; icon: string } {
  if (!mapModel) return { state: "free", icon: "○" };
  const state = getPositionState(location, mapModel.unitsByLocation);
  const icon: Record<PositionState, string> = {
    free: "○",
    occupied: "●",
    inactive: "–",
    inconsistent: "!",
  };
  return { state, icon: icon[state] };
}

function positionsUnder(node: LocationTreeNode): LocationDto[] {
  if (node.location.kind === "position") return [node.location];
  return node.children.flatMap(positionsUnder);
}

function groupStatusText(node: LocationTreeNode): string {
  const positions = positionsUnder(node);
  const occupied = positions.filter((position) => {
    const { state } = stateFor(position);
    return state === "occupied" || state === "inconsistent";
  }).length;
  const active = positions.filter((position) => position.active).length;
  return `${positions.length} ${positions.length === 1 ? "posición" : "posiciones"} · ${occupied} ocupadas · ${active} activas`;
}

function renderSummary(model: WarehouseMapModel): void {
  summaryElement.replaceChildren();
  const metrics: Array<[string, string, string]> = [
    ["Zonas", String(model.summary.zoneCount), "Nivel superior de la bodega"],
    ["Posiciones activas", String(model.summary.activePositionCount), "Ubicaciones habilitadas"],
    ["Ocupadas", String(model.summary.occupiedPositionCount), "Según el estado guardado"],
    ["Pallets registrados", String(model.summary.handlingUnitCount), "Sin contar despachados"],
  ];

  for (const [label, value, caption] of metrics) {
    const card = createElement("article", "metric-card");
    card.append(
      createElement("span", "metric-label", label),
      createElement("strong", "metric-value", value),
      createElement("span", "metric-caption", caption),
    );
    summaryElement.append(card);
  }
}

function renderPosition(node: LocationTreeNode): HTMLButtonElement {
  const position = node.location;
  const { state, icon } = stateFor(position);
  const button = createElement("button", `position-card state-${state}`);
  button.type = "button";
  button.dataset.positionId = position.id;
  button.dataset.mapFocusId = position.id;
  button.setAttribute("aria-pressed", String(position.id === selectedPositionId));
  button.setAttribute(
    "aria-label",
    formatPositionAccessibleLabel(position, stateLabels[state]),
  );
  button.append(
    createElement("span", "position-code", position.code),
    createElement("span", "position-name", position.name),
  );
  const status = createElement("span", "position-status");
  status.append(createElement("span", "status-symbol", icon), document.createTextNode(stateLabels[state]));
  button.append(status);
  button.addEventListener("click", () => selectPosition(position.id));
  return button;
}

function syncSelectedPosition(): void {
  for (const button of mapElement.querySelectorAll<HTMLButtonElement>("[data-position-id]")) {
    button.setAttribute("aria-pressed", String(button.dataset.positionId === selectedPositionId));
  }
}

function rememberMapFocus(): void {
  const activeElement = document.activeElement;
  pendingMapFocusId =
    activeElement instanceof HTMLElement && mapElement.contains(activeElement)
      ? activeElement.dataset.mapFocusId ?? null
      : null;
}

function restoreMapFocus(): void {
  const focusId = pendingMapFocusId;
  pendingMapFocusId = null;
  if (!focusId) return;
  const target = [...mapElement.querySelectorAll<HTMLElement>("[data-map-focus-id]")].find(
    (element) => element.dataset.mapFocusId === focusId,
  );
  target?.focus({ preventScroll: true });
}

function renderLocationGroup(node: LocationTreeNode): HTMLElement {
  if (node.location.kind === "position") return renderPosition(node);

  const group = createElement("details", `location-group group-${node.location.kind}`);
  group.dataset.locationId = node.location.id;
  group.open = !mapHasRendered || expandedLocationIds.has(node.location.id);
  if (!mapHasRendered) expandedLocationIds.add(node.location.id);
  const summary = createElement("summary", "location-summary");
  summary.dataset.mapFocusId = node.location.id;
  const copy = createElement("span", "location-summary-copy");
  copy.append(
    createElement("span", "location-kind", locationKindLabels[node.location.kind]),
    createElement("strong", "location-name", node.location.name),
    createElement("span", "location-code", node.location.code),
  );
  summary.append(copy, createElement("span", "location-count", groupStatusText(node)));
  group.append(summary);
  group.addEventListener("toggle", () => {
    if (group.open) expandedLocationIds.add(node.location.id);
    else expandedLocationIds.delete(node.location.id);
  });

  if (!node.location.active) {
    group.classList.add("is-inactive");
  }

  if (node.children.length) {
    const children = createElement("div", "location-children");
    for (const child of node.children) {
      const childElement = renderLocationGroup(child);
      if (child.location.kind === "position") {
        const positionGrid = createElement("div", "position-grid");
        positionGrid.append(childElement);
        children.append(positionGrid);
      } else {
        children.append(childElement);
      }
    }
    group.append(children);
  } else {
    group.append(createElement("p", "group-empty", "Todavía no hay ubicaciones debajo de este nivel."));
  }
  return group;
}

function renderMap(model: WarehouseMapModel): void {
  mapElement.replaceChildren();
  if (model.zones.length === 0 && model.unassigned.length === 0) {
    const empty = createElement("div", "empty-state");
    empty.append(
      createElement("span", "empty-state-mark", "⌖"),
      createElement("h3", "empty-state-title", "La bodega todavía no tiene ubicaciones"),
      createElement(
        "p",
        "empty-state-copy",
        "Cuando configures zonas, pasillos, racks y posiciones, aparecerán aquí en su jerarquía. La configuración de ubicaciones se habilitará en el siguiente flujo del proyecto.",
      ),
    );
    mapElement.append(empty);
    mapStatus.textContent = "Sin ubicaciones configuradas.";
    restoreMapFocus();
    return;
  }

  const list = createElement("div", "zone-list");
  for (const zone of model.zones) {
    const card = createElement("article", "zone-card");
    const header = createElement("div", "zone-header");
    const heading = createElement("div", "zone-heading");
    heading.append(
      createElement("span", "level-kicker", "ZONA"),
      createElement("h3", "zone-title", zone.location.name),
      createElement("span", "zone-code", zone.location.code),
    );
    header.append(heading, createElement("span", "zone-summary", groupStatusText(zone)));
    card.append(header);
    const children = createElement("div", "zone-children");
    if (zone.children.length === 0) {
      children.append(createElement("p", "group-empty", "Añade un pasillo para empezar a organizar esta zona."));
    } else {
      for (const child of zone.children) children.append(renderLocationGroup(child));
    }
    card.append(children);
    if (!zone.location.active) card.classList.add("is-inactive");
    list.append(card);
  }

  if (model.unassigned.length) {
    const warning = createElement("section", "unassigned-section");
    warning.append(
      createElement("h3", "unassigned-title", "Ubicaciones por revisar"),
      createElement(
        "p",
        "unassigned-copy",
        "Estos registros no tienen un padre compatible en la jerarquía guardada.",
      ),
    );
    const items = createElement("div", "unassigned-list");
    for (const node of model.unassigned) items.append(renderLocationGroup(node));
    warning.append(items);
    list.append(warning);
  }
  mapElement.append(list);
  mapHasRendered = true;
  restoreMapFocus();

  const positionCount = model.summary.positionCount;
  mapStatus.textContent = `${model.summary.zoneCount} zonas y ${positionCount} ${positionCount === 1 ? "posición" : "posiciones"} cargadas.`;
}

function renderUnit(unit: HandlingUnitDto): HTMLElement {
  const section = createElement("section", "unit-detail");
  section.append(createElement("h3", "detail-section-title", "Pallet asociado"));
  const metadata = createElement("dl", "detail-list");
  appendTextPair(metadata, "Etiqueta", unit.label);
  appendTextPair(metadata, "Identificador", unit.id);
  appendTextPair(metadata, "Estado", unit.status === "stored" ? "Almacenado" : "Recibido");
  section.append(metadata);

  const linesHeading = createElement("h4", "detail-subtitle", "Productos en el pallet");
  section.append(linesHeading);
  if (unit.lines.length === 0) {
    section.append(createElement("p", "detail-muted", "El pallet no tiene líneas de producto."));
  } else {
    const lines = createElement("ul", "product-line-list");
    for (const line of unit.lines) {
      const item = createElement("li", "product-line");
      const copy = createElement("span", "product-line-copy");
      copy.append(
        createElement("strong", "product-sku", line.sku),
        createElement("span", "product-lot", line.lotCode ? `Lote ${line.lotCode}` : "Sin lote"),
      );
      item.append(copy, createElement("span", "product-quantity", `× ${line.quantity}`));
      lines.append(item);
    }
    section.append(lines);
  }
  return section;
}

function renderPositionDetails(
  location: LocationDto,
  units: HandlingUnitDto[],
  loading = false,
  errorMessage?: string,
): void {
  detailsElement.replaceChildren();
  const { state, icon } = stateFor(location);
  const header = createElement("div", "detail-header");
  header.append(
    createElement("span", `detail-status state-${state}`, `${icon} ${stateLabels[state]}`),
    createElement("p", "detail-kicker", "DETALLE DE POSICIÓN"),
    createElement("h2", "detail-title", location.code),
    createElement("p", "detail-name", location.name),
  );
  detailsElement.append(header);

  const metadata = createElement("dl", "detail-list");
  appendTextPair(metadata, "Estado de ubicación", location.active ? "Activa" : "Inactiva");
  appendTextPair(metadata, "Código", location.code);
  detailsElement.append(metadata);

  if (!location.active) {
    detailsElement.append(
      createElement("p", "detail-note", "Esta posición está inactiva y no debe recibir movimientos."),
    );
  }
  if (units.length > 1) {
    detailsElement.append(
      createElement("p", "detail-warning", "Hay más de una unidad asociada. Revisa la consistencia de estos datos."),
    );
    for (const unit of units) detailsElement.append(renderUnit(unit));
  } else if (loading) {
    detailsElement.append(createElement("p", "detail-muted", "Consultando el pallet asociado…"));
  } else if (errorMessage) {
    detailsElement.append(createElement("p", "detail-warning", errorMessage));
  } else if (units.length === 1) {
    detailsElement.append(renderUnit(units[0]));
  } else {
    detailsElement.append(
      createElement("p", "detail-note", "No hay un pallet almacenado en esta posición."),
    );
  }
}

function renderNoSelection(): void {
  detailsElement.removeAttribute("aria-busy");
  detailsElement.replaceChildren();
  const empty = createElement("div", "selection-empty");
  empty.append(
    createElement("span", "selection-mark", "⌖"),
    createElement("p", "detail-kicker", "DETALLE DE UBICACIÓN"),
    createElement("h2", "detail-title", "Selecciona una posición"),
    createElement(
      "p",
      "detail-muted",
      "Aquí podrás consultar el estado guardado y, si está ocupada, ver el pallet y los productos asociados.",
    ),
  );
  detailsElement.append(empty);
}

function renderMapUpdatingDetails(): void {
  detailsElement.replaceChildren();
  const empty = createElement("div", "selection-empty");
  empty.append(
    createElement("span", "selection-mark", "⌖"),
    createElement("p", "detail-kicker", "DETALLE DE UBICACIÓN"),
    createElement("h2", "detail-title", "Actualizando posición"),
    createElement("p", "detail-muted", "Consultando la ubicación y el pallet guardados."),
  );
  detailsElement.append(empty);
}

async function selectPosition(locationId: string): Promise<void> {
  if (!mapModel) return;
  const node = mapModel.locationsById.get(locationId);
  if (!node || node.location.kind !== "position") return;

  selectedPositionId = locationId;
  syncSelectedPosition();
  const units = mapModel.unitsByLocation.get(locationId) ?? [];
  const requestNumber = refreshGuard.beginDetailRequest();
  if (units.length === 1) detailsElement.setAttribute("aria-busy", "true");
  else detailsElement.removeAttribute("aria-busy");
  renderPositionDetails(node.location, units, units.length === 1);
  const state = stateLabels[stateFor(node.location).state].toLowerCase();
  if (units.length === 1) {
    selectionAnnouncement.textContent = `Posición ${node.location.code} seleccionada, ${state}. Consultando el pallet ${units[0].label}.`;
  } else if (units.length > 1) {
    selectionAnnouncement.textContent = `Posición ${node.location.code} seleccionada. Hay más de una unidad asociada y requiere revisión.`;
    return;
  } else {
    selectionAnnouncement.textContent = `Posición ${node.location.code} seleccionada, ${state}. No hay un pallet almacenado.`;
    return;
  }

  try {
    const detailedUnit = await inventoryIpc.getHandlingUnit(units[0].id);
    if (!refreshGuard.isCurrentDetailRequest(requestNumber) || selectedPositionId !== locationId) return;
    renderPositionDetails(node.location, detailedUnit ? [detailedUnit] : [], false);
    selectionAnnouncement.textContent = detailedUnit
      ? `Detalle del pallet ${detailedUnit.label} cargado para la posición ${node.location.code}.`
      : `No se encontró el pallet asociado a la posición ${node.location.code}. Actualiza el mapa.`;
  } catch (error: unknown) {
    if (!refreshGuard.isCurrentDetailRequest(requestNumber) || selectedPositionId !== locationId) return;
    const message = error instanceof Error ? error.message : "No se pudo consultar el pallet asociado.";
    renderPositionDetails(node.location, units, false, message);
    selectionAnnouncement.textContent = `No se pudo consultar el pallet de la posición ${node.location.code}.`;
  } finally {
    if (refreshGuard.isCurrentDetailRequest(requestNumber)) detailsElement.removeAttribute("aria-busy");
  }
}

async function refreshWarehouseMap(): Promise<void> {
  const requestNumber = refreshGuard.beginRefresh();
  rememberMapFocus();
  if (selectedPositionId) {
    detailsElement.setAttribute("aria-busy", "true");
    renderMapUpdatingDetails();
  } else {
    detailsElement.removeAttribute("aria-busy");
  }
  refreshButton.disabled = true;
  refreshButton.setAttribute("aria-busy", "true");
  mapPanel.setAttribute("aria-busy", "true");
  mapElement.setAttribute("aria-busy", "true");
  mapStatus.textContent = "Consultando ubicaciones e inventario guardados…";
  mapElement.replaceChildren(createElement("p", "loading-state", "Cargando mapa…"));

  try {
    const [locations, units] = await Promise.all([
      inventoryIpc.listLocations(true),
      inventoryIpc.listHandlingUnits(false),
    ]);
    if (!refreshGuard.isLatestRefresh(requestNumber)) return;
    mapModel = buildWarehouseMap(locations, units);
    renderSummary(mapModel);
    renderMap(mapModel);
    mapElement.removeAttribute("aria-busy");

    const selected = selectedPositionId ? mapModel.locationsById.get(selectedPositionId) : undefined;
    if (selected && selected.location.kind === "position") {
      await selectPosition(selected.location.id);
    } else {
      selectedPositionId = null;
      renderNoSelection();
    }
  } catch (error: unknown) {
    if (!refreshGuard.isLatestRefresh(requestNumber)) return;
    mapModel = null;
    selectedPositionId = null;
    summaryElement.replaceChildren();
    mapElement.replaceChildren();
    renderNoSelection();
    const message = error instanceof Error ? error.message : "No se pudo cargar el mapa de la bodega.";
    mapStatus.textContent = "No fue posible actualizar los datos.";
    selectionAnnouncement.textContent = "No se pudo actualizar el mapa de la bodega.";
    const errorState = createElement("div", "error-state");
    errorState.append(
      createElement("strong", "error-title", "No se pudo leer el inventario"),
      createElement("p", "error-copy", message),
    );
    mapElement.append(errorState);
    if (pendingMapFocusId) {
      pendingMapFocusId = null;
      mapStatus.tabIndex = -1;
      mapStatus.focus({ preventScroll: true });
    }
  } finally {
    if (refreshGuard.isLatestRefresh(requestNumber)) {
      refreshButton.disabled = false;
      refreshButton.removeAttribute("aria-busy");
      mapPanel.removeAttribute("aria-busy");
      mapElement.removeAttribute("aria-busy");
    }
  }
}

refreshButton.addEventListener("click", () => void refreshWarehouseMap());
window.addEventListener("ariadna:inventory-changed", () => void refreshWarehouseMap());
void refreshWarehouseMap();

