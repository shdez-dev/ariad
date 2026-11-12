import { AriadnaIpcError, inventoryIpc, type ProductDto } from "../lib/ipc.ts";
import { filterProducts } from "../lib/catalog-model.ts";
import { createCatalogEditRequestGuard } from "../lib/catalog-edit-request-guard.ts";

type RefreshResult =
  | { kind: "loaded" }
  | { kind: "failed"; message: string }
  | { kind: "superseded" };

type ProductFormMode = "create" | "edit";

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`No se encontró el elemento requerido ${selector}.`);
  return element;
}

function messageFor(error: unknown): string {
  if (error instanceof AriadnaIpcError) return error.message;
  return error instanceof Error ? error.message : "No se pudo completar la operación.";
}

export function mountProductsView(): { refresh: () => Promise<RefreshResult> } {
  const rows = requiredElement<HTMLTableSectionElement>("#products-table-body");
  const status = requiredElement<HTMLElement>("#products-status");
  const filter = requiredElement<HTMLInputElement>("#products-filter");
  const includeInactive = requiredElement<HTMLInputElement>("#products-include-inactive");
  const form = requiredElement<HTMLFormElement>("#product-form");
  const formTitle = requiredElement<HTMLElement>("#product-form-title");
  const formMessage = requiredElement<HTMLElement>("#product-form-message");
  const formSubmit = requiredElement<HTMLButtonElement>("#product-form-submit");
  const skuInput = requiredElement<HTMLInputElement>("#product-sku");
  const descriptionInput = requiredElement<HTMLInputElement>("#product-description");
  const unitInput = requiredElement<HTMLInputElement>("#product-unit");
  const newButton = requiredElement<HTMLButtonElement>("#product-new");
  const cancelButton = requiredElement<HTMLButtonElement>("#product-cancel");
  const refreshButton = requiredElement<HTMLButtonElement>("#products-refresh");
  const editRequestGuard = createCatalogEditRequestGuard();

  let products: ProductDto[] = [];
  let formMode: ProductFormMode = "create";
  let editingSku: string | null = null;
  let loadFailed = false;
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

  function renderProducts(): void {
    rows.replaceChildren();
    const filtered = filterProducts(products, filter.value);
    if (filtered.length === 0) {
      const row = document.createElement("tr");
      const cell = document.createElement("td");
      cell.colSpan = 5;
      cell.className = "table-empty";
      cell.textContent = loadFailed
        ? "No se pudo cargar el catálogo de productos."
        : products.length === 0
          ? "Aún no hay productos. Crea un producto para empezar el catálogo."
          : "No hay productos que coincidan con el filtro.";
      row.append(cell);
      rows.append(row);
      return;
    }

    for (const product of filtered) {
      const row = document.createElement("tr");
      const skuCell = document.createElement("td");
      const sku = document.createElement("strong");
      sku.className = "table-primary-text";
      sku.textContent = product.sku;
      skuCell.append(sku);

      const description = document.createElement("td");
      description.textContent = product.description;
      const unit = document.createElement("td");
      unit.textContent = product.unitOfMeasure;

      const state = document.createElement("td");
      const badge = document.createElement("span");
      badge.className = `record-badge ${product.active ? "record-active" : "record-inactive"}`;
      badge.textContent = product.active ? "Activo" : "Inactivo";
      state.append(badge);

      const actions = document.createElement("td");
      actions.className = "table-actions";
      const edit = document.createElement("button");
      edit.type = "button";
      edit.className = "text-button";
      edit.textContent = "Ver / editar";
      edit.addEventListener("click", () => void openEditForm(product));
      actions.append(edit);
      if (product.active) {
        const deactivate = document.createElement("button");
        deactivate.type = "button";
        deactivate.className = "text-button danger-text";
        deactivate.textContent = "Desactivar";
        deactivate.addEventListener("click", () => void deactivateProduct(product));
        actions.append(deactivate);
      }
      row.append(skuCell, description, unit, state, actions);
      rows.append(row);
    }
  }

  function openNewForm(): void {
    beginUserIntent();
    editRequestGuard.invalidate();
    formMode = "create";
    editingSku = null;
    form.reset();
    form.hidden = false;
    formTitle.textContent = "Nuevo producto";
    formSubmit.textContent = "Guardar producto";
    formSubmit.disabled = false;
    skuInput.disabled = false;
    formMessage.textContent = "";
    formMessage.dataset.kind = "neutral";
    setStatus("Formulario de nuevo producto listo.");
    skuInput.focus();
  }

  async function openEditForm(product: ProductDto): Promise<void> {
    beginUserIntent();
    form.hidden = true;
    formSubmit.disabled = true;
    setStatus(`Consultando ${product.sku}…`);
    await editRequestGuard.run(
      () => inventoryIpc.getProduct(product.sku),
      (current) => {
        if (!current) {
          setStatus(`No se encontró ${product.sku}. Actualiza el catálogo.`, "error");
          void refresh();
          return;
        }
        formMode = "edit";
        editingSku = current.sku;
        skuInput.value = current.sku;
        skuInput.disabled = true;
        descriptionInput.value = current.description;
        unitInput.value = current.unitOfMeasure;
        form.hidden = false;
        formTitle.textContent = `Editar ${current.sku}`;
        formSubmit.textContent = "Guardar cambios";
        formSubmit.disabled = false;
        formMessage.textContent = current.active
          ? "El SKU identifica el registro y no se puede cambiar."
          : "Este producto está inactivo; editarlo no lo vuelve a activar.";
        formMessage.dataset.kind = "neutral";
        descriptionInput.focus();
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
    if (announceRefresh && editRequestGuard.isCurrent(editIntent)) setStatus("Consultando catálogo…");
    try {
      const nextProducts = await inventoryIpc.listProducts(includeInactive.checked);
      if (requestNumber !== refreshRequestNumber) return { kind: "superseded" };
      products = nextProducts;
      loadFailed = false;
      renderProducts();
      if (announceRefresh && editRequestGuard.isCurrent(editIntent)) {
        setStatus(`${products.length} ${products.length === 1 ? "producto cargado" : "productos cargados"}.`);
      }
      return { kind: "loaded" };
    } catch (error: unknown) {
      if (requestNumber !== refreshRequestNumber) return { kind: "superseded" };
      products = [];
      loadFailed = true;
      renderProducts();
      const message = messageFor(error);
      if (announceRefresh && editRequestGuard.isCurrent(editIntent)) setStatus(message, "error");
      return { kind: "failed", message };
    } finally {
      if (requestNumber === refreshRequestNumber) refreshButton.disabled = false;
    }
  }

  async function deactivateProduct(product: ProductDto): Promise<void> {
    const confirmed = window.confirm(`¿Desactivar el producto ${product.sku}? Se conservarán sus referencias e historial.`);
    if (!confirmed) return;
    editRequestGuard.invalidate();
    const actionIntent = beginUserIntent();
    setStatus(`Desactivando ${product.sku}…`);
    try {
      await inventoryIpc.deactivateProduct(product.sku);
      const result = await refresh({ preserveEditIntent: true });
      if (!isCurrentUserIntent(actionIntent)) return;
      if (result.kind === "loaded" || result.kind === "superseded") {
        setStatus(`Producto ${product.sku} desactivado.`, "success");
      } else if (result.kind === "failed") {
        setStatus(`Producto ${product.sku} desactivado, pero no se pudo actualizar la lista: ${result.message}`, "error");
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

    const sku = skuInput.value.trim();
    const description = descriptionInput.value.trim();
    const unitOfMeasure = unitInput.value.trim();
    formSubmit.disabled = true;
    formMessage.textContent = "Guardando…";
    formMessage.dataset.kind = "neutral";
    setStatus("Guardando producto…");

    const request = formMode === "create"
      ? inventoryIpc.createProduct({ sku, description, unitOfMeasure })
      : inventoryIpc.updateProduct({ sku: editingSku ?? sku, description, unitOfMeasure });

    void request.then(async (product) => {
      if (isCurrentUserIntent(formIntent)) form.hidden = true;
      const result = await refresh({ preserveEditIntent: true });
      if (!isCurrentUserIntent(formIntent)) return;
      if (result.kind === "loaded" || result.kind === "superseded") {
        setStatus(`Producto ${product.sku} guardado.`, "success");
      } else if (result.kind === "failed") {
        setStatus(`Producto ${product.sku} guardado, pero no se pudo actualizar la lista: ${result.message}`, "error");
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
    setStatus("Formulario de producto cerrado.");
    newButton.focus();
  });
  refreshButton.addEventListener("click", () => void refresh());
  includeInactive.addEventListener("change", () => void refresh());
  filter.addEventListener("input", renderProducts);

  return { refresh };
}

