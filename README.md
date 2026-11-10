# Ariadna

Prototipo local de escritorio para consultar inventario por ubicación e historial de movimientos en una bodega de demostración. Usa datos sintéticos. No integra robots, lectores físicos, WMS/ERP ni una API HTTP local.

## Estado

La estructura de Tauri v2 y la pantalla inicial están creadas. La compilación de TypeScript/Vite y la compilación nativa terminan correctamente; la ventana Tauri se abrió y se verificó en Windows x64. En el primer intento, Smart App Control bloqueó binarios auxiliares de Cargo (evento Code Integrity 3077); el entorno de desarrollo se ajustó para poder compilar. La pantalla no consulta ni guarda inventario. Dominio, SQLite, comandos IPC y flujos funcionales siguen pendientes según ARD-03 en adelante.

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
- Sin comandos IPC registrados todavía. Cuando existan, serán la frontera entre interfaz y casos de uso Rust.
- SQLite con SQLx es la decisión de persistencia para las siguientes tarjetas; aún no está incorporada al código.
- El simulador y la interfaz deberán invocar los mismos casos de uso. El simulador no hablará por HTTP ni MQTT.

Consulta [la línea base de arquitectura](docs/arquitectura-base.md) y [la matriz de trazabilidad](docs/trazabilidad.md).

## Trabajo siguiente

1. Cerrar ARD-01 sincronizando los documentos de AETHER con las decisiones técnicas y manteniendo actualizada la matriz de trazabilidad.
2. Completar ARD-03 definiendo límites de módulos, configuración y manejo de errores.
3. Implementar el dominio y las migraciones SQLite en ARD-04 y ARD-05.

