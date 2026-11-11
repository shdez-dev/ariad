# Contrato IPC de Ariadna

## Propósito y límites

Este módulo define el puente privado entre la interfaz TypeScript y los casos de uso Rust mediante comandos IPC de Tauri v2. Los comandos delegan las reglas al dominio y a la capa de aplicación; no duplican reglas en la interfaz. Ariadna no inicia una API HTTP ni abre un puerto de negocio.

La interfaz llama a las funciones exportadas en `src/lib/ipc.ts`. Rust registra los comandos en `src-tauri/src/lib.rs` y adapta las solicitudes en `src-tauri/src/ipc.rs`. La aplicación comparte el repositorio SQLite mediante el estado administrado de Tauri.

## Convenciones de datos

- Los nombres JSON de argumentos y respuestas usan `camelCase`.
- `LocationKind` usa las cadenas `zone`, `aisle`, `rack` o `position`.
- `HandlingUnit.status` usa `received`, `stored` o `dispatched`.
- Los resultados de `search_inventory` usan `product`, `handling_unit` o `location` y se calculan en Rust desde las listas persistidas.
- `run_simulation` acepta `seed` y `scenario`: `normal`, `duplicate`, `unknown_label`, `invalid_location` u `occupied_destination`. Retorna pasos ordenados con `id`, `label`, `result`, `message` y las referencias `recordId`, `eventId`, `handlingUnitId` o `alertId` cuando correspondan.
- Las alertas usan estado `open`, `reviewed` o `resolved`; `mark_alert_reviewed` cambia `open` a `reviewed` y repetirlo devuelve el estado ya persistido.
- Los campos opcionales se devuelven como `null`.
- La cantidad es un entero positivo.
- Los tiempos del historial son milisegundos desde Unix UTC.
- La interfaz crea cada `eventId` y `occurredAtMs` una sola vez con `newEventMetadata()` y conserva ambos valores al reintentar la misma intención. El tiempo es Unix UTC en milisegundos. Si un intento de negocio llega a persistirse como rechazado, su identificador queda consumido y la nueva operación requiere otro ID. Una validación previa a la persistencia no consume el identificador.
- Rust asigna la fuente `desktop_ui`. La interfaz no puede falsificarla. El tiempo de ocurrencia forma parte del contenido idempotente y por eso se conserva junto con el ID durante reintentos.
- El simulador asigna la fuente `simulator` dentro de Rust. La semilla fija los identificadores de evento y marcas de tiempo; los registros resultantes se guardan mediante los mismos casos de uso y tablas que las operaciones de escritorio.
- Una alerta contiene `recordId` y `targetKind`. La migración 0003 establece una referencia exclusiva con clave foránea a `movement_events` o `movement_attempts`. En alertas por intento, `movementEventId` es `null`, `movementAttemptId` identifica la fila y `eventId` conserva el ID de solicitud compartido por el historial. El campo exacto para navegar es `recordId`.
- Una corrección requiere `correctionId`, `handlingUnitId`, `correctsEventId` y `reason`. Es un registro de auditoría y no cambia el inventario actual.

## Comandos registrados

| Comando | Argumentos principales | Respuesta |
| --- | --- | --- |
| `create_product` | `sku`, `description`, `unitOfMeasure` | Producto creado y normalizado |
| `get_product` | `sku` | Producto o `null` |
| `list_products` | `includeInactive` | Lista de productos |
| `search_inventory` | `query` | Coincidencias parciales de productos, unidades y ubicaciones activas |
| `update_product` | `sku`, `description`, `unitOfMeasure` | Producto actualizado |
| `deactivate_product` | `sku` | Producto desactivado |
| `create_location` | `id`, `code`, `name`, `kind`, `parentId` | Ubicación creada |
| `get_location` | `id` | Ubicación o `null` |
| `list_locations` | `includeInactive` | Lista jerárquica de ubicaciones |
| `update_location` | `id`, `code`, `name` | Ubicación actualizada |
| `deactivate_location` | `id` | Ubicación desactivada |
| `get_handling_unit` | `identifier` (ID o etiqueta) | Unidad con líneas o `null` |
| `list_handling_units` | `includeDispatched` | Unidades ordenadas por etiqueta |
| `receive_unit` | `id`, `label`, `lines`, `eventId`, `occurredAtMs` | `eventId` y resultado `applied` o `duplicate` |
| `store_unit` | `eventId`, `handlingUnitId`, `destinationLocationId`, `occurredAtMs` | Asignación inicial de posición y resultado |
| `move_unit` | `eventId`, `handlingUnitId`, `originLocationId`, `destinationLocationId`, `occurredAtMs` | `eventId` y resultado |
| `dispatch_unit` | `eventId`, `handlingUnitId`, `originLocationId`, `occurredAtMs` | `eventId` y resultado |
| `record_movement_correction` | `correctionId`, `handlingUnitId`, `correctsEventId`, `reason`, `occurredAtMs` | `correctionId` y resultado |
| `get_movement_history` | `handlingUnitId` | Cronología de eventos, intentos y correcciones |
| `list_alerts` | Ninguno | Alertas con movimiento y unidad relacionados |
| `mark_alert_reviewed` | `alertId` | Alerta con estado persistido tras la revisión |
| `run_simulation` | `seed`, `scenario` | Secuencia sintética de operaciones y referencias a eventos, intentos y alertas |

`search_inventory` consulta el catálogo mediante repositorios SQLite y aplica una coincidencia parcial sin distinguir mayúsculas. El SKU puede devolver el producto y las unidades que contienen ese producto; los códigos/IDs de ubicación y etiquetas/IDs de unidad también se pueden buscar. La interfaz no filtra listas parciales ni consulta SQLite directamente.

Las alertas pueden apuntar a un movimiento aceptado o a un intento duplicado/rechazado. Las alertas de escenarios simulados se generan dentro de la misma transacción que registra el intento, por lo que nunca quedan asociadas a un movimiento aceptado no relacionado. La bandeja abre el `recordId` exacto; si la referencia no tiene ficha de unidad, muestra su historial persistido.

## Errores

Cuando una operación falla, la promesa de `invoke` se rechaza con `{ code, message }`. `AriadnaIpcError` adapta ese rechazo para que la interfaz pueda mostrar el mensaje seguro y tomar decisiones por código. Los códigos estables incluyen `validation`, `conflict`, `not_found`, `event_id_conflict`, `inactive_product`, `invalid_transition`, `invalid_location_hierarchy`, `location_hierarchy_immutable`, `in_use`, `position_occupied`, `invalid_destination`, `correction_reference_mismatch`, `correction_before_original` y `storage`.

Los códigos estables incluyen `invalid_alert_transition` además de los errores de validación, conflicto, referencias faltantes, transiciones, jerarquía, ocupación, corrección y almacenamiento. Los detalles internos, como errores SQLite, se escriben en `logs/ariadna.log`; no se incluyen en los mensajes IPC. Una operación rechazada no confirma cambios parciales.

Después de crear, actualizar o desactivar una ubicación, y después de una recepción, asignación inicial, traslado o despacho confirmados, el wrapper emite el evento de ventana `ariadna:inventory-changed`. Las vistas que dependen de la jerarquía o la ocupación, como el mapa lógico de ARD-11, lo escuchan para volver a consultar ubicaciones y unidades persistidas. El evento no se emite si el comando falla. Los cambios de productos no modifican la ocupación ni requieren refrescar el mapa.

Marcar una alerta como revisada no cambia el inventario y no emite `ariadna:inventory-changed`. La vista actualiza la tarjeta con la respuesta persistida del comando.

## Verificación

- Pruebas Rust para búsqueda parcial de productos, etiquetas/IDs de unidad y ubicaciones, además de búsqueda de SKU en las líneas de unidades.
- Pruebas Rust para alertas vinculadas a eventos o intentos, transición `open` a `reviewed`, migración, idempotencia y clave exacta.
- Pruebas Rust para los cinco escenarios, semilla reproducible, origen sintético y vínculo entre anomalía, intento y alerta.
- La cronología existente cubre eventos aceptados, intentos duplicados/rechazados y correcciones en orden estable.
- Pruebas de serialización de DTO, enums, nombres camelCase y forma estable de errores.
- `npm run build` comprueba que los consumidores TypeScript usan el contrato sin tipos contradictorios.
- `cargo test` y `cargo clippy --all-targets -- -D warnings` comprueban el puente y el núcleo Rust.
