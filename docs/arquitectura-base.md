# Ariadna: línea base de arquitectura

Estado: línea base de implementación, 10 de noviembre de 2026. Esta decisión concreta las propuestas ADR del documento de Arquitectura y decisiones técnicas de AETHER. El prototipo no está listo para operación productiva.

## Decisiones vigentes

1. **Aplicación:** monolito modular de escritorio con Rust estable y Tauri v2.
2. **Interfaz:** plantilla Vanilla TypeScript; HTML, CSS y recursos compilados a archivos estáticos con Vite. Tauri empaqueta esos recursos.
3. **Comunicación:** las acciones de interfaz llamarán comandos IPC Tauri que delegan en casos de uso Rust. El esqueleto inicial aún no registra comandos.
4. **Persistencia:** SQLite local mediante SQLx 0.9, con migraciones SQL versionadas que se ejecutan al iniciar la aplicación.
5. **Eventos:** el simulador correrá en proceso y usará los mismos casos de uso que la interfaz. No se usará API HTTP ni MQTT en esta etapa.
6. **Datos:** sintéticos y configurados para una bodega lógica. No hay lectores físicos, robótica ni mapeo físico automático.

## Desarrollo local

`src-tauri/tauri.conf.json` no declara `devUrl`. El hook de desarrollo ejecuta `npm run build`; Tauri CLI sirve los recursos estáticos de `dist` en modo desarrollo y el WebView puede mostrar un origen local. No hay servidor Vite separado ni API HTTP de negocio. Para publicación, `frontendDist` se empaqueta con la aplicación. La interfaz se comunica con el backend mediante IPC de Tauri.

## Límites de módulos implementados

`src-tauri/src/domain.rs` contiene SKU, productos, ubicaciones, unidades de manipulación, líneas, eventos y alertas, además de sus validaciones e invariantes. No importa Tauri ni SQLx.

`src-tauri/src/application.rs` define el puerto asíncrono `InventoryRepository`, errores de aplicación y casos de uso para crear, consultar, listar, actualizar y desactivar productos y ubicaciones, además de `ReceiveUnit`, `MoveUnit` y `DispatchUnit`. La recepción valida líneas y construye un evento; el traslado y el despacho construyen eventos contra el origen informado y el repositorio lo compara con el estado persistido. Las operaciones devuelven `EventOutcome::Applied` o `EventOutcome::Duplicate` para distinguir un cambio nuevo de un reintento ya procesado. Al crear una ubicación se carga y valida su padre. La relación jerárquica queda fija después del alta para que los movimientos históricos no cambien de contexto; las actualizaciones posteriores modifican código y nombre, incluso en registros inactivos.

`src-tauri/src/infrastructure/sqlite.rs` implementa el puerto de persistencia, apertura de SQLite, migraciones, operaciones de catálogo y ubicaciones, consultas con inclusión opcional de entidades inactivas y escrituras transaccionales. La recepción crea unidad, líneas y evento en una operación atómica; comprueba que los productos existan y estén activos. El traslado valida el estado y origen, exige una posición destino activa y libre, actualiza la unidad y registra el origen y destino en el historial. El despacho valida el estado y origen, libera la posición y registra el evento. Las escrituras de movimiento comienzan con `BEGIN IMMEDIATE` para serializar la comprobación de idempotencia y evitar resultados inconsistentes entre reintentos concurrentes. Si falla una inserción, se revierte el conjunto completo. Los SKU y códigos únicos se normalizan o restringen mediante el dominio y el esquema. Los registros referenciados se conservan y se desactivan; ubicaciones con descendientes activos o posiciones ocupadas no se desactivan. Un trigger impide cambiar el padre de una ubicación ya creada. Las consultas usan parámetros.

`src-tauri/src/config.rs` deriva las rutas desde `app.path().app_data_dir()`. La base predeterminada es `ariadna.sqlite3`; el registro diagnóstico es `logs/ariadna.log`. Las carpetas se crean si faltan.

La migración `src-tauri/migrations/0001_initial_inventory.sql` crea productos, ubicaciones jerárquicas, unidades, líneas, historial y alertas. Incluye restricciones de SKU, etiqueta y evento; claves foráneas; índices de consulta; validación jerárquica y de ancestros activos; una restricción única para que una posición no tenga dos unidades activas; un trigger que conserva inmutable la relación padre; y triggers que hacen append-only el historial de movimientos.

La persistencia deriva el nuevo estado de la unidad desde el movimiento solicitado y el estado leído dentro de la transacción. Valida origen, destino, estado previo y tipo de movimiento; actualiza unidad e historial como una sola operación. Un `event_id` repetido con igual contenido produce `Duplicate`; el mismo identificador con contenido distinto produce un conflicto explícito. Una posición ocupada y un destino inactivo también se informan con errores tipados. Si el evento no coincide o falla una restricción, se revierte la transacción.

Entorno verificado: Windows 11 Pro 25H2 (10.0.26200.9711, x64), Rust/Cargo 1.99.1 con `stable-x86_64-pc-windows-msvc`, Node.js 24.22.1, npm 11.19.1, Visual Studio Community 2022 con herramientas C++, Windows SDK 10.0.26100.0 y WebView2 Runtime 157.0.4341.42. Smart App Control bloqueó auxiliares de Cargo en el primer intento; cada equipo de desarrollo requiere una configuración de ejecución aprobada.

Comando de inicio:

```powershell
npm run tauri dev
```

## Límites de esta entrega

La interfaz TypeScript/Vite y la aplicación Tauri se compilaron correctamente. La ventana inicial se abrió en Windows x64 y mostró la pantalla de arranque. En el primer intento, Smart App Control bloqueó auxiliares de Cargo; el entorno se ajustó para completar la compilación.

ARD-03 a ARD-08 tienen una base ejecutable y pruebas de módulos, dominio, catálogo, ubicaciones, recepción, despacho y traslados. Los casos de uso del backend persisten y permiten consultar catálogo y ubicaciones; los movimientos funcionan con SQLite e historial transaccional, incluidos reintentos concurrentes y competencia por posiciones. No hay aún comandos IPC de inventario conectados a la interfaz ni pantallas funcionales de catálogo, mapa, recepción, traslados, despacho, búsqueda o simulación. Las pruebas locales verifican reglas y persistencia, pero no representan una operación real ni rendimiento en una bodega.

Verificación ejecutada el 10 de noviembre de 2026: `npm run build`, `cargo test --manifest-path src-tauri/Cargo.toml` (34 pruebas) y `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`; todos pasan.

