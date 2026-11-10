# Ariadna

Prototipo local de escritorio para consultar inventario por ubicación e historial de movimientos en una bodega de demostración. Usa datos sintéticos. No integra robots, lectores físicos, WMS/ERP ni una API HTTP local.

## Estado

La aplicación Tauri v2 y la pantalla inicial están creadas. ARD-03 a ARD-08 añaden módulos de dominio y aplicación, catálogo y ubicaciones, recepción, despacho y traslados con persistencia SQLite, idempotencia de eventos, configuración local y registro diagnóstico. La interfaz todavía no consulta ni guarda inventario y aún no registra comandos IPC de negocio. La aceptación de flujos gráficos continúa pendiente.

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
- El dominio no depende de Tauri ni de SQLite. La capa de aplicación define el puerto `InventoryRepository` y el caso de uso inicial `CreateProduct`.
- SQLx 0.9 y SQLite local ejecutan migraciones versionadas al iniciar la aplicación. La carpeta que entrega `app.path().app_data_dir()` contiene `ariadna.sqlite3` y `logs/ariadna.log`.
- La adaptación de errores separa el mensaje seguro para la interfaz del detalle que se conserva en el registro local.
- No hay comandos IPC de negocio conectados a pantallas todavía.
- El simulador y la interfaz deberán invocar los mismos casos de uso. El simulador no hablará por HTTP ni MQTT.

Consulta [la línea base de arquitectura](docs/arquitectura-base.md) y [la matriz de trazabilidad](docs/trazabilidad.md).

## Trabajo siguiente

1. Completar historial y correcciones (ARD-09).
2. Exponer los casos de uso mediante IPC (ARD-10) y conectarlos con las pantallas previstas.

## Verificación local

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

El incremento actual pasó la compilación de interfaz, 34 pruebas Rust y Clippy sin advertencias. Las pruebas cubren validaciones de dominio, operaciones de catálogo y ubicaciones, jerarquía y ancestros activos, ocupación, desactivación sin borrar referencias, recepción atómica, SKU y cantidades inválidas, despacho con liberación de posición, traslados con origen validado y registro de origen/destino, rechazos por destino ocupado o inactivo, idempotencia secuencial y concurrente, competencia por una posición, rollback ante fallo del evento, historial inmutable, migraciones y copia de base.

