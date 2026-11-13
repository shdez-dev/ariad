import {
  AriadnaIpcError,
  inventoryIpc,
  type InventorySearchResultDto,
} from "../lib/ipc.ts";
import { locationKindLabels } from "../lib/catalog-model.ts";

type SearchKind = InventorySearchResultDto["kind"];

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`No se encontró el elemento requerido ${selector}.`);
  return element;
}

function messageFor(error: unknown): string {
  if (error instanceof AriadnaIpcError) return error.message;
  return error instanceof Error ? error.message : "No se pudo completar la búsqueda.";
}

function kindLabel(kind: SearchKind): string {
  const labels: Record<SearchKind, string> = {
    product: "Producto",
    handling_unit: "Unidad",
    location: "Ubicación",
  };
  return labels[kind];
}

function kindPluralLabel(kind: SearchKind): string {
  const labels: Record<SearchKind, string> = {
    product: "Productos",
    handling_unit: "Unidades",
    location: "Ubicaciones",
  };
  return labels[kind];
}

function unitStatusLabel(status: string): string {
  const labels: Record<string, string> = {
    received: "Recibido",
    stored: "Almacenado",
    dispatched: "Despachado",
  };
  return labels[status] ?? status;
}

export function mountSearchView(): { refresh: () => Promise<void> } {
  const input = requiredElement<HTMLInputElement>("#inventory-search-input");
  const clearButton = requiredElement<HTMLButtonElement>("#inventory-search-clear");
  const status = requiredElement<HTMLElement>("#inventory-search-status");
  const resultsElement = requiredElement<HTMLElement>("#inventory-search-results");
  const detail = requiredElement<HTMLElement>("#inventory-search-detail");

  let timer: number | undefined;
  let requestNumber = 0;
  let results: InventorySearchResultDto[] = [];
  let loadFailed = false;
  let waitingForSearch = false;

  function setStatus(message: string, kind: "neutral" | "error" = "neutral"): void {
    status.textContent = message;
    status.dataset.kind = kind;
  }

  function setBusy(element: HTMLElement, busy: boolean): void {
    if (busy) element.setAttribute("aria-busy", "true");
    else element.removeAttribute("aria-busy");
  }

  function showEmptyDetail(title: string, message: string): void {
    detail.replaceChildren();
    const wrapper = document.createElement("div");
    wrapper.className = "selection-empty";
    const kicker = document.createElement("p");
    kicker.className = "detail-kicker";
    kicker.textContent = "DETALLE";
    const heading = document.createElement("h2");
    heading.className = "detail-title";
    heading.textContent = title;
    const copy = document.createElement("p");
    copy.className = "detail-muted";
    copy.textContent = message;
    wrapper.append(kicker, heading, copy);
    detail.append(wrapper);
  }

  function renderResults(): void {
    resultsElement.replaceChildren();
    const kinds: SearchKind[] = ["product", "handling_unit", "location"];
    for (const kind of kinds) {
      const group = results.filter((result) => result.kind === kind);
      if (group.length === 0) continue;
      const section = document.createElement("section");
      section.className = "search-result-group";
      const heading = document.createElement("h3");
      heading.className = "detail-section-title";
      heading.textContent = `${group.length === 1 ? kindLabel(kind) : kindPluralLabel(kind)} (${group.length})`;
      const list = document.createElement("ul");
      list.className = "search-result-list";
      for (const result of group) {
        const item = document.createElement("li");
        const button = document.createElement("button");
        button.type = "button";
        button.className = "search-result-button";
        const copy = document.createElement("span");
        copy.className = "search-result-copy";
        const title = document.createElement("strong");
        title.textContent = result.title;
        const subtitle = document.createElement("span");
        subtitle.textContent = result.subtitle;
        copy.append(title, subtitle);
        const badge = document.createElement("span");
        badge.className = `record-badge ${result.status === "active" ? "record-active" : `unit-${result.status}`}`;
        badge.textContent = result.kind === "handling_unit"
          ? unitStatusLabel(result.status)
          : result.status === "active" ? "Activo" : result.status;
        button.append(copy, badge);
        button.addEventListener("click", () => void openResult(result));
        item.append(button);
        list.append(item);
      }
      section.append(heading, list);
      resultsElement.append(section);
    }
    if (results.length === 0) {
      const empty = document.createElement("p");
      empty.className = "detail-muted search-empty";
      empty.textContent = waitingForSearch
        ? "Actualizando resultados…"
        : loadFailed
        ? "No se pudo consultar el inventario. Revisa el mensaje de estado e inténtalo de nuevo."
        : input.value.trim()
          ? "No se encontraron productos, unidades ni ubicaciones para este término."
          : "Escribe un SKU, una etiqueta o un código de ubicación para comenzar.";
      resultsElement.append(empty);
    }
  }

  function renderProductDetail(product: NonNullable<Awaited<ReturnType<typeof inventoryIpc.getProduct>>>): void {
    detail.replaceChildren();
    const header = document.createElement("div");
    header.className = "detail-header";
    const kicker = document.createElement("p");
    kicker.className = "detail-kicker";
    kicker.textContent = "PRODUCTO";
    const title = document.createElement("h2");
    title.className = "detail-title";
    title.textContent = product.description;
    const sku = document.createElement("p");
    sku.className = "detail-name";
    sku.textContent = product.sku;
    header.append(kicker, title, sku);
    const metadata = document.createElement("dl");
    metadata.className = "detail-list";
    appendDetailRow(metadata, "Unidad de medida", product.unitOfMeasure);
    appendDetailRow(metadata, "Estado", product.active ? "Activo" : "Inactivo");
    detail.append(header, metadata);
  }

  function renderLocationDetail(location: NonNullable<Awaited<ReturnType<typeof inventoryIpc.getLocation>>>): void {
    detail.replaceChildren();
    const header = document.createElement("div");
    header.className = "detail-header";
    const kicker = document.createElement("p");
    kicker.className = "detail-kicker";
    kicker.textContent = "UBICACIÓN";
    const title = document.createElement("h2");
    title.className = "detail-title";
    title.textContent = location.code;
    const name = document.createElement("p");
    name.className = "detail-name";
    name.textContent = location.name;
    header.append(kicker, title, name);
    const metadata = document.createElement("dl");
    metadata.className = "detail-list";
    appendDetailRow(metadata, "Nivel", locationKindLabels[location.kind]);
    appendDetailRow(metadata, "Identificador", location.id);
    appendDetailRow(metadata, "Ubicación superior", location.parentId ?? "Nivel superior");
    appendDetailRow(metadata, "Estado", location.active ? "Activa" : "Inactiva");
    detail.append(header, metadata);
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

  async function openResult(result: InventorySearchResultDto): Promise<void> {
    if (result.kind === "handling_unit") {
      setStatus(`Abriendo el historial de ${result.title}…`);
      window.dispatchEvent(new CustomEvent("ariadna:open-unit", {
        detail: { handlingUnitId: result.id },
      }));
      return;
    }
    const currentRequest = ++requestNumber;
    setBusy(detail, true);
    showEmptyDetail("Cargando detalle…", "Consultando el registro local.");
    try {
      if (result.kind === "product") {
        const product = await inventoryIpc.getProduct(result.id);
        if (currentRequest !== requestNumber) return;
        if (!product) {
          showEmptyDetail("Producto no disponible", "Actualiza la búsqueda para volver a consultar el catálogo.");
          setStatus("El producto ya no está disponible. Actualiza la búsqueda para volver a consultar el catálogo.", "error");
          return;
        }
        renderProductDetail(product);
      } else {
        const location = await inventoryIpc.getLocation(result.id);
        if (currentRequest !== requestNumber) return;
        if (!location) {
          showEmptyDetail("Ubicación no disponible", "Actualiza la búsqueda para volver a consultar el catálogo.");
          setStatus("La ubicación ya no está disponible. Actualiza la búsqueda para volver a consultar el catálogo.", "error");
          return;
        }
        renderLocationDetail(location);
      }
      setStatus(`Detalle de ${result.title} cargado.`);
    } catch (error: unknown) {
      if (currentRequest !== requestNumber) return;
      showEmptyDetail("No se pudo cargar el detalle", messageFor(error));
      setStatus(messageFor(error), "error");
    } finally {
      if (currentRequest === requestNumber) setBusy(detail, false);
    }
  }

  async function refresh(): Promise<void> {
    const query = input.value.trim();
    const currentRequest = ++requestNumber;
    if (!query) {
      results = [];
      loadFailed = false;
      waitingForSearch = false;
      setBusy(resultsElement, false);
      setBusy(detail, false);
      renderResults();
      showEmptyDetail("Selecciona un resultado", "Los datos se consultan desde SQLite mediante IPC.");
      setStatus("Escribe un término para consultar los datos locales.");
      return;
    }
    waitingForSearch = true;
    setBusy(resultsElement, true);
    setBusy(detail, true);
    setStatus("Buscando en productos, unidades y ubicaciones…");
    showEmptyDetail("Buscando…", "Consultando el inventario persistido.");
    try {
      const nextResults = await inventoryIpc.searchInventory(query);
      if (currentRequest !== requestNumber) return;
      results = nextResults;
      loadFailed = false;
      waitingForSearch = false;
      setBusy(resultsElement, false);
      setBusy(detail, false);
      renderResults();
      showEmptyDetail("Selecciona un resultado", "El detalle se consultará desde el registro local.");
      setStatus(`${results.length} ${results.length === 1 ? "resultado" : "resultados"} para “${query}”.`);
    } catch (error: unknown) {
      if (currentRequest !== requestNumber) return;
      results = [];
      loadFailed = true;
      waitingForSearch = false;
      setBusy(resultsElement, false);
      setBusy(detail, false);
      renderResults();
      showEmptyDetail("Búsqueda no disponible", messageFor(error));
      setStatus(messageFor(error), "error");
    }
  }

  input.addEventListener("input", () => {
    requestNumber += 1;
    if (timer !== undefined) window.clearTimeout(timer);
    results = [];
    loadFailed = false;
    waitingForSearch = true;
    setBusy(resultsElement, true);
    setBusy(detail, true);
    renderResults();
    setStatus("Actualizando búsqueda…");
    showEmptyDetail("Esperando búsqueda…", "Los resultados se consultan desde los datos locales.");
    timer = window.setTimeout(() => void refresh(), 220);
  });
  clearButton.addEventListener("click", () => {
    if (timer !== undefined) window.clearTimeout(timer);
    input.value = "";
    void refresh();
    input.focus();
  });

  return { refresh };
}
