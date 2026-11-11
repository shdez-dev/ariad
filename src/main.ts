import "./views/warehouse-map-view";
import { mountLocationsView } from "./views/locations-view";
import { mountProductsView } from "./views/products-view";
import { mountUnitsView } from "./views/units-view";

document.documentElement.dataset.app = "ariadna";

type ViewName = "map" | "products" | "locations" | "units";

const viewElements = new Map<ViewName, HTMLElement>([
  ["map", requiredElement<HTMLElement>("#view-map")],
  ["products", requiredElement<HTMLElement>("#view-products")],
  ["locations", requiredElement<HTMLElement>("#view-locations")],
  ["units", requiredElement<HTMLElement>("#view-units")],
]);
const navButtons = [...document.querySelectorAll<HTMLButtonElement>("[data-view-button]")];
const productsView = mountProductsView();
const locationsView = mountLocationsView();
const unitsView = mountUnitsView();
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
}

for (const button of navButtons) {
  button.addEventListener("click", () => {
    const view = button.dataset.viewButton as ViewName | undefined;
    if (view && viewElements.has(view)) void activateView(view);
  });
}

window.addEventListener("ariadna:inventory-changed", () => {
  if (activeView === "units") void unitsView.refresh();
});

