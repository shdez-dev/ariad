import "./views/warehouse-map-view";
import { mountAlertsView } from "./views/alerts-view";
import { mountLocationsView } from "./views/locations-view";
import { mountOperationsView } from "./views/operations-view";
import { mountProductsView } from "./views/products-view";
import { mountSearchView } from "./views/search-view";
import { mountSimulatorView } from "./views/simulator-view";
import { mountUnitsView } from "./views/units-view";

document.documentElement.dataset.app = "ariadna";

type ViewName = "map" | "products" | "locations" | "units" | "operations" | "search" | "alerts" | "simulator";

const viewElements = new Map<ViewName, HTMLElement>([
  ["map", requiredElement<HTMLElement>("#view-map")],
  ["products", requiredElement<HTMLElement>("#view-products")],
  ["locations", requiredElement<HTMLElement>("#view-locations")],
  ["units", requiredElement<HTMLElement>("#view-units")],
  ["operations", requiredElement<HTMLElement>("#view-operations")],
  ["search", requiredElement<HTMLElement>("#view-search")],
  ["alerts", requiredElement<HTMLElement>("#view-alerts")],
  ["simulator", requiredElement<HTMLElement>("#view-simulator")],
]);
const navButtons = [...document.querySelectorAll<HTMLButtonElement>("[data-view-button]")];
const productsView = mountProductsView();
const locationsView = mountLocationsView();
const unitsView = mountUnitsView();
const operationsView = mountOperationsView();
const searchView = mountSearchView();
const alertsView = mountAlertsView();
const simulatorView = mountSimulatorView();
let activeView: ViewName = "map";

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`No se encontró el elemento requerido ${selector}.`);
  return element;
}

async function activateView(viewName: ViewName): Promise<void> {
  activeView = viewName;
  for (const [name, element] of viewElements) element.hidden = name !== viewName;
  for (const button of navButtons) {
    const selected = button.dataset.viewButton === viewName;
    button.setAttribute("aria-pressed", String(selected));
    button.classList.toggle("is-current", selected);
  }

  if (viewName === "products") await productsView.refresh();
  if (viewName === "locations") await locationsView.refresh();
  if (viewName === "units") await unitsView.refresh();
  if (viewName === "operations") await operationsView.refresh();
  if (viewName === "search") await searchView.refresh();
  if (viewName === "alerts") await alertsView.refresh();
  if (viewName === "simulator") await simulatorView.refresh();
}

for (const button of navButtons) {
  button.addEventListener("click", () => {
    const view = button.dataset.viewButton as ViewName | undefined;
    if (view && viewElements.has(view)) void activateView(view);
  });
}

window.addEventListener("ariadna:inventory-changed", () => {
  if (activeView === "units") void unitsView.refresh();
  if (activeView === "search") void searchView.refresh();
  if (activeView === "alerts") void alertsView.refresh();
});

window.addEventListener("ariadna:open-unit", (event: Event) => {
  const detail = (event as CustomEvent<{
    handlingUnitId: string;
    recordId?: string;
    movementEventId?: string;
  }>).detail;
  if (!detail?.handlingUnitId) return;
  void (async () => {
    const activation = activateView("units");
    requiredElement<HTMLElement>("#units-title").focus();
    await activation;
    await unitsView.openByIdentifier(detail.handlingUnitId, detail.recordId ?? detail.movementEventId);
  })();
});

window.addEventListener("ariadna:open-alert", (event: Event) => {
  const detail = (event as CustomEvent<{ alertId: string }>).detail;
  if (!detail?.alertId) return;
  void (async () => {
    const activation = activateView("alerts");
    requiredElement<HTMLElement>("#alerts-title").focus();
    await activation;
    alertsView.focusAlert(detail.alertId);
  })();
});

