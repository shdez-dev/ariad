# Ariadna: línea base de arquitectura

Estado: línea base de implementación, 10 de noviembre de 2026. Esta decisión concreta las propuestas ADR del documento de Arquitectura y decisiones técnicas de AETHER. El prototipo no está listo para operación productiva.

## Decisiones vigentes

1. **Aplicación:** monolito modular de escritorio con Rust estable y Tauri v2.
2. **Interfaz:** plantilla Vanilla TypeScript; HTML, CSS y recursos compilados a archivos estáticos con Vite. Tauri empaqueta esos recursos.
3. **Comunicación:** las acciones de interfaz llaman comandos IPC Tauri registrados en `src-tauri/src/lib.rs`. Los adaptadores viven en `src-tauri/src/ipc.rs`, delegan en casos de uso Rust y exponen respuestas JSON `camelCase`; no existe API HTTP de negocio.
4. **Persistencia:** SQLite local mediante SQLx 0.9, con migraciones SQL versionadas que se ejecutan al iniciar la aplicación.
5. **Eventos:** el simulador correrá en proceso y usará los mismos casos de uso que la interfaz. No se usará API HTTP ni MQTT en esta etapa.
6. **Datos:** sintéticos y configurados para una bodega lógica. No hay lectores físicos, robótica ni mapeo físico automático.

## Desarrollo local

`src-tauri/tauri.conf.json` no declara `devUrl`. El hook de desarrollo ejecuta `npm run build`; Tauri CLI sirve los recursos estáticos de `dist` en modo desarrollo y el WebView puede mostrar un origen local. No hay servidor Vite separado ni API HTTP de negocio. Para publicación, `frontendDist` se empaqueta con la aplicación. La interfaz se comunica con el backend mediante IPC de Tauri.

## Límites de módulos implementados

`src-tauri/src/domain.rs` contiene SKU, productos, ubicaciones, unidades de manipulación, líneas, eventos, correcciones y alertas, además de sus validaciones e invariantes. No importa Tauri ni SQLx.

`src-tauri/src/application.rs` define el puerto asíncrono `InventoryRepository`, errores de aplicación y casos de uso para crear, consultar, listar, actualizar y desactivar productos y ubicaciones, además de `ReceiveUnit`, `MoveUnit`, `DispatchUnit`, `GetMovementHistory` y `RecordCorrection`. El historial devuelve movimientos aceptados, intentos duplicados o rechazados y correcciones, ordenados por fecha de ocurrencia y con desempate estable. Una corrección administrativa enlaza un evento previo, registra un motivo y tiene fecha igual o posterior a la del evento corregido; no modifica por sí sola la ubicación ni las existencias. Si se necesita cambiar el estado actual, se registra un movimiento compensatorio válido. Las operaciones devuelven `EventOutcome::Applied` o `EventOutcome::Duplicate` para distinguir un cambio nuevo de un reintento ya procesado.

`src-tauri/src/infrastructure/sqlite.rs` implementa el puerto de persistencia, apertura de SQLite, migraciones, operaciones de catálogo y ubicaciones, consultas de historial y escrituras transaccionales. La migración `0002_movement_history.sql` agrega una bitácora append-only de intentos duplicados o rechazados y una tabla de correcciones enlazadas a eventos aceptados. Restricciones y triggers de SQLite comprueban el vínculo a la misma unidad, la fecha de corrección igual o posterior a la del evento original y la ausencia de colisiones de identificadores. La consulta de historial reúne movimientos, intentos y correcciones sin modificar los registros originales. La recepción crea unidad, líneas y evento en una operación atómica; comprueba que los productos existan y estén activos. El traslado valida estado y origen, exige una posición destino activa y libre, actualiza la unidad y registra origen y destino. El despacho libera la posición y registra el evento. Las escrituras comienzan con `BEGIN IMMEDIATE` para serializar la idempotencia. Si falla una inserción, se revierte el conjunto completo. Las consultas usan parámetros.

`src-tauri/src/config.rs` deriva las rutas desde `app.path().app_data_dir()`. La base predeterminada es `ariadna.sqlite3`; el registro diagnóstico es `logs/ariadna.log`. Las carpetas se crean si faltan.

`src-tauri/src/state.rs` mantiene la configuración, la base SQLite, el repositorio de inventario compartido y el registro diagnóstico en el estado administrado de Tauri. Al iniciar, `lib.rs` crea el repositorio sobre el pool SQLx y registra los comandos mediante `generate_handler!`.

`src-tauri/src/ipc.rs` expone operaciones de catálogo, ubicaciones, consulta de unidades por ID o etiqueta, lista de unidades, recepción, traslado, despacho, corrección e historial. Convierte los modelos de dominio a DTO serializables y mapea errores a `{ code, message }`: el mensaje seguro vuelve al frontend y el detalle se registra localmente. La fuente operativa queda fijada en Rust como `desktop_ui`. La interfaz conserva `eventId` y `occurredAtMs` al reintentar una misma operación para que la comparación idempotente use el mismo contenido. `src/lib/ipc.ts` define los tipos y wrappers de `invoke`. Las búsquedas combinadas y alertas siguen en ARD-14.

La migración `src-tauri/migrations/0001_initial_inventory.sql` crea productos, ubicaciones jerárquicas, unidades, líneas, historial y alertas. Incluye restricciones de SKU, etiqueta y evento; claves foráneas; índices de consulta; validación jerárquica y de ancestros activos; una restricción única para que una posición no tenga dos unidades activas; un trigger que conserva inmutable la relación padre; y triggers que hacen append-only el historial de movimientos.

La persistencia deriva el nuevo estado de la unidad desde el movimiento solicitado y el estado leído dentro de la transacción. Valida origen, destino, estado previo y tipo de movimiento; actualiza unidad e historial como una sola operación. Un `event_id` repetido con igual contenido produce `Duplicate` y deja un intento auditable; el mismo identificador con contenido distinto produce un rechazo auditable y un conflicto explícito. Los rechazos de transición, destino y referencias también quedan registrados sin cambiar la unidad. El identificador de un intento de negocio que se persiste como rechazado queda consumido; tras corregir esa causa se debe usar un `event_id` nuevo. Una validación previa a la persistencia no consume el identificador. Si falla una restricción, se revierte cualquier cambio de estado.

Entorno verificado: Windows 11 Pro 25H2 (10.0.26200.9711, x64), Rust/Cargo 1.99.1 con `stable-x86_64-pc-windows-msvc`, Node.js 24.22.1, npm 11.19.1, Visual Studio Community 2022 con herramientas C++, Windows SDK 10.0.26100.0 y WebView2 Runtime 157.0.4341.42. Smart App Control bloqueó auxiliares de Cargo en el primer intento; cada equipo de desarrollo requiere una configuración de ejecución aprobada.

Comando de inicio:

```powershell
npm run tauri dev
```

## Límites de esta entrega

La interfaz TypeScript/Vite y la aplicación Tauri se compilaron correctamente. La ventana inicial se abrió en Windows x64 y mostró la pantalla de arranque. En el primer intento, Smart App Control bloqueó auxiliares de Cargo; el entorno se ajustó para completar la compilación.

ARD-03 a ARD-10 tienen una base ejecutable; ARD-10 expone comandos IPC registrados, DTO serializables, errores estructurados y consultas de unidad. ARD-11 implementa el mapa lógico consultando ubicaciones y unidades por IPC. La vista construye la jerarquía desde `parentId`, calcula ocupación con unidades almacenadas, muestra estados libre/ocupada/inactiva y consulta el detalle del pallet al seleccionar una posición. Los cambios confirmados de recepción, traslado o despacho emiten un evento local para actualizar el mapa. Las 42 pruebas Rust cubren el núcleo y las 3 pruebas TypeScript cubren el modelo del mapa; build, formato y Clippy pasan. Se verificó el arranque de Tauri con una base temporal limpia. Formularios de catálogo, flujos operativos completos, búsqueda, alertas y simulación siguen pendientes. Estas pruebas no representan una operación real ni rendimiento en una bodega.

Verificación ejecutada el 10 de noviembre de 2026: `npm run build`, `cargo fmt --check`, `cargo test --manifest-path src-tauri/Cargo.toml` (37 pruebas) y `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`; todos pasan.

