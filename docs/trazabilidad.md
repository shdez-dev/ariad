# Ariadna: matriz de trazabilidad

Estado: actualizada el 10 de noviembre de 2026. Refleja ARD-03 a ARD-10 publicadas y ARD-11 en curso. Distingue el IPC y el mapa lógico ya implementados de los formularios, flujos operativos y aceptación gráfica pendientes.

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
| RF-01 Configuración de ubicaciones | Crear, consultar, actualizar código/nombre y desactivar zona, pasillo, rack o posición | Casos de uso `CreateLocation`, `GetLocation`, `ListLocations`, `UpdateLocation`, `DeactivateLocation`; dominio `Location`; repositorio SQLite; adaptadores IPC | ARD-06/10: jerarquía, código único, actualización, padre inmutable y desactivación segura; comandos disponibles, formulario pendiente |
| RF-02 Catálogo de productos | Crear, consultar, actualizar y desactivar SKU | Casos de uso `CreateProduct`, `GetProduct`, `ListProducts`, `UpdateProduct`, `DeactivateProduct`; dominio `Product`; SQLite e IPC | ARD-06/10: normalización, conflicto, consulta, actualización, filtros y desactivación conservando referencias; pantalla pendiente |
| RF-03 Registro de pallet o contenedor | Crear y consultar unidad por ID/etiqueta, con líneas y estado | Dominio `HandlingUnit`, `GetHandlingUnit`, `ListHandlingUnits`, `HandlingUnitResponse` e IPC | ARD-10: consulta por identificador o etiqueta sin distinguir mayúsculas, líneas y estado; pantalla de unidades pendiente |
| RF-04 Recepción | Recibir una unidad y dejarla pendiente de ubicación con una o más líneas válidas | Caso de uso `ReceiveUnit`, dominio `HandlingUnit`, líneas, evento de recepción, transacción SQLite y comando IPC | ARD-07/10: recepción atómica, SKU activo, cantidades, rollback y DTO de resultado; flujo visual pendiente |
| RF-05 Asignación y traslado | Asignar posición libre o mover entre posiciones libres | Caso de uso `MoveUnit`, reglas de ocupación, transacción SQLite y comando IPC | ARD-08/10: pruebas de traslado, DTO `applied`/`duplicate` y errores; flujo visual pendiente |
| RF-06 Ingesta de eventos simulados | El simulador entrega un evento tipado directamente a `process_event`; se devuelve aceptado, duplicado o rechazado | Simulador en proceso, capa de aplicación, dominio, repositorios SQLite; IPC solo para la acción de interfaz que inicia el escenario | AC-06, AC-11 a AC-13 y escenarios de RF-14; pruebas deterministas. Sin API HTTP, MQTT ni lector real |
| RF-07 Despacho | Despachar una unidad almacenada, liberar su posición y preservar historial y líneas | Caso de uso `DispatchUnit`, dominio, historial, transacción SQLite y comando IPC | ARD-07/10: despacho, origen/estado incorrectos y reutilización de posición; interfaz pendiente |
| RF-08 Consulta de inventario | Buscar por identificador, etiqueta, SKU, zona o estado | Consultas de aplicación, repositorios SQLite e interfaz de búsqueda | AC-08; pruebas de filtros, resultados y estado vacío |
| RF-09 Vista del mapa lógico | Mostrar jerarquía y ocupación registrada, sin inferencia por sensores | `ListLocations(true)`, `ListHandlingUnits(false)`, modelo TypeScript del mapa y componente de interfaz | ARD-11: jerarquía zona/pasillo/rack/posición, ocupación por unidad almacenada, estados con texto e indicador, selección de posición y consulta IPC del detalle; aceptación visual pendiente |
| RF-10 Historial de movimientos | Consultar cronología por unidad con fecha, tipo, origen, destino, fuente y resultado | `GetMovementHistory`, DTO `MovementHistoryEntry`, repositorio SQLite y comando IPC | ARD-09/10: orden estable, intentos rechazados/duplicados visibles y campos serializados; vista pendiente |
| RF-11 Idempotencia de eventos | Reconocer el identificador ya procesado sin repetir efectos; rechazar su reutilización con otro contenido | `EventOutcome`, clave única, comparación de contenido, `movement_attempts` y transacción SQLite con `BEGIN IMMEDIATE` | ARD-08/09: reintento de evento aceptado devuelve `Duplicate`; conflictos y rechazos persistidos quedan auditados y consumen el ID; validaciones que no llegan a persistencia no consumen el ID |
| RF-12 Validación de etiquetas, ubicaciones y transiciones | Rechazar datos y estados inválidos sin cambios parciales | Validadores de dominio, casos de uso, restricciones SQL y errores IPC | AC-12 y AC-13; pruebas de rechazo y rollback |
| RF-13 Alertas y correcciones trazables | Añadir corrección vinculada sin reescribir eventos; la revisión visual de alertas queda en ARD-14 | Dominio `MovementCorrection`, caso `RecordCorrection`, tabla append-only, historial SQLite y comando IPC | ARD-09/10: vínculo a la misma unidad, motivo, fecha válida y preservación de evento e inventario; vista de alertas pendiente |
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

La interfaz TypeScript/Vite se compiló con `npm run build`; `npm run test:map` pasó con 3 pruebas. En Windows x64, `cargo fmt --check`, `cargo test --manifest-path src-tauri/Cargo.toml` (42 pruebas) y Clippy pasaron sin advertencias. ARD-03 a ARD-09 están respaldadas por módulos, invariantes, persistencia, transacciones, historial y correcciones. ARD-10 registra 17 comandos IPC, DTO JSON `camelCase`, códigos de error estables, consultas de unidad y wrappers tipados. ARD-11 arma el mapa desde `parentId`, deriva ocupación del estado almacenado, muestra ubicaciones inactivas y solicita el detalle con `get_handling_unit`. Tres pruebas cubren orden jerárquico, ocupación, inactividad, huérfanos y doble ocupación. Tauri inició con una base temporal limpia; la interacción visual de aceptación, formularios, flujos operativos, búsqueda, alertas y simulador siguen pendientes.

La ventana inicial fue observada como evidencia de ARD-02. El perfil local existente presenta un checksum de migración distinto; se dejó intacto y se aisló la prueba de arranque con una base temporal limpia. La aceptación de flujos completos, integración visual, medición de rendimiento y uso real en bodega siguen pendientes.
