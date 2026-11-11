# Ariadna: matriz de trazabilidad

Estado: actualizada el 11 de noviembre de 2026. ARD-10 está completada. ARD-11, ARD-12 y ARD-13 tienen incrementos publicados y siguen pendientes de aceptación visual manual. ARD-14 está en implementación: búsqueda transversal, cronología visible y revisión de alertas ya persistidas; no incluye generación de escenarios, que corresponde a ARD-15.

## Decisiones técnicas que resuelven pendientes

1. La aplicación es un monolito modular de escritorio con Rust estable y Tauri v2.
2. La interfaz se comunica con la capa de aplicación mediante IPC de Tauri. No se publica una API HTTP, no se abre un puerto de backend y no se integra un WMS, ERP o lector físico.
3. SQLite local con SQLx es la persistencia del prototipo. Las migraciones SQL versionan el esquema.
4. El simulador llama los mismos casos de uso de aplicación que usa la interfaz. El contrato del evento es interno y tipado; no requiere HTTP, MQTT ni un servicio externo.
5. Se usan ubicaciones configuradas manualmente y datos sintéticos. No se afirma que el simulador represente lecturas RFID reales.

Estas decisiones concretan ADR-01 a ADR-07 y DEC-04 a DEC-08. Las versiones 0.2 de Visión y alcance y Requisitos y criterios de aceptación, y la versión 0.9 de Arquitectura y decisiones técnicas registran estas elecciones. ARD-10 define el puente IPC interno; ARD-11 usa ese contrato para mostrar jerarquía y ocupación sin incorporar geometría física ni hardware. Documento AETHER vinculado: Matriz de trazabilidad de requisitos, ID 3a65eb00-0457-4e44-be52-80739fcfc560.

## Requisitos funcionales

| Requisito | Caso de uso o flujo | Componentes previstos | Criterio y evidencia prevista |
| --- | --- | --- | --- |
| RF-01 Configuración de ubicaciones | Crear, consultar, actualizar código/nombre y desactivar zona, pasillo, rack o posición | Casos de uso `CreateLocation`, `GetLocation`, `ListLocations`, `UpdateLocation`, `DeactivateLocation`; dominio `Location`; repositorio SQLite; adaptadores IPC y vista de ubicaciones | ARD-06/10/12: formulario para crear, consultar, actualizar nombre/código y desactivar con confirmación; padre y nivel quedan fijos al editar para preservar la jerarquía histórica; inspección visual pendiente |
| RF-02 Catálogo de productos | Crear, consultar, actualizar y desactivar SKU | Casos de uso `CreateProduct`, `GetProduct`, `ListProducts`, `UpdateProduct`, `DeactivateProduct`; dominio `Product`; SQLite, IPC y vista de productos | ARD-06/10/12: formulario, filtro local, SKU fijo al editar, errores IPC y desactivación lógica conservando referencias; inspección visual pendiente |
| RF-03 Registro de pallet o contenedor | Consultar unidad por ID/etiqueta, con líneas, estado y ubicación | Dominio `HandlingUnit`, `GetHandlingUnit`, `ListHandlingUnits`, `HandlingUnitResponse` e IPC; vista de unidades | ARD-10/12: lista filtrable y consulta del detalle por IPC con etiqueta, estado, ubicación y líneas; recepción/creación operativa queda en ARD-13 y el historial visible en ARD-14; aceptación visual pendiente |
| RF-04 Recepción | Recibir una unidad y dejarla pendiente de ubicación con una o más líneas válidas | `ReceiveUnit`, dominio `HandlingUnit`, líneas, evento y transacción SQLite; formulario de ARD-13 | ARD-07/10/13: recepción atómica, SKU activo, cantidades, rollback, respuesta IPC y captura de SKU/cantidad/lote; aceptación visual pendiente |
| RF-05 Asignación y traslado | Asignar una posición libre a una unidad recibida o mover una almacenada | `StoreUnit`, `MoveUnit`, reglas de ocupación, transacción SQLite y comandos IPC `store_unit` / `move_unit` | ARD-08/10/13: origen persistido, destino activo/libre, idempotencia `applied`/`duplicate` y errores sin mutación; aceptación visual pendiente |
| RF-06 Ingesta de eventos simulados | El simulador entrega un evento tipado directamente a `process_event`; se devuelve aceptado, duplicado o rechazado | Simulador en proceso, capa de aplicación, dominio, repositorios SQLite; IPC solo para la acción de interfaz que inicia el escenario | AC-06, AC-11 a AC-13 y escenarios de RF-14; pruebas deterministas. Sin API HTTP, MQTT ni lector real |
| RF-07 Despacho | Despachar una unidad almacenada, liberar su posición y preservar historial y líneas | `DispatchUnit`, dominio, historial, transacción SQLite, comando IPC y formulario de ARD-13 | ARD-07/10/13: despacho con origen persistido, confirmación y rechazo de transiciones inválidas; aceptación visual pendiente |
| RF-08 Consulta de inventario | Buscar productos por SKU/descripción, unidades por etiqueta/ID/SKU de líneas y ubicaciones por código/nombre/ID | `SearchInventory`, repositorio SQLite, comando `search_inventory` y vista global de búsqueda | ARD-14: coincidencia parcial sin distinguir mayúsculas; pruebas para resultados de productos/unidades/ubicaciones y estados vacío/error |
| RF-09 Vista del mapa lógico | Mostrar jerarquía y ocupación registrada, sin inferencia por sensores | `ListLocations(true)`, `ListHandlingUnits(false)`, modelo TypeScript del mapa y componente de interfaz | ARD-11: jerarquía zona/pasillo/rack/posición, ocupación por unidad almacenada, estados con texto e indicador, selección de posición y consulta IPC del detalle; aceptación visual pendiente |
| RF-10 Historial de movimientos | Consultar cronología por unidad con fecha, tipo, origen, destino, fuente y resultado | `GetMovementHistory`, DTO `MovementHistoryEntry`, repositorio SQLite y comando IPC; panel de cronología en detalle de unidad | ARD-09/10/14: orden estable, intentos rechazados/duplicados y correcciones visibles; cada alerta puede abrir y enfocar su movimiento asociado |
| RF-11 Idempotencia de eventos | Reconocer el identificador ya procesado sin repetir efectos; rechazar su reutilización con otro contenido | `EventOutcome`, clave única, comparación de contenido, `movement_attempts` y transacción SQLite con `BEGIN IMMEDIATE` | ARD-08/09: reintento de evento aceptado devuelve `Duplicate`; conflictos y rechazos persistidos quedan auditados y consumen el ID; validaciones que no llegan a persistencia no consumen el ID |
| RF-12 Validación de etiquetas, ubicaciones y transiciones | Rechazar datos y estados inválidos sin cambios parciales | Validadores de dominio, casos de uso, restricciones SQL y errores IPC | AC-12 y AC-13; pruebas de rechazo y rollback |
| RF-13 Alertas y correcciones trazables | Añadir corrección vinculada sin reescribir eventos; consultar alertas persistidas y marcar las abiertas como revisadas | Dominio `MovementCorrection`, tabla append-only, historial SQLite, tabla `alerts`, casos de uso e IPC de alertas | ARD-09/10/14: el marcado `open` a `reviewed` es idempotente; las alertas se asocian a movimientos aceptados. ARD-15 debe decidir cómo representar anomalías que solo existan como intentos rechazados/duplicados |
| RF-14 Escenarios del simulador | Ejecutar recorrido normal y anomalías repetibles | Simulador con semilla/configuración, procesador de eventos y casos de uso compartidos | ARD-15 y aceptación de la tarjeta; recorridos de recepción, almacenamiento, traslado, despacho, duplicado, etiqueta/ubicación desconocida y destino ocupado |

## Requisitos de calidad

| Requisito | Decisión o componente | Verificación y evidencia prevista |
| --- | --- | --- |
| RQ-01 Rendimiento local | Consultas SQLite acotadas e índices derivados de búsquedas reales | ARD-21: medir p50/p95 en el conjunto RQ-02 y registrar equipo, versión, volumen y método; meta p95 <= 2 s |
| RQ-02 Capacidad de demostración | Datos sintéticos reproducibles: 500 unidades, 250 posiciones y 10.000 eventos | ARD-21: generar el conjunto, medir operaciones clave y revisar consistencia e historial; no es una garantía de producción |
| RQ-03 Persistencia e integridad | Archivo SQLite, migraciones y transacciones para estado más historial | ARD-18: cerrar/reabrir la base y comparar estado; comprobar que un rechazo/fallo no deja cambios parciales |
| RQ-04 Privacidad y seguridad básica | Datos sintéticos, validación Rust, SQL parametrizado y capacidades Tauri mínimas | ARD-19: revisión de configuración y código; confirmar que no se guardan secretos ni se inicia una API local |
| RQ-05 Accesibilidad básica | Etiquetas explícitas, estado no dependiente solo del color, foco visible y teclado | Revisión manual de formularios, mapa, búsqueda, alertas y flujos antes de la versión candidata |
| RQ-06 Compatibilidad | Primera aceptación en Windows x64 documentado; otros sistemas no garantizados | ARD-02/24: registrar Windows, Rust/MSVC, Tauri, Node si aplica y pasos reproducibles |
| RQ-07 Mantenibilidad | Formato, lint, pruebas y compilación Rust documentados | ARD-17, ARD-18 y ARD-20: comandos locales y salida de CI de la revisión candidata |
| RQ-08 Disponibilidad/recuperación productiva | No aplica al prototipo local de un usuario; sin SLA, alta disponibilidad ni DR | Registrar esta exclusión; no simular una aceptación productiva |

## Correspondencia con el backlog

| Trabajo | Tarjetas principales |
| --- | --- |
| Alinear requisitos, decisiones y trazabilidad | ARD-01 |
| Base de escritorio y módulos | ARD-02, ARD-03 |
| Dominio y persistencia | ARD-04, ARD-05 |
| Catálogo y movimientos | ARD-06 a ARD-09 |
| IPC e interfaz | ARD-10 a ARD-14 |
| Simulador, diagnóstico y verificación | ARD-15 a ARD-21 |
| Instalación, operación, aceptación y resultados | ARD-22 a ARD-26 |

## Estado de evidencia

La implementación actual de ARD-14 añade `search_inventory`, `list_alerts` y `mark_alert_reviewed`, mantiene el acceso de la interfaz vía IPC y presenta detalles de catálogo, historial cronológico y alertas. La búsqueda cubre SKU en catálogo y líneas de unidad, etiquetas/IDs y ubicaciones; la revisión de alertas persiste en SQLite y es idempotente. La consulta de alertas solo refleja filas ya persistidas; no crea anomalías. Verificación: `npm run build`, `npm run test:map` (3), `npm run test:catalog` (4), `cargo fmt --check`, 45 pruebas Rust, Clippy con `-D warnings` y `git diff --check` pasan. La inspección visual manual de ARD-11 a ARD-14 sigue pendiente. ARD-15 mantiene el simulador y debe definir cómo enlazar alertas a intentos rechazados o duplicados que no pueden representarse con la clave foránea actual.

La ventana inicial fue observada como evidencia de ARD-02. El perfil local existente presenta un checksum de migración distinto; se dejó intacto y se aisló la prueba de arranque con una base temporal limpia. La aceptación de flujos completos, integración visual, medición de rendimiento y uso real en bodega siguen pendientes.
