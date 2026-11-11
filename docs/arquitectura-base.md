# Ariadna: línea base de arquitectura

Estado: línea base de implementación, 11 de noviembre de 2026. Esta decisión concreta las propuestas ADR del documento de Arquitectura y decisiones técnicas de AETHER. El prototipo no está listo para operación productiva.

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

`src-tauri/src/application.rs` define el puerto asíncrono `InventoryRepository`, errores de aplicación y casos de uso para crear, consultar, listar, actualizar y desactivar productos y ubicaciones, además de `ReceiveUnit`, `MoveUnit`, `DispatchUnit`, `GetMovementHistory`, `RecordCorrection` y `RunSimulation`. El simulador tiene escenarios deterministas de recorrido normal, duplicado, etiqueta desconocida, ubicación inválida y destino ocupado. Utiliza los mismos casos de uso operativos y no crea un almacenamiento paralelo. El historial devuelve movimientos aceptados, intentos duplicados o rechazados y correcciones, ordenados por fecha de ocurrencia y con desempate estable. Una corrección administrativa enlaza un evento previo, registra un motivo y tiene fecha igual o posterior a la del evento corregido; no modifica por sí sola la ubicación ni las existencias. Las operaciones devuelven `EventOutcome::Applied` o `EventOutcome::Duplicate` para distinguir un cambio nuevo de un reintento ya procesado.

`src-tauri/src/infrastructure/sqlite.rs` implementa el puerto de persistencia, apertura de SQLite, migraciones, operaciones de catálogo y ubicaciones, consultas de historial y escrituras transaccionales. La migración `0002_movement_history.sql` agrega una bitácora append-only de intentos duplicados o rechazados y una tabla de correcciones enlazadas a eventos aceptados. La migración `0003_alert_record_target.sql` reconstruye alertas para permitir una clave foránea exclusiva a un evento o a un intento, conservando los registros previos y haciendo inmutable el destino de cada alerta. Las alertas de simulación se insertan en la misma transacción que el evento o intento exacto que las originó. Restricciones y triggers de SQLite comprueban el vínculo a la misma unidad, la fecha de corrección igual o posterior a la del evento original y la ausencia de colisiones de identificadores. La consulta de historial reúne movimientos, intentos y correcciones sin modificar los registros originales. La recepción crea unidad, líneas y evento en una operación atómica; comprueba que los productos existan y estén activos. El traslado valida estado y origen, exige una posición destino activa y libre, actualiza la unidad y registra origen y destino. El despacho libera la posición y registra el evento. Las escrituras comienzan con `BEGIN IMMEDIATE` para serializar la idempotencia. Si falla una inserción, se revierte el conjunto completo. Las consultas usan parámetros.

`src-tauri/src/config.rs` deriva las rutas desde `app.path().app_data_dir()`. La base predeterminada es `ariadna.sqlite3`; el registro diagnóstico es `logs/ariadna.log`. Las carpetas se crean si faltan. ARD-16 conserva como máximo 5 MiB por archivo y tres copias rotativas. En Windows la ruta es `%APPDATA%\\com.ariadna.inventory\\logs\\ariadna.log`.

`src-tauri/src/state.rs` mantiene la configuración, la base SQLite, el repositorio de inventario compartido y el registro diagnóstico en el estado administrado de Tauri. Al iniciar, `lib.rs` crea el repositorio sobre el pool SQLx y registra los comandos mediante `generate_handler!`.

`src-tauri/src/ipc.rs` expone operaciones de catálogo, ubicaciones, consulta de unidades por ID o etiqueta, lista de unidades, recepción, traslado, despacho, corrección, historial, alertas y ejecución del simulador. Convierte los modelos de dominio a DTO serializables y mapea errores a `{ code, message, operationId }`: el mensaje seguro y el ID vuelven al frontend. Cada comando registra resultado, duración e ID de correlación. El detalle del error y los payloads de inventario no se guardan. La fuente de los movimientos de escritorio queda fijada en Rust como `desktop_ui`; el simulador usa `simulator`. La interfaz conserva `eventId` y `occurredAtMs` al reintentar una misma operación. `src/lib/ipc.ts` define los tipos y wrappers de `invoke`. La bandeja abre el `recordId` exacto de una alerta y admite intentos de referencias desconocidas sin ficha de unidad.

La migración `src-tauri/migrations/0001_initial_inventory.sql` crea productos, ubicaciones jerárquicas, unidades, líneas, historial y alertas. Incluye restricciones de SKU, etiqueta y evento; claves foráneas; índices de consulta; validación jerárquica y de ancestros activos; una restricción única para que una posición no tenga dos unidades activas; un trigger que conserva inmutable la relación padre; y triggers que hacen append-only el historial de movimientos.

La persistencia deriva el nuevo estado de la unidad desde el movimiento solicitado y el estado leído dentro de la transacción. Valida origen, destino, estado previo y tipo de movimiento; actualiza unidad e historial como una sola operación. Un `event_id` repetido con igual contenido produce `Duplicate` y deja un intento auditable; el mismo identificador con contenido distinto produce un rechazo auditable y un conflicto explícito. Los rechazos de transición, destino y referencias también quedan registrados sin cambiar la unidad. El identificador de un intento de negocio que se persiste como rechazado queda consumido; tras corregir esa causa se debe usar un `event_id` nuevo. Una validación previa a la persistencia no consume el identificador. Si falla una restricción, se revierte cualquier cambio de estado.

Entorno verificado: Windows 11 Pro 25H2 (10.0.26200.9711, x64), Rust/Cargo 1.99.1 con `stable-x86_64-pc-windows-msvc`, Node.js 24.22.1, npm 11.19.1, Visual Studio Community 2022 con herramientas C++, Windows SDK 10.0.26100.0 y WebView2 Runtime 157.0.4341.42. Smart App Control bloqueó auxiliares de Cargo en el primer intento; cada equipo de desarrollo requiere una configuración de ejecución aprobada.

Comando de inicio:

```powershell
npm run tauri dev
```

## Diagnóstico local de ARD-16

`DiagnosticLog` escribe un objeto JSON por línea con hora Unix en milisegundos, nivel, `operationId`, comando, resultado, duración, código de error y detalle técnico opcional acotado. La implementación actual no escribe detalles dinámicos de SQLite, rutas, semillas ni contenido del inventario. Los errores de IPC llevan el mismo `operationId` visible en el mensaje de la interfaz. El inicio registra la apertura de SQLite y el cierre registra la salida de Tauri. Al alcanzar el límite de 5 MiB se rota el archivo actual y se conservan hasta tres copias. No hay envío ni almacenamiento remoto de telemetría.

ARD-15 añade la vista Simulador para ejecutar los cinco escenarios con una semilla de hasta 80 caracteres. Los pasos son visibles, los datos se marcan como sintéticos y la interfaz permite abrir el registro o la alerta exacta. Los mismos IDs de evento y marcas de tiempo se derivan de la semilla; cada nuevo intento duplicado o rechazado queda append-only con su propio `attempt_id`. La migración preserva las alertas previas. La verificación automática pasa y la aceptación visual manual sigue pendiente.

## Estado de implementación y límites

ARD-10 a ARD-18 se implementaron en incrementos publicados. ARD-15 está en `feat/ard-15-simulator`, commit `b8dbf20`; la migración de alertas mantiene vínculos exactos a eventos e intentos. ARD-16 está publicada en `feat/ard-16-diagnostics`, commit `ecfee57`, y agrega los registros locales descritos arriba. ARD-17, publicada en `feat/ard-17-domain-tests`, commit `1097d5a`, amplía la cobertura unitaria de las reglas del dominio. ARD-18, publicada en `feat/ard-18-sqlite-integration-tests`, commit `7eadb23`, verifica atomicidad y persistencia SQLite. La aceptación visual manual de ARD-11 a ARD-15 continúa pendiente. La aplicación sigue siendo un prototipo local con datos sintéticos, no validado para una operación real de bodega.

Verificación de ARD-17: 65 pruebas Rust, formato, Clippy, compilación TypeScript/Vite y 14 pruebas frontend pasan. La inspección visual manual de las vistas Tauri no forma parte de estas pruebas automatizadas.

## Verificación de ARD-18

ARD-18 añade tres pruebas de integración con SQLite. `failed_receipt_event_insert_rolls_back_unit_and_lines` provoca un error después de insertar la unidad y sus líneas, y verifica que no queden unidad, líneas, evento ni intento parciales. `failed_alert_insert_rolls_back_movement_and_inventory_update` fuerza un fallo al guardar la alerta de un traslado y comprueba que el estado y la ubicación originales se conserven y que no queden evento, alerta ni intento. `inventory_and_history_persist_after_reopening_the_same_database_file` cierra y reabre la misma ruta temporal y comprueba que producto, unidad, líneas, ubicación, movimientos, historial y alerta sigan disponibles.

La cobertura existente ya verifica migraciones desde una base vacía y una base de prueba con esquema anterior, claves únicas, jerarquías, claves foráneas, restricciones, idempotencia y concurrencia. Las bases de prueba son SQLite en memoria o archivos temporales aislados y no requieren servicios externos. En esta revisión pasan 68 pruebas Rust, `cargo fmt --check` y `cargo clippy --all-targets -- -D warnings`. Esta cobertura no sustituye la aceptación visual manual pendiente ni valida el uso productivo.

## Revisión de seguridad de ARD-19

La capacidad `src-tauri/capabilities/default.json` aplica solo a la ventana principal y enumera los 22 comandos de Ariadna. `src-tauri/build.rs` restringe el manifiesto a los mismos comandos para que el frontend no reciba acceso implícito a todas las funciones registradas. Se eliminó `core:default`; no se habilitan plugins de filesystem, shell o red. La compilación genera permisos individuales bajo `src-tauri/permissions/autogenerated`.

La Content Security Policy de Tauri limita scripts, estilos, imágenes y fuentes al contenido local, permite el transporte IPC propio de Tauri y bloquea objetos. No se permiten hosts externos. El frontend construye elementos con APIs DOM y presenta datos mediante `textContent`; no usa `innerHTML` ni `document.write` con entradas de inventario.

Las consultas de SQLite usan SQL estático con parámetros enlazados por SQLx. Los casos de uso y el dominio validan entradas antes de persistir. ARD-19 fija un máximo de 120 caracteres para una búsqueda y un código de lote, y de 100 líneas por unidad recibida. El adaptador IPC comprueba el límite de líneas antes de convertirlas y la entidad vuelve a proteger el invariante.

La revisión no encontró API HTTP de negocio, listener TCP, credenciales ni dependencias directas de servidor HTTP, cliente HTTP, filesystem de Tauri o shell de Tauri. El informe detallado y sus referencias técnicas están en `docs/revision-seguridad-ARD-19.md`. Pasan 71 pruebas Rust, 14 pruebas frontend, compilación TypeScript/Vite, formato, Clippy y compilación Tauri de producción con `--no-bundle`. Una comprobación confirma que los comandos del registro, manifiesto y capacidad coinciden en los 22 nombres. La inspección visual manual de ARD-11 a ARD-15 continúa pendiente.

