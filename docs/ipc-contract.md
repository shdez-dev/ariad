# Contrato IPC de Ariadna

## Propósito y límites

Este módulo define el puente privado entre la interfaz TypeScript y los casos de uso Rust mediante comandos IPC de Tauri v2. Los comandos delegan las reglas al dominio y a la capa de aplicación; no duplican reglas en la interfaz. Ariadna no inicia una API HTTP ni abre un puerto de negocio.

La interfaz llama a las funciones exportadas en `src/lib/ipc.ts`. Rust registra los comandos en `src-tauri/src/lib.rs` y adapta las solicitudes en `src-tauri/src/ipc.rs`. La aplicación comparte el repositorio SQLite mediante el estado administrado de Tauri.

## Convenciones de datos

- Los nombres JSON de argumentos y respuestas usan `camelCase`.
- `LocationKind` usa las cadenas `zone`, `aisle`, `rack` o `position`.
- `HandlingUnit.status` usa `received`, `stored` o `dispatched`.
- Los campos opcionales se devuelven como `null`.
- La cantidad es un entero positivo.
- Los tiempos del historial son milisegundos desde Unix UTC.
- La interfaz crea cada `eventId` y `occurredAtMs` una sola vez con `newEventMetadata()` y conserva ambos valores al reintentar la misma intención. El tiempo es Unix UTC en milisegundos. Si un intento de negocio llega a persistirse como rechazado, su identificador queda consumido y la nueva operación requiere otro ID. Una validación previa a la persistencia no consume el identificador.
- Rust asigna la fuente `desktop_ui`. La interfaz no puede falsificarla. El tiempo de ocurrencia forma parte del contenido idempotente y por eso se conserva junto con el ID durante reintentos.
- Una corrección requiere `correctionId`, `handlingUnitId`, `correctsEventId` y `reason`. Es un registro de auditoría y no cambia el inventario actual.

## Comandos registrados

| Comando | Argumentos principales | Respuesta |
| --- | --- | --- |
| `create_product` | `sku`, `description`, `unitOfMeasure` | Producto creado y normalizado |
| `get_product` | `sku` | Producto o `null` |
| `list_products` | `includeInactive` | Lista de productos |
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

Las búsquedas combinadas por SKU, etiqueta y ubicación, las alertas y sus flujos visuales siguen en ARD-14. Este incremento incorpora la lectura de unidad por ID o etiqueta para que ARD-12 pueda mostrar detalles sin consultar SQLite desde TypeScript.

## Errores

Cuando una operación falla, la promesa de `invoke` se rechaza con `{ code, message }`. `AriadnaIpcError` adapta ese rechazo para que la interfaz pueda mostrar el mensaje seguro y tomar decisiones por código. Los códigos estables incluyen `validation`, `conflict`, `not_found`, `event_id_conflict`, `inactive_product`, `invalid_transition`, `invalid_location_hierarchy`, `location_hierarchy_immutable`, `in_use`, `position_occupied`, `invalid_destination`, `correction_reference_mismatch`, `correction_before_original` y `storage`.

Los detalles internos, como errores SQLite, se escriben en `logs/ariadna.log`; no se incluyen en los mensajes IPC. Una operación rechazada no confirma cambios parciales.

Después de crear, actualizar o desactivar una ubicación, y después de una recepción, asignación inicial, traslado o despacho confirmados, el wrapper emite el evento de ventana `ariadna:inventory-changed`. Las vistas que dependen de la jerarquía o la ocupación, como el mapa lógico de ARD-11, lo escuchan para volver a consultar ubicaciones y unidades persistidas. El evento no se emite si el comando falla. Los cambios de productos no modifican la ocupación ni requieren refrescar el mapa.

## Verificación

- Pruebas Rust para búsqueda de unidades por ID o etiqueta, líneas, estado, orden y filtrado de despachadas.
- Pruebas de serialización de DTO, enums, nombres camelCase y forma estable de errores.
- `npm run build` comprueba que los consumidores TypeScript usan el contrato sin tipos contradictorios.
- `cargo test` y `cargo clippy --all-targets -- -D warnings` comprueban el puente y el núcleo Rust.
