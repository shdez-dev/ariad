# ARD-21: medición local de capacidad y latencia

Fecha de ejecución Unix (ms): 1794423775172
Sistema: windows x86_64
Hardware: 12th Gen Intel(R) Core(TM) i5-12450H; 8 núcleos, 12 procesadores lógicos; 31,8 GiB de memoria visible
Revisión Git: 3acb83948047
Compilador: rustc 1.99.1 (58cf8f9c3 2026-10-13)
Paralelismo lógico disponible: 12
SQLite: 3.51.3
Rust crate: Ariadna 0.1.0
Base sintética: `ariadna-ard21-1794423748982365800.sqlite3`

Conjunto: 500 unidades (250 almacenadas y 250 despachadas), 255 ubicaciones (252 posiciones), 25 productos y 10000 eventos de movimiento aceptados. El generador usa una semilla fija implícita en los identificadores, cantidades, lotes y marcas de tiempo. Se registran 101 muestras por operación después de 10 calentamientos; p95 usa el rango de vecino más cercano.
La búsqueda fija `SKU-000` devuelve 21 coincidencias. El historial de `unit-000000` contiene 20 registros. El movimiento alterna `unit-000250` entre `position-000` y `position-250`.

Las duraciones miden la llamada al caso de uso Rust y su repositorio SQLite, excluyendo interfaz, serialización IPC y renderizado. El resultado corresponde solo a este equipo y configuración local; no representa un SLA ni una medición de bodega real.

| Operación | p50 (ms) | p95 (ms) | Meta p95 | Muestras |
| --- | ---: | ---: | --- | ---: |
| Búsqueda transversal de SKU-000 | 6.005 | 10.057 | Cumple | 101 |
| Historial completo de unit-000000 | 0.389 | 0.565 | Cumple | 101 |
| Movimiento SQLite de una unidad almacenada | 2.002 | 3.346 | Cumple | 101 |
