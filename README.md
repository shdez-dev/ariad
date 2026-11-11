# Ariadna

Prototipo local de escritorio para consultar inventario por ubicación e historial de movimientos en una bodega de demostración. Usa datos sintéticos. No integra robots, lectores físicos, WMS/ERP ni una API HTTP local.

## Estado

ARD-03 a ARD-10 proporcionan el núcleo Rust/SQLite y 17 comandos IPC tipados. ARD-11 construye el mapa lógico desde las ubicaciones y unidades guardadas: agrupa zona, pasillo, rack y posición, muestra disponibilidad y abre el detalle del pallet seleccionado. La vista no infiere geometría ni usa hardware. Los formularios de catálogo y los flujos de recepción, traslado y despacho siguen en tarjetas posteriores.

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
- El dominio no depende de Tauri ni de SQLite. La capa de aplicación define el puerto `InventoryRepository` y casos de uso para catálogo, ubicaciones, recepción, movimientos, despacho, consulta de unidades e historial.
- SQLx 0.9 y SQLite local ejecutan migraciones versionadas al iniciar la aplicación. La carpeta que entrega `app.path().app_data_dir()` contiene `ariadna.sqlite3` y `logs/ariadna.log`.
- `src-tauri/src/ipc.rs` registra adaptadores delgados con Tauri v2. Usan DTO JSON `camelCase`, códigos de error estables y mensajes seguros; los detalles técnicos quedan en el log local.
- `src/lib/ipc.ts` contiene las funciones tipadas `invoke` para el frontend. La fuente del evento la fija Rust; el frontend conserva el ID y la fecha UTC al reintentar la misma intención para respetar la idempotencia.
- No hay API HTTP de negocio ni permisos generales de shell, filesystem o red habilitados.
- El simulador y la interfaz deberán invocar los mismos casos de uso. El simulador no hablará por HTTP ni MQTT.

Consulta el [contrato IPC](docs/ipc-contract.md), la [línea base de arquitectura](docs/arquitectura-base.md) y la [matriz de trazabilidad](docs/trazabilidad.md).

## Trabajo siguiente

1. Cerrar ARD-11 con una inspección visual del mapa en la ventana Tauri.
2. Continuar ARD-12 para configurar y consultar productos, ubicaciones y unidades.
3. Implementar ARD-13 para ejecutar recepción, traslado y despacho desde la interfaz.

## Verificación local

```powershell
npm run build
npm run test:map
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

ARD-10 está publicada en `feat/ard-10-tauri-ipc`, commit `672159c`. ARD-11 está en curso en `feat/ard-11-location-map`. `npm run build`, las 3 pruebas del modelo de mapa, `cargo fmt --check`, las 42 pruebas Rust y Clippy pasan sin advertencias. Las pruebas del mapa cubren jerarquía ordenada, ocupación a partir de unidades almacenadas, estados inactivos, ubicaciones huérfanas y conflicto de ocupación. La app Tauri inicia con una base SQLite temporal limpia. La inspección visual del escritorio y la aceptación de flujos completos quedan pendientes.

