# Ariadna: matriz de trazabilidad

Estado: línea base de implementación, 9 de noviembre de 2026. Esta matriz interpreta los requisitos de AETHER a la luz de la arquitectura vigente. Los casos de prueba que aparecen aquí son evidencia planificada, no resultados ejecutados.

## Decisiones técnicas que resuelven pendientes

1. La aplicación es un monolito modular de escritorio con Rust estable y Tauri v2.
2. La interfaz se comunica con la capa de aplicación mediante IPC de Tauri. No se publica una API HTTP, no se abre un puerto de backend y no se integra un WMS, ERP o lector físico.
3. SQLite local con SQLx es la persistencia del prototipo. Las migraciones SQL versionan el esquema.
4. El simulador llama los mismos casos de uso de aplicación que usa la interfaz. El contrato del evento es interno y tipado; no requiere HTTP, MQTT ni un servicio externo.
5. Se usan ubicaciones configuradas manualmente y datos sintéticos. No se afirma que el simulador represente lecturas RFID reales.

Estas decisiones concretan ADR-01 a ADR-07 y DEC-04 a DEC-08. Las versiones 0.2 de Visión y alcance y Requisitos y criterios de aceptación, junto con la versión 0.3 de Arquitectura y decisiones técnicas, registran estas elecciones. Esta matriz conserva la correspondencia entre requisitos, casos de uso, componentes, tarjetas y evidencia prevista. Documento AETHER vinculado: Matriz de trazabilidad de requisitos, ID 3a65eb00-0457-4e44-be52-80739fcfc560.

## Requisitos funcionales

| Requisito | Caso de uso o flujo | Componentes previstos | Criterio y evidencia prevista |
| --- | --- | --- | --- |
| RF-01 Configuración de ubicaciones | Crear zona, pasillo, rack y posición; validar código y padre | Dominio `Location`, servicio de ubicaciones, repositorio SQLite, formulario de configuración | AC-01; pruebas de jerarquía, código único y padre válido |
| RF-02 Catálogo de productos | Crear y consultar SKU sin duplicados | Dominio `Product`, servicio de catálogo, repositorio SQLite, pantalla de productos | AC-02; pruebas de alta, consulta y conflicto de SKU |
| RF-03 Registro de pallet o contenedor | Crear unidad de manipulación con etiqueta única y líneas positivas | Dominio `HandlingUnit` y `HandlingUnitLine`, servicio de unidades, persistencia SQLite | AC-03; pruebas de etiqueta duplicada, SKU inexistente y cantidades inválidas |
| RF-04 Recepción | Recibir una unidad y dejarla pendiente de ubicación | Caso de uso `receive_unit`, historial, transacción SQLite, comando IPC | AC-04; prueba de recepción válida, repetida y rollback |
| RF-05 Asignación y traslado | Asignar posición libre o mover entre posiciones libres | Caso de uso `assign_location`/`move_unit`, reglas de ocupación, transacción, IPC | AC-05 y AC-06; pruebas de posición ocupada, inactiva y movimiento válido |
| RF-06 Ingesta de eventos simulados | El simulador entrega un evento tipado directamente a `process_event`; se devuelve aceptado, duplicado o rechazado | Simulador en proceso, capa de aplicación, dominio, repositorios SQLite; IPC solo para la acción de interfaz que inicia el escenario | AC-06, AC-11 a AC-13 y escenarios de RF-14; pruebas deterministas. Sin API HTTP, MQTT ni lector real |
| RF-07 Despacho | Despachar una unidad almacenada, liberar su posición y preservar el evento | Caso de uso `dispatch_unit`, dominio, historial y transacción SQLite, IPC | AC-07; pruebas de despacho único y de transición posterior inválida |
| RF-08 Consulta de inventario | Buscar por identificador, etiqueta, SKU, zona o estado | Consultas de aplicación, repositorios SQLite e interfaz de búsqueda | AC-08; pruebas de filtros, resultados y estado vacío |
| RF-09 Vista del mapa lógico | Mostrar jerarquía y ocupación registrada, sin inferencia por sensores | Consulta de ubicaciones/ocupación y componente de mapa de la interfaz | AC-09; comprobación de posiciones libres, ocupadas e inactivas, con texto además de color |
| RF-10 Historial de movimientos | Consultar cronología con fecha, tipo, origen, destino, fuente y resultado | Entidad append-only `MovementEvent`, consulta de historial e interfaz | AC-10; prueba de orden cronológico y campos visibles |
| RF-11 Idempotencia de eventos | Reconocer el identificador ya procesado sin repetir efectos | Clave única del evento, caso de uso `process_event`, transacción SQLite | AC-11; prueba de reenvío sin cambio de inventario ni duplicación del movimiento |
| RF-12 Validación de etiquetas, ubicaciones y transiciones | Rechazar datos y estados inválidos sin cambios parciales | Validadores de dominio, casos de uso, restricciones SQL y errores IPC | AC-12 y AC-13; pruebas de rechazo y rollback |
| RF-13 Alertas y correcciones trazables | Revisar alertas y añadir corrección vinculada sin reescribir eventos | Dominio `Alert`/`MovementEvent`, casos de uso de revisión/corrección, historial SQLite, interfaz | AC-14; prueba de corrección enlazada y evento original preservado |
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

La interfaz TypeScript/Vite y la aplicación nativa Tauri se compilaron correctamente; la ventana inicial fue observada en Windows x64. En este entorno, Smart App Control bloqueó binarios auxiliares de Cargo en el primer intento y después se ajustó el entorno de desarrollo. ARD-02 queda verificada para el arranque. Las pruebas de reglas, persistencia, flujos de inventario y rendimiento siguen pendientes; esta evidencia no representa aceptación funcional del inventario.
