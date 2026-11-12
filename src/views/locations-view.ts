import { AriadnaIpcError, inventoryIpc, type LocationDto, type LocationKind } from "../lib/ipc.ts";
import {
  availableLocationParents,
  filterLocations,
  getLocationPath,
  locationKindLabels,
  requiredLocationParentKind,
} from "../lib/catalog-model.ts";
import { createCatalogEditRequestGuard } from "../lib/catalog-edit-request-guard.ts";

type RefreshResult =
  | { kind: "loaded" }
  | { kind: "failed"; message: string }
  | { kind: "superseded" };

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`No se encontró el elemento requerido ${selector}.`);
  return element;
}

function messageFor(error: unknown): string {
  if (error instanceof AriadnaIpcError) return error.message;
  return error instanceof Error ? error.message : "No se pudo completar la operación.";
}

export function mountLocationsView(): { refresh: () => Promise<RefreshResult> } {
  const rows = requiredElement<HTMLTableSectionElement>("#locations-table-body");
  const status = requiredElement<HTMLElement>("#locations-status");
  const filter = requiredElement<HTMLInputElement>("#locations-filter");
  const includeInactive = requiredElement<HTMLInputElement>("#locations-include-inactive");
  const form = requiredElement<HTMLFormElement>("#location-form");
  const formTitle = requiredElement<HTMLElement>("#location-form-title");
  const formMessage = requiredElement<HTMLElement>("#location-form-message");
  const formSubmit = requiredElement<HTMLButtonElement>("#location-form-submit");
  const codeInput = requiredElement<HTMLInputElement>("#location-code");
  const nameInput = requiredElement<HTMLInputElement>("#location-name");
  const kindInput = requiredElement<HTMLSelectElement>("#location-kind");
  const parentInput = requiredElement<HTMLSelectElement>("#location-parent");
  const parentField = requiredElement<HTMLElement>("#location-parent-field");
  const parentHint = requiredElement<HTMLElement>("#location-parent-hint");
  const newButton = requiredElement<HTMLButtonElement>("#location-new");
  const cancelButton = requiredElement<HTMLButtonElement>("#location-cancel");
  const refreshButton = requiredElement<HTMLButtonElement>("#locations-refresh");
  const editRequestGuard = createCatalogEditRequestGuard();

  let locations: LocationDto[] = [];
  let editingLocation: LocationDto | null = null;
  let loadingFailed = false;
  let refreshRequestNumber = 0;
  let userIntentNumber = 0;

  function beginUserIntent(): number {
    return ++userIntentNumber;
  }

  function isCurrentUserIntent(intent: number): boolean {
    return intent === userIntentNumber;
  }

  function setStatus(text: string, kind: "success" | "error" | "neutral" = "neutral"): void {
    status.textContent = text;
    status.dataset.kind = kind;
  }

  function renderLocations(): void {
    rows.replaceChildren();
    const filtered = filterLocations(locations, filter.value);
    if (filtered.length === 0) {
      const row = document.createElement("tr");
      const cell = document.createElement("td");
      cell.colSpan = 5;
      cell.className = "table-empty";
      cell.textContent = loadingFailed
        ? "No se pudo cargar la lista de ubicaciones."
        : locations.length === 0
          ? "Aún no hay ubicaciones. Crea una zona para empezar la jerarquía."
          : "No hay ubicaciones que coincidan con el filtro.";
      row.append(cell);
      rows.append(row);
      return;
    }

    for (const location of filtered) {
      const row = document.createElement("tr");
      const place = document.createElement("td");
      const identity = document.createElement("div");
      identity.className = "location-identity";
      const code = document.createElement("strong");
      code.className = "table-primary-text";
      code.textContent = location.code;
      const name = document.createElement("span");
      name.className = "table-secondary-text";
      name.textContent = location.name;
      identity.append(code, name);
      place.append(identity);

      const kind = document.createElement("td");
      kind.textContent = locationKindLabels[location.kind];
      const parent = document.createElement("td");
      parent.textContent = location.parentId
        ? getLocationPath(locations.find((item) => item.id === location.id), locations).split(" / ").slice(0, -1).join(" / ") || "Sin nivel superior"
        : "Nivel superior";

      const state = document.createElement("td");
      const badge = document.createElement("span");
      badge.className = `record-badge ${location.active ? "record-active" : "record-inactive"}`;
      badge.textContent = location.active ? "Activa" : "Inactiva";
      state.append(badge);

      const actions = document.createElement("td");
      actions.className = "table-actions";
      const edit = document.createElement("button");
      edit.type = "button";
      edit.className = "text-button";
      edit.textContent = "Ver / editar";
      edit.addEventListener("click", () => void openEditForm(location));
      actions.append(edit);
      if (location.active) {
        const deactivate = document.createElement("button");
        deactivate.type = "button";
        deactivate.className = "text-button danger-text";
        deactivate.textContent = "Desactivar";
        deactivate.addEventListener("click", () => void deactivateLocation(location));
        actions.append(deactivate);
      }
      row.append(place, kind, parent, state, actions);
      rows.append(row);
    }
  }

  function refreshParents(kind: LocationKind, selectedParentId?: string | null): void {
    const parents = availableLocationParents(kind, locations);
    parentInput.replaceChildren();
    const prompt = document.createElement("option");
    prompt.value = "";
    prompt.textContent = "Selecciona una ubicación superior";
    parentInput.append(prompt);
    for (const parent of parents) {
      const option = document.createElement("option");
      option.value = parent.id;
      option.textContent = `${parent.code} · ${parent.name}`;
      parentInput.append(option);
    }
    if (selectedParentId) parentInput.value = selectedParentId;
    const requiredParentKind = requiredLocationParentKind(kind);
    const needsParent = requiredParentKind !== null;
    parentField.hidden = !needsParent;
    parentInput.required = needsParent;
    parentHint.textContent = needsParent
      ? parents.length
        ? `El nivel ${locationKindLabels[kind].toLowerCase()} requiere una ${locationKindLabels[requiredParentKind].toLowerCase()} activa.`
        : `Primero crea una ubicación activa del nivel ${locationKindLabels[requiredParentKind].toLowerCase()}.`
      : "Las zonas son el nivel superior y no requieren ubicación padre.";
    formSubmit.disabled = needsParent && parents.length === 0;
  }

  function openNewForm(): void {
    beginUserIntent();
    editRequestGuard.invalidate();
    editingLocation = null;
    form.reset();
    form.hidden = false;
    formTitle.textContent = "Nueva ubicación";
    formSubmit.textContent = "Guardar ubicación";
    formSubmit.disabled = false;
    kindInput.disabled = false;
    formMessage.textContent = "El identificador interno se generará automáticamente.";
    formMessage.dataset.kind = "neutral";
    setStatus("Formulario de nueva ubicación listo.");
    refreshParents("zone");
    codeInput.focus();
  }

  async function openEditForm(location: LocationDto): Promise<void> {
    beginUserIntent();
    form.hidden = true;
    formSubmit.disabled = true;
    setStatus(`Consultando ${location.code}…`);
    await editRequestGuard.run(
      () => inventoryIpc.getLocation(location.id),
      (current) => {
        if (!current) {
          setStatus(`No se encontró ${location.code}. Actualiza las ubicaciones.`, "error");
          void refresh();
          return;
        }
        editingLocation = current;
        form.hidden = false;
        formTitle.textContent = `Editar ${current.code}`;
        formSubmit.textContent = "Guardar cambios";
        kindInput.value = current.kind;
        kindInput.disabled = true;
        codeInput.value = current.code;
        nameInput.value = current.name;
        refreshParents(current.kind, current.parentId);
        parentField.hidden = true;
        parentInput.required = false;
        formSubmit.disabled = false;
        const parentPath = current.parentId
          ? getLocationPath(locations.find((item) => item.id === current.parentId), locations)
          : "Nivel superior";
        formMessage.textContent = `${locationKindLabels[current.kind]} · ${parentPath}. El nivel y su ubicación superior quedan fijos para conservar la jerarquía histórica.`;
        formMessage.dataset.kind = "neutral";
        codeInput.focus();
      },
      (error) => setStatus(messageFor(error), "error"),
    );
  }

  async function refresh(options: { preserveEditIntent?: boolean } = {}): Promise<RefreshResult> {
    const editIntent = options.preserveEditIntent
      ? editRequestGuard.current()
      : editRequestGuard.invalidate();
    const requestNumber = ++refreshRequestNumber;
    refreshButton.disabled = true;
    const announceRefresh = !options.preserveEditIntent;
    if (announceRefresh && editRequestGuard.isCurrent(editIntent)) setStatus("Consultando ubicaciones…");
    try {
      const nextLocations = await inventoryIpc.listLocations(includeInactive.checked);
      if (requestNumber !== refreshRequestNumber) return { kind: "superseded" };
      locations = nextLocations;
      loadingFailed = false;
      renderLocations();
      if (announceRefresh && editRequestGuard.isCurrent(editIntent)) {
        setStatus(`${locations.length} ${locations.length === 1 ? "ubicación cargada" : "ubicaciones cargadas"}.`);
      }
      return { kind: "loaded" };
    } catch (error: unknown) {
      if (requestNumber !== refreshRequestNumber) return { kind: "superseded" };
      locations = [];
      loadingFailed = true;
      renderLocations();
      const message = messageFor(error);
      if (announceRefresh && editRequestGuard.isCurrent(editIntent)) setStatus(message, "error");
      return { kind: "failed", message };
    } finally {
      if (requestNumber === refreshRequestNumber) refreshButton.disabled = false;
    }
  }

  async function deactivateLocation(location: LocationDto): Promise<void> {
    const confirmed = window.confirm(`¿Desactivar ${location.code}? Sus referencias históricas se conservarán.`);
    if (!confirmed) return;
    editRequestGuard.invalidate();
    const actionIntent = beginUserIntent();
    setStatus(`Desactivando ${location.code}…`);
    try {
      await inventoryIpc.deactivateLocation(location.id);
      const result = await refresh({ preserveEditIntent: true });
      if (!isCurrentUserIntent(actionIntent)) return;
      if (result.kind === "loaded" || result.kind === "superseded") {
        setStatus(`Ubicación ${location.code} desactivada.`, "success");
      } else if (result.kind === "failed") {
        setStatus(`Ubicación ${location.code} desactivada, pero no se pudo actualizar la lista: ${result.message}`, "error");
      }
      if (result.kind === "loaded") refreshButton.focus();
    } catch (error: unknown) {
      if (isCurrentUserIntent(actionIntent)) setStatus(messageFor(error), "error");
    }
  }

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (formSubmit.disabled || !form.reportValidity()) return;
    editRequestGuard.invalidate();
    const formIntent = beginUserIntent();

    const code = codeInput.value.trim();
    const name = nameInput.value.trim();
    formSubmit.disabled = true;
    formMessage.textContent = "Guardando…";
    formMessage.dataset.kind = "neutral";
    setStatus("Guardando ubicación…");

    const request = editingLocation
      ? inventoryIpc.updateLocation({ id: editingLocation.id, code, name })
      : inventoryIpc.createLocation({
          id: globalThis.crypto.randomUUID(),
          code,
          name,
          kind: kindInput.value as LocationKind,
          parentId: kindInput.value === "zone" ? null : parentInput.value,
        });

    void request.then(async (location) => {
      if (isCurrentUserIntent(formIntent)) form.hidden = true;
      const result = await refresh({ preserveEditIntent: true });
      if (!isCurrentUserIntent(formIntent)) return;
      if (result.kind === "loaded" || result.kind === "superseded") {
        setStatus(`Ubicación ${location.code} guardada.`, "success");
      } else if (result.kind === "failed") {
        setStatus(`Ubicación ${location.code} guardada, pero no se pudo actualizar la lista: ${result.message}`, "error");
      }
      newButton.focus();
    }).catch((error: unknown) => {
      if (!isCurrentUserIntent(formIntent)) return;
      formMessage.textContent = messageFor(error);
      formMessage.dataset.kind = "error";
      setStatus(messageFor(error), "error");
    }).finally(() => {
      if (isCurrentUserIntent(formIntent)) formSubmit.disabled = false;
    });
  });

  newButton.addEventListener("click", openNewForm);
  cancelButton.addEventListener("click", () => {
    beginUserIntent();
    editRequestGuard.invalidate();
    form.hidden = true;
    formMessage.textContent = "";
    setStatus("Formulario de ubicación cerrado.");
    newButton.focus();
  });
  refreshButton.addEventListener("click", () => void refresh());
  includeInactive.addEventListener("change", () => void refresh());
  filter.addEventListener("input", renderLocations);
  kindInput.addEventListener("change", () => refreshParents(kindInput.value as LocationKind));

  return { refresh };
}

