# Ariadna

Prototipo local de escritorio para consultar inventario por ubicación e historial de movimientos en una bodega de demostración. Usa datos sintéticos. No integra robots, lectores físicos, WMS/ERP ni una API HTTP local.

## Estado

ARD-10 proporcionó los comandos IPC de catálogo y movimientos; ARD-14 añadió búsqueda, historial y revisión de alertas. ARD-15 incorpora un simulador en proceso con semilla y escenarios reproducibles, persistidos mediante los mismos casos de uso y SQLite. ARD-16 añade registros JSON Lines correlacionados para inicio, cierre y comandos IPC, con duración, códigos de error seguros y rotación limitada. ARD-17 amplía las pruebas unitarias del dominio y ARD-18 añade cobertura de rollback y persistencia tras reabrir SQLite. ARD-19 limita los permisos de Tauri a 22 comandos, añade una CSP restrictiva y valida límites de búsqueda y recepción en Rust. ARD-20 configura CI en Windows para pruebas de frontend y Rust, formato, Clippy y compilación Tauri. ARD-21 incorpora un benchmark local reproducible y documenta latencias con datos sintéticos. ARD-22 genera un instalador NSIS sin firma y verifica en CI su instalación, inicio, base local y desinstalación. No se usan robots ni lectores físicos.

## Requisitos del entorno

- Windows 10/11 x64
- Rust estable con toolchain `x86_64-pc-windows-msvc`
- Microsoft C++ Build Tools y Windows SDK
- WebView2 Runtime
- Node.js y npm para compilar la interfaz TypeScript/Vite

Entorno verificado: Windows 11 Pro 25H2 (10.0.26200.9711, x64), Rust/Cargo 1.99.1 con `stable-x86_64-pc-windows-msvc`, Node.js 24.22.1, npm 11.19.1, Visual Studio Community 2022 con herramientas C++ y Windows SDK 10.0.26100.0, WebView2 Runtime 157.0.4341.42.

## Preparar y ejecutar

Desde la carpeta del proyecto:

```powershell
npm install
npm run tauri dev
```

La configuración construye la interfaz estática y después abre la ventana Tauri. No configura `devUrl`, servidor Vite ni una API HTTP de negocio. En modo desarrollo, Tauri CLI sirve los recursos estáticos de `dist` y el WebView puede mostrar un origen local; no es un backend de inventario. En el bundle de producción, los recursos de `frontendDist` se empaquetan con la aplicación. Para volver a compilar solo la interfaz:

```powershell
npm run build
```

Comprobación de compilación Rust, una vez que Windows permita ejecutar los procesos de compilación:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
```

Si aparece `An Application Control policy has blocked this file (os error 4551)`, revisa el evento 3077 en `Microsoft-Windows-CodeIntegrity/Operational`. Smart App Control no tiene una excepción por aplicación para estos binarios de desarrollo. Usa solo un entorno de desarrollo permitido; en equipos administrados, consulta a quien administra Windows.

## Arquitectura inicial

- Rust estable y Tauri v2 para la aplicación de escritorio.
- HTML, CSS y TypeScript empaquetados como recursos estáticos.
- El dominio no depende de Tauri ni de SQLite. La capa de aplicación define el puerto `InventoryRepository` y casos de uso para catálogo, ubicaciones, recepción, movimientos, despacho, búsqueda, consulta de unidades, historial, revisión de alertas y simulación.
- SQLx 0.9 y SQLite local ejecutan migraciones versionadas al iniciar la aplicación. La migración 0003 permite que una alerta apunte, con clave foránea, a un evento aceptado o a un intento duplicado/rechazado, conservando las alertas existentes.
- `src-tauri/src/ipc.rs` registra adaptadores delgados con Tauri v2. Usan DTO JSON `camelCase`, códigos de error estables y mensajes seguros; cada operación recibe un ID de diagnóstico que se muestra si falla. El registro local solo almacena contexto técnico limitado.
- En Windows, los registros JSON Lines se guardan bajo `%APPDATA%\\com.ariadna.inventory\\logs\\ariadna.log`. Cada archivo tiene un límite de 5 MiB y conserva hasta tres copias rotativas (`ariadna.log.1` a `.3`). No se envían registros a servicios externos.
- `src/lib/ipc.ts` contiene las funciones tipadas `invoke` para el frontend. La fuente del evento la fija Rust; el frontend conserva el ID y la fecha UTC al reintentar la misma intención para respetar la idempotencia.
- No hay API HTTP de negocio ni permisos generales de shell, filesystem o red habilitados.
- El simulador invoca los casos de uso de recepción, asignación, traslado y despacho; no tiene persistencia paralela y no habla por HTTP ni MQTT. Una misma semilla genera los mismos IDs de evento y tiempos. Las filas de intentos son append-only y cada ejecución de una anomalía puede añadir un intento y alerta nuevos.

Consulta el [contrato IPC](docs/ipc-contract.md), la [línea base de arquitectura](docs/arquitectura-base.md) y la [matriz de trazabilidad](docs/trazabilidad.md).

El workflow de integración continua valida cada push y pull request en Windows. ARD-22 añade la generación del instalador NSIS, prueba aislada de instalación e inicio, verificación de la base local y desinstalación, además de publicar instalador e informe como artefacto temporal de Actions. Consulta la [guía de CI](docs/continuous-integration.md) y la [guía del instalador Windows](docs/distribution/windows-installer.md) para los chequeos y ubicaciones.

## Trabajo siguiente

1. Repetir la carga del artefacto de ARD-22, inspeccionar el instalador actual y cotejar su hash, informe y firma.
2. Completar y registrar los criterios visuales pendientes de ARD-11 a ARD-15 en la ventana Tauri: teclado, CRUD/errores/estados vacíos, rechazos operativos y escenarios del simulador.
3. Ejecutar ARD-23: documentar operación, copia y restauración de SQLite, y validar el procedimiento con una restauración de prueba.
4. Ejecutar ARD-24: preparar la guía pública de instalación y demostración para portfolio con límites y versiones verificables.
5. Ejecutar ARD-25: aceptar la versión candidata en instalación limpia y dejar registro de cambios y defectos remanentes.
6. Ejecutar ARD-26: consolidar mediciones reproducibles, marcar métricas sin piloto real como pendientes y proponer mejoras basadas en evidencia.

## Verificación local

```powershell
npm run build
npm run test:map
npm run test:catalog
npm run test:simulator
npm run test:ipc
npm run test:accessibility
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run tauri -- build
cargo run --release --manifest-path src-tauri/Cargo.toml --example ard21_benchmark
```

La última orden crea una base y un informe sintéticos en la carpeta temporal de Windows. Consulta [la línea base de rendimiento de ARD-21](docs/performance/ard-21-baseline.md) para metodología, resultados y limitaciones.

ARD-10 está publicada en `feat/ard-10-tauri-ipc`, commit `672159c`; ARD-11 en `feat/ard-11-location-map`, commit `23ac1ff`; ARD-12 en `feat/ard-12-catalog-units`, commit `37bea39`; ARD-13 en `feat/ard-13-operations`, commit `0e6d095`; ARD-14 en `feat/ard-14-search-history-alerts`, commit `a1925d2`; ARD-15 en `feat/ard-15-simulator`, commit `b8dbf20`; ARD-16 en `feat/ard-16-diagnostics`, commit `ecfee57`; ARD-17 en `feat/ard-17-domain-tests`, commit `1097d5a`; ARD-18 en `feat/ard-18-sqlite-integration-tests`, commit `7eadb23`; ARD-19 en `feat/ard-19-tauri-security`, commit `8b5ea7c`; ARD-20 en `feat/ard-20-ci`, commit `202772a`; ARD-21 en `feat/ard-21-performance`, commit `3acb839`. La inspección visual manual de las vistas Tauri continúa pendiente; las pruebas automatizadas no la sustituyen.
