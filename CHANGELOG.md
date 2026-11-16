# Registro de cambios

Este archivo registra el alcance de las versiones de demostración de Ariadna. La evidencia detallada de implementación, pruebas y límites se mantiene en docs/trazabilidad.md.

## 0.1.0 — borrador de demostración local

Estado: borrador documental. La versión 0.1.0 está declarada en los manifiestos de frontend y Tauri, pero todavía no tiene etiqueta Git ni aceptación completa como candidata. No se debe interpretar este borrador como aprobación para producción.

### Capacidades incluidas

- Aplicación de escritorio local construida con Tauri 2, interfaz web y lógica de dominio en Rust.
- Persistencia del catálogo, ubicaciones, unidades de manipulación y movimientos mediante SQLite local.
- Mapa lógico configurable de zonas, pasillos, racks y posiciones, con ocupación derivada de las unidades almacenadas.
- Administración de productos y ubicaciones, y consulta de unidades y sus líneas.
- Flujos de recepción, asignación de posición, traslado y despacho, con validaciones en Rust y persistencia transaccional.
- Búsqueda de productos, unidades y ubicaciones; cronología de movimientos y bandeja de alertas.
- Simulador determinista con escenarios sintéticos normales y anómalos que recorren los mismos casos de uso.
- Guía de operación, copia y restauración; la restauración automatizada se prueba con una base temporal aislada.

### Límites conocidos

- El mapa representa una estructura lógica configurada manualmente. No escanea ni mapea físicamente una bodega.
- No se incluyen robots, lectores RFID, sensores ni otro hardware real.
- No se incluyen integraciones con ERP o WMS, servicio remoto multiusuario ni despliegue productivo.
- Los escenarios y registros de demostración son sintéticos. No se afirma que el programa haya sido validado en una operación real.
- La aceptación de escritorio de ARD-11 a ARD-15 y el cierre de ARD-25 siguen pendientes de revisión criterio por criterio en una candidata vigente, incluida la revisión de teclado y accesibilidad.
- La restauración manual del perfil real de Windows no se ha ejecutado. La evidencia automatizada corresponde a una base SQLite temporal.
- MR-05, beneficio operativo, requiere una línea base y un piloto autorizado; no se reporta ahorro ni mejora operacional real.

### Cierre de la versión

No crear una etiqueta de aceptación ni presentar 0.1.0 como versión final hasta completar la aceptación pendiente, registrar los defectos y riesgos abiertos con su impacto, mitigación y responsable, y conservar la evidencia correspondiente en docs/trazabilidad.md y AETHER.
