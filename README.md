# Ariadna

Prototipo local de escritorio para consultar inventario por ubicación e historial de movimientos en una bodega de demostración. Usa datos sintéticos. No integra robots, lectores físicos, WMS/ERP ni una API HTTP local.

## Estado

ARD-10 proporcionó 17 comandos IPC tipados; ARD-13 agregó `store_unit` para registrar la asignación inicial. ARD-14 añade la búsqueda transversal, la cronología visible de movimientos y la consulta/revisión de alertas persistidas, para un total de 21 comandos. ARD-11 construye el mapa lógico desde las ubicaciones y unidades guardadas. ARD-12 incorpora formularios de catálogo y consulta de unidades. ARD-13 cubre recepción, asignación, traslado y despacho. La interfaz no infiere geometría ni usa hardware. Las anomalías simuladas y su eventual generación de alertas corresponden a ARD-15.

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
- El dominio no depende de Tauri ni de SQLite. La capa de aplicación define el puerto `InventoryRepository` y casos de uso para catálogo, ubicaciones, recepción, movimientos, despacho, búsqueda, consulta de unidades, historial y revisión de alertas.
- SQLx 0.9 y SQLite local ejecutan migraciones versionadas al iniciar la aplicación. La carpeta que entrega `app.path().app_data_dir()` contiene `ariadna.sqlite3` y `logs/ariadna.log`.
- `src-tauri/src/ipc.rs` registra adaptadores delgados con Tauri v2. Usan DTO JSON `camelCase`, códigos de error estables y mensajes seguros; los detalles técnicos quedan en el log local.
- `src/lib/ipc.ts` contiene las funciones tipadas `invoke` para el frontend. La fuente del evento la fija Rust; el frontend conserva el ID y la fecha UTC al reintentar la misma intención para respetar la idempotencia.
- No hay API HTTP de negocio ni permisos generales de shell, filesystem o red habilitados.
- El simulador y la interfaz deberán invocar los mismos casos de uso. El simulador no hablará por HTTP ni MQTT.

Consulta el [contrato IPC](docs/ipc-contract.md), la [línea base de arquitectura](docs/arquitectura-base.md) y la [matriz de trazabilidad](docs/trazabilidad.md).

## Trabajo siguiente

1. Completar revisión de código, verificación final y publicar ARD-14 en GitHub.
2. Cerrar la aceptación visual manual de ARD-11 a ARD-14 en la ventana Tauri.
3. Continuar con ARD-15, el simulador determinista de escenarios.

## Verificación local

```powershell
npm run build
npm run test:map
npm run test:catalog
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

ARD-10 está publicada en `feat/ard-10-tauri-ipc`, commit `672159c`; ARD-11 en `feat/ard-11-location-map`, commit `23ac1ff`; ARD-12 en `feat/ard-12-catalog-units`, commit `37bea39`; ARD-13 en `feat/ard-13-operations`, commit `0e6d095`. ARD-14 se implementa en `feat/ard-14-search-history-alerts`. La verificación en curso cubre búsqueda por SKU, etiqueta y ubicación, cronología completa y revisión idempotente de alertas. Las pruebas automatizadas no sustituyen la aceptación visual manual de las vistas Tauri.

