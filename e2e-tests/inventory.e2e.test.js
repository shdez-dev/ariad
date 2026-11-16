import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { Builder, By, Capabilities, Key, until } from "selenium-webdriver";
import { Select } from "selenium-webdriver/lib/select.js";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const application = process.env.ARIADNA_APP_BINARY
  ?? resolve(repositoryRoot, "src-tauri", "target", "release", "ariadna.exe");
const webdriverUrl = "http://127.0.0.1:4444/";
const artifactsDirectory = process.env.ARIADNA_E2E_ARTIFACTS;

async function waitForDriver(child, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  let startupError;
  child.once("error", (error) => {
    startupError = error;
  });

  while (Date.now() < deadline) {
    if (startupError) throw startupError;
    if (child.exitCode !== null) {
      throw new Error(`tauri-driver terminó antes de iniciar (código ${child.exitCode}).`);
    }
    try {
      const response = await fetch(`${webdriverUrl}status`, { signal: AbortSignal.timeout(500) });
      if (response.ok) return;
    } catch {
      // tauri-driver aún puede estar iniciando.
    }
    await new Promise((done) => setTimeout(done, 250));
  }
  throw new Error("tauri-driver no abrió el endpoint WebDriver dentro de 30 segundos.");
}

async function saveScreenshot(driver, name) {
  if (!artifactsDirectory) return;
  await mkdir(artifactsDirectory, { recursive: true });
  await writeFile(resolve(artifactsDirectory, name), Buffer.from(await driver.takeScreenshot(), "base64"));
}

test("Tauri integra el recorrido normal, el historial y una alerta revisable", async (t) => {
  const driverProcess = spawn("tauri-driver", [], { stdio: ["ignore", "inherit", "inherit"] });
  let driver;
  t.after(async () => {
    try {
      await driver?.quit();
    } finally {
      if (driverProcess.exitCode === null && driverProcess.signalCode === null) {
        driverProcess.kill();
        await Promise.race([once(driverProcess, "exit"), new Promise((done) => setTimeout(done, 3_000))]);
      }
    }
  });

  try {
    await waitForDriver(driverProcess);
    const capabilities = new Capabilities();
    capabilities.set("tauri:options", { application });
    capabilities.setBrowserName("wry");
    driver = await new Builder()
      .usingServer(webdriverUrl)
      .withCapabilities(capabilities)
      .build();
    await driver.manage().setTimeouts({ implicit: 0, pageLoad: 30_000, script: 30_000 });

    await driver.wait(async () => {
      const mapPanel = await driver.findElement(By.css("#map-panel"));
      return (await mapPanel.getAttribute("aria-busy")) !== "true";
    }, 30_000, "El mapa inicial debe terminar de cargar");
    assert.equal(
      await driver.findElement(By.css(".primary-nav")).getAttribute("aria-label"),
      "Secciones de Ariadna",
    );

    const simulatorNav = driver.findElement(By.css('[data-view-button="simulator"]'));
    await simulatorNav.sendKeys(Key.ENTER);
    await driver.wait(async () => (await simulatorNav.getAttribute("aria-pressed")) === "true", 5_000);
    await driver.wait(async () => await driver.findElement(By.css("#view-simulator")).isDisplayed(), 5_000);

    const runId = process.env.GITHUB_RUN_ID ?? `local-${Date.now()}`;
    const runAttempt = process.env.GITHUB_RUN_ATTEMPT ?? "1";
    const normalSeed = `ARIADNA-E2E-${runId}-${runAttempt}`;
    await driver.findElement(By.css("#simulator-seed")).clear();
    await driver.findElement(By.css("#simulator-seed")).sendKeys(normalSeed);
    await new Select(await driver.findElement(By.css("#simulator-scenario"))).selectByValue("normal");
    await driver.findElement(By.css("#simulator-run")).click();
    await driver.wait(async () => (
      await driver.findElement(By.css("#simulator-status")).getAttribute("data-kind")
    ) === "success", 45_000, "El recorrido normal debe completarse por IPC");

    const steps = await driver.findElements(By.css("#simulator-steps [data-simulation-step-id]"));
    assert.equal(steps.length, 4, "el recorrido normal debe registrar recepción, ubicación, traslado y despacho");
    assert.match(await driver.findElement(By.css("#simulator-status")).getText(), /datos sintéticos/i);
    for (const step of steps) assert.match(await step.getText(), /Dato sintético/);
    await saveScreenshot(driver, "ariadna-tauri-e2e-normal.png");

    await steps[0].findElement(By.css("button")).click();
    await driver.wait(async () => await driver.findElement(By.css("#view-units")).isDisplayed(), 5_000);
    await driver.wait(async () => (
      await driver.findElements(By.css("#unit-details .movement-history-entry"))
    ).length === 4, 30_000, "La unidad debe abrirse con sus cuatro movimientos persistidos");
    const unitDetail = await driver.findElement(By.css("#unit-details")).getText();
    assert.match(unitDetail, /Estado\s+Despachado/);
    assert.match(unitDetail, /Ubicación\s+Sin ubicación/);
    assert.match(unitDetail, /Fecha simulada/);
    assert.match(unitDetail, /registrado/);

    await simulatorNav.click();
    await driver.wait(async () => await driver.findElement(By.css("#view-simulator")).isDisplayed(), 5_000);
    const anomalySeed = `${normalSeed}-ANOMALY`;
    await driver.findElement(By.css("#simulator-seed")).clear();
    await driver.findElement(By.css("#simulator-seed")).sendKeys(anomalySeed);
    await new Select(await driver.findElement(By.css("#simulator-scenario"))).selectByValue("unknown_label");
    await driver.findElement(By.css("#simulator-run")).click();
    await driver.wait(async () => (
      await driver.findElement(By.css("#simulator-status")).getAttribute("data-kind")
    ) === "success", 45_000, "La anomalía debe quedar registrada por IPC");
    const anomalyStep = driver.findElement(By.css("#simulator-steps [data-simulation-step-id]"));
    const anomalyActions = await anomalyStep.findElements(By.css("button"));
    assert.equal(anomalyActions.length, 2);
    assert.equal(await anomalyActions[0].getText(), "Mostrar registro exacto");
    await anomalyActions[0].click();
    const attemptDetail = await driver.findElement(By.css("#simulator-record-detail")).getText();
    assert.match(attemptDetail, /REGISTRO SINTÉTICO/);
    assert.match(attemptDetail, /attempt:/);
    assert.match(attemptDetail, /Sin unidad persistida/);
    await anomalyActions[1].click();

    await driver.wait(async () => await driver.findElement(By.css("#view-alerts")).isDisplayed(), 5_000);
    const alertCard = await driver.wait(
      until.elementLocated(By.css('#alerts-list [data-alert-id]')),
      30_000,
      "La alerta sintética debe aparecer en la bandeja",
    );
    await driver.wait(async () => await alertCard.isDisplayed(), 5_000);
    const alertText = await alertCard.getText();
    assert.match(alertText, /Etiqueta desconocida/);
    assert.match(alertText, /Intento de movimiento/);
    assert.match(alertText, /Registro exacto/);

    await alertCard.findElement(By.css('[data-alert-action="open-event"]')).click();
    await driver.wait(async () => (
      await driver.findElements(By.css("#alerts-list [data-alert-record-preview]"))
    ).length === 1, 5_000, "El intento rechazado debe abrir su registro exacto sin ficha de unidad");
    const exactRecord = await driver.findElement(By.css("#alerts-list [data-alert-record-preview]")).getText();
    assert.match(exactRecord, /Registro exacto sin ficha de unidad/);
    assert.match(exactRecord, /Identificador:\s+attempt:/);

    await driver.findElement(By.css('#alerts-list [data-alert-action="review"]')).click();
    await driver.wait(async () => (
      await driver.findElement(By.css("#alerts-list .alert-status-reviewed")).getText()
    ) === "Revisada", 15_000, "La revisión debe persistirse en SQLite");
    assert.equal((await driver.findElements(By.css('#alerts-list [data-alert-action="review"]'))).length, 0,
      "una alerta revisada no debe ofrecer de nuevo la misma acción");
    await saveScreenshot(driver, "ariadna-tauri-e2e-alert.png");
  } catch (error) {
    if (driver) await saveScreenshot(driver, "ariadna-tauri-e2e-failure.png").catch(() => undefined);
    throw error;
  }
});
